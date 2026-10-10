// ocr_sidecar.swift
//
// 幕本 OCR Sidecar：使用 Apple Vision Framework 进行文本识别。
// 支持输入：PNG/JPG 图片、PDF 文档（按页 OCR）。
//
// 用法：
//   ocr_sidecar <file_path> [--langs zh-Hans,en-US] [--level accurate|fast] [--pdf-dpi 200]
//
// 输出（stdout，UTF-8）：
//   PAGE 1
//   0.98\tEXT. BAR - NIGHT
//   0.95\tJOHN
//   0.97\tHello, world.
//   PAGE 2
//   ...
//   END
//
// 错误：写至 stderr，并以非 0 退出码退出。
//
// 由 Rust `screenplay-import` crate 通过 std::process::Command 调用。
// 由 Tauri .app 打包时随 binary 一同分发至 Resources/ocr_sidecar。

import Foundation
import Vision
import AppKit
import PDFKit

struct OcrConfig {
    var path: String = ""
    // 默认中文优先：本产品是中文剧本编辑器，扫描件绝大多数是中文。
    // 与 ScreenplaySettings.ocr_languages 默认值（["zh-Hans", "en-US"]）保持一致。
    var langs: [String] = ["zh-Hans", "en-US"]
    var level: VNRequestTextRecognitionLevel = .accurate
    var pdfDpi: Double = 200
    var textFirst = false
}

func parseArgs() -> OcrConfig? {
    var cfg = OcrConfig()
    let args = CommandLine.arguments
    guard args.count >= 2 else {
        FileHandle.standardError.write("usage: ocr_sidecar <file_path> [--langs ...] [--level accurate|fast] [--pdf-dpi N]\n".data(using: .utf8)!)
        return nil
    }
    cfg.path = args[1]
    var i = 2
    while i < args.count {
        let a = args[i]
        switch a {
        case "--text-first":
            cfg.textFirst = true; i += 1
        case "--langs":
            if i + 1 < args.count {
                cfg.langs = args[i+1].split(separator: ",").map { String($0) }
                i += 2
            } else { i += 1 }
        case "--level":
            if i + 1 < args.count {
                let v = args[i+1].lowercased()
                cfg.level = (v == "fast") ? .fast : .accurate
                i += 2
            } else { i += 1 }
        case "--pdf-dpi":
            if i + 1 < args.count, let d = Double(args[i+1]) {
                cfg.pdfDpi = d
                i += 2
            } else { i += 1 }
        default:
            i += 1
        }
    }
    return cfg
}

/// 对单张 NSImage 同步执行 OCR，返回 [(confidence, text)]
/// 已按阅读顺序（自上而下）排序
func ocrImage(_ image: NSImage, _ cfg: OcrConfig) throws -> [(Double, String)] {
    guard let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
        throw NSError(domain: "OcrSidecar", code: 1, userInfo: [NSLocalizedDescriptionKey: "无法从 NSImage 获取 CGImage"])
    }
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = cfg.level
    request.recognitionLanguages = cfg.langs
    request.usesLanguageCorrection = true

    let handler = VNImageRequestHandler(cgImage: cg, orientation: .up, options: [:])
    // perform(_:) 是同步调用，回调在内部队列触发，避免主队列死锁
    try handler.perform([request])

    var observations: [VNRecognizedTextObservation] = (request.results ?? []) as? [VNRecognizedTextObservation] ?? []
    // Vision 坐标系原点在左下角，origin.y 越大越靠上
    observations.sort { $0.boundingBox.origin.y > $1.boundingBox.origin.y }

    var out: [(Double, String)] = []
    for obs in observations {
        if let t = obs.topCandidates(1).first {
            out.append((Double(t.confidence), t.string))
        }
    }
    return out
}

/// 渲染 PDF 单页为 NSImage（使用 PDFPage.thumbnail 接口）
func renderPdfPage(_ page: PDFPage, _ dpi: Double) -> NSImage? {
    let scale = dpi / 72.0
    let bounds = page.bounds(for: .mediaBox)
    let scaledSize = NSSize(width: bounds.width * scale, height: bounds.height * scale)
    return page.thumbnail(of: scaledSize, for: .mediaBox)
}

func ocrPdf(_ url: URL, _ cfg: OcrConfig) throws -> [[(Double, String)]] {
    guard let doc = PDFDocument(url: url) else {
        throw NSError(domain: "OcrSidecar", code: 2, userInfo: [NSLocalizedDescriptionKey: "PDFDocument 打开失败"])
    }
    var pages: [[(Double, String)]] = []
    for i in 0..<doc.pageCount {
        let lines: [(Double,String)] = try autoreleasepool {
            guard let page = doc.page(at: i) else { return [] }
            let text = page.string ?? ""
            if cfg.textFirst && text.unicodeScalars.filter({CharacterSet.letters.contains($0)}).count >= 8 && !text.contains("�") {
                return text.components(separatedBy: .newlines).filter{!$0.trimmingCharacters(in: .whitespaces).isEmpty}.map{(1.0,$0)}
            }
            guard let image = renderPdfPage(page,cfg.pdfDpi) else {return []}
            return try ocrImage(image,cfg)
        }
        pages.append(lines)
        FileHandle.standardError.write("PROGRESS \(i+1) \(doc.pageCount)\n".data(using:.utf8)!)
    }
    return pages
}

func isPdf(_ url: URL) -> Bool {
    return url.pathExtension.lowercased() == "pdf"
}

func main() -> Int32 {
    guard let cfg = parseArgs() else { return 1 }
    let url = URL(fileURLWithPath: cfg.path)
    guard FileManager.default.fileExists(atPath: cfg.path) else {
        FileHandle.standardError.write("文件不存在: \(cfg.path)\n".data(using: .utf8)!)
        return 2
    }

    do {
        var pages: [[(Double, String)]] = []
        if isPdf(url) {
            pages = try ocrPdf(url, cfg)
        } else {
            guard let img = NSImage(contentsOf: url) else {
                FileHandle.standardError.write("无法加载图片: \(cfg.path)\n".data(using: .utf8)!)
                return 3
            }
            pages = [try ocrImage(img, cfg)]
        }

        var buf = ""
        for (i, page) in pages.enumerated() {
            buf += "PAGE \(i+1)\n"
            for (conf, text) in page {
                buf += String(format: "%.2f\t%@\n", conf, text)
            }
        }
        buf += "END\n"
        FileHandle.standardOutput.write(buf.data(using: .utf8)!)
        return 0
    } catch {
        FileHandle.standardError.write("OCR 失败: \(error.localizedDescription)\n".data(using: .utf8)!)
        return 4
    }
}

exit(main())
