// pdf_sidecar.swift —— 用 macOS CoreText 生成 PDF（保证 CJK 正确嵌入）。
//
// 为什么需要 sidecar：
//   printpdf 的内置字体（Courier/Helvetica）是 ASCII-only，嵌入中文必然乱码。
//   printpdf 0.8 的 `ParsedFont` 没有公开构造器，自己用 ttf-parser 解析
//   CJK 字形既脆弱又难维护。CoreText 是 macOS 系统字体栈的官方接口，
//   能正确处理中文字形嵌入、字体回退、断行，且无需分发字体文件
//   （符合 Apple 平台规范）。
//
// 调用约定（与 Rust 侧 `pdf.rs::run_pdf_sidecar` 对应）：
//   stdin  接收 JSON：{ pages: [ { lines: [ { text, x, y, size, bold } ] } ], width, height }
//   stdout 写出 PDF 字节
//   stderr 写诊断信息
//
// 坐标约定：Rust 侧已经把「剧本行」算成绝对坐标（点，PDF 原点在左下），
// Swift 侧只负责渲染，不做分页计算 —— 保证与 GUI 纸面、PDF 完全一致。

import Foundation
import CoreText
import AppKit

// MARK: - 输入模型

struct Line: Decodable {
    let text: String
    let x: Double
    let y: Double
    let size: Double
    let bold: Bool
    let role: String?
    let font_family: String?
    let italic: Bool?
    let color: String?
}

struct Page: Decodable {
    let lines: [Line]
}

struct Spec: Decodable {
    let pages: [Page]
    let width: Double
    let height: Double
    let typography: String?
}

// MARK: - 字体栈
//
// 剧本是「等宽」文体，但中文必须用可显示汉字的字体。
// 策略：优先用系统等宽 CJK（Menlo 回退到苹方/黑体），保证：
//   - 英文数字仍是等宽（剧本格式要求）
//   - 中文能正常显示（不出现豆腐块）
// 粗体用 Menlo Bold / 苹方 Semibold。

func pickFont(size: CGFloat, bold: Bool) -> CTFont {
    let candidates: [(String, CGFloat)] = bold
        ? [("Menlo-Bold", size), ("PingFangSC-Semibold", size), ("STHeitiSC-Medium", size)]
        : [("Menlo-Regular", size), ("PingFangSC-Regular", size), ("STHeitiSC-Light", size)]
    for (name, sz) in candidates {
        let f = CTFontCreateWithName(name as CFString, sz, nil)
        // CTFontCreateWithName 对未知名字会返回 LastResort 字体；
        // 通过 PostScript 名回查确认是否真的存在。
        let actual = CTFontCopyPostScriptName(f) as String
        if !actual.isEmpty && actual != "LastResort" {
            return f
        }
    }
    return CTFontCreateWithName("Helvetica" as CFString, size, nil)
}

// MARK: - 渲染

func makePDF(spec: Spec) -> Data? {
    let mediaBox = CGRect(x: 0, y: 0,
                          width: spec.width,
                          height: spec.height)
    // 必须持有 NSMutableData 的强引用，CGDataConsumer 不会 retain 它。
    let outData = NSMutableData()
    guard let consumer = CGDataConsumer(data: outData) else { return nil }
    var mediaBoxVar = mediaBox
    guard let ctx = CGContext(consumer: consumer, mediaBox: &mediaBoxVar, nil) else {
        return nil
    }

    for page in spec.pages {
        ctx.beginPDFPage(nil)
        // CoreGraphics 的 PDF 原点在左下，与 Rust 侧传入的 y 一致。
        for line in page.lines {
            let size = CGFloat(line.size)
            let chinese = spec.typography == "chinese-reading"
            var font = chinese ? CTFontCreateWithName((line.role == "character" ? "PingFangSC-Semibold" : "STSongti-SC-Regular") as CFString, size, nil) : pickFont(size: size, bold: line.bold)
            if let family = line.font_family {
                let name = family.components(separatedBy: ",")[0].trimmingCharacters(in: CharacterSet(charactersIn: " \"'"))
                font = CTFontCreateWithName(name as CFString, size, nil)
                var traits: CTFontSymbolicTraits = []
                if line.bold { traits.insert(.traitBold) }
                if line.italic == true { traits.insert(.traitItalic) }
                if let variant = CTFontCreateCopyWithSymbolicTraits(font, size, nil, traits, [.traitBold, .traitItalic]) { font = variant }
            }
            if line.italic == true && !CTFontGetSymbolicTraits(font).contains(.traitItalic) {
                var shear = CGAffineTransform(a: 1, b: 0, c: 0.2, d: 1, tx: 0, ty: 0)
                font = CTFontCreateCopyWithAttributes(font, size, &shear, nil)
            }
            var color = NSColor.black
            if let hex = line.color, let rgb = UInt32(hex.dropFirst(), radix: 16) {
                color = NSColor(red: CGFloat((rgb >> 16) & 255) / 255, green: CGFloat((rgb >> 8) & 255) / 255, blue: CGFloat(rgb & 255) / 255, alpha: 1)
            }
            let attributed = NSMutableAttributedString(
                string: line.text,
                attributes: [
                    .font: font,
                    .foregroundColor: color,
                ]
            )

            if line.font_family != nil && line.bold && !CTFontGetSymbolicTraits(font).contains(.traitBold) {
                attributed.addAttribute(.strokeWidth, value: -2.5, range: NSRange(location: 0, length: attributed.length))
            }
            if chinese && line.font_family == nil {
                if line.bold && line.role != "character" {
                    attributed.addAttribute(.font, value: CTFontCreateWithName("STSongti-SC-Bold" as CFString, size, nil), range: NSRange(location: 0, length: attributed.length))
                }
                if line.role == "character", let regex = try? NSRegularExpression(pattern: "[（(](?:画外音|画外|旁白|离画|电话音|V\\.O\\.?|O\\.S\\.?|OFF|CONT['’]?D)(?:[,，]\\s*CONT['’]?D)?[）)]$", options: .caseInsensitive), let match = regex.firstMatch(in: line.text, range: NSRange(location: 0, length: attributed.length)) {
                    attributed.addAttribute(.font, value: CTFontCreateWithName("STSongti-SC-Regular" as CFString, size * 0.85, nil), range: match.range)
                }
                for prefix in ["镜头切换。", "镜头切换.", "蒙太奇。", "蒙太奇."] where line.text.hasPrefix(prefix) {
                    attributed.addAttribute(.font, value: CTFontCreateWithName("STSongti-SC-Bold" as CFString, size, nil), range: NSRange(location: 0, length: (prefix.hasPrefix("蒙太奇") ? 3 : (prefix as NSString).length)))
                }
            }

            // 关键：用 CTLine 而不是 CTFrame。
            //
            // CTFrame 是一个「排版框」：它会按宽度自动换行，并在高度放不下时
            // **静默裁掉**剩余字符（CTFrameGetVisibleStringRange 的 length
            // 小于实际长度），表现为 PDF 里少字但不报错。
            //
            // 但 Rust 侧已经把每一行折好了（复用 pagination::split_to_lines），
            // 这里要做的只是「把这一行原样画在给定基线上」。
            // CTLine 是单行排版结果，不换行、不裁字 —— 零字符丢失。
            let lineRef = CTLineCreateWithAttributedString(attributed)
            let centered = chinese && (line.role == "character" || line.role == "parenthetical")
            var width = CTLineGetTypographicBounds(lineRef, nil, nil, nil)
            if chinese && line.role == "character", let regex = try? NSRegularExpression(pattern: "[（(](?:画外音|画外|旁白|离画|电话音|V\\.O\\.?|O\\.S\\.?|OFF|CONT['’]?D)(?:[,，]\\s*CONT['’]?D)?[）)]$", options: .caseInsensitive), let match = regex.firstMatch(in: line.text, range: NSRange(location: 0, length: attributed.length)) {
                let name = attributed.attributedSubstring(from: NSRange(location: 0, length: match.range.location))
                width = CTLineGetTypographicBounds(CTLineCreateWithAttributedString(name), nil, nil, nil)
            }
            ctx.textPosition = CGPoint(x: centered ? line.x - width / 2 : line.x, y: line.y)
            CTLineDraw(lineRef, ctx)
        }
        ctx.endPDFPage()
    }
    ctx.closePDF()

    return outData as Data
}

// MARK: - main

let inputData = FileHandle.standardInput.readDataToEndOfFile()
guard !inputData.isEmpty else {
    FileHandle.standardError.write("no input\n".data(using: .utf8)!)
    exit(1)
}

do {
    let spec = try JSONDecoder().decode(Spec.self, from: inputData)
    guard let pdf = makePDF(spec: spec) else {
        FileHandle.standardError.write("failed to create pdf\n".data(using: .utf8)!)
        exit(2)
    }
    FileHandle.standardOutput.write(pdf)
    exit(0)
} catch {
    FileHandle.standardError.write(
        "decode/render error: \(error)\n".data(using: .utf8)!)
    exit(3)
}
