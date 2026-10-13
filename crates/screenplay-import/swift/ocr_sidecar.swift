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
    var layout = false
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
        case "--layout":
            cfg.layout = true; i += 1
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
    observations.sort { a,b in
        abs(a.boundingBox.midY-b.boundingBox.midY)>0.003 ? a.boundingBox.midY>b.boundingBox.midY : a.boundingBox.minX<b.boundingBox.minX
    }

    var out: [(Double, String)] = []
    let left=observations.map{$0.boundingBox.minX}.min() ?? 0
    var previous:CGRect?=nil
    for obs in observations {
        if let t = obs.topCandidates(1).first {
            if cfg.layout {
                let number=t.string.trimmingCharacters(in:.whitespaces)
                if number.count<=4 && !number.isEmpty && number.allSatisfy({$0.isNumber}) && (obs.boundingBox.midY<0.08 || obs.boundingBox.midY>0.92){continue}
                if let prev=previous,prev.minY-obs.boundingBox.maxY>max(obs.boundingBox.height*0.9,0.012){out.append((1.0,""))}
                let spaces=Int(max(0,min(60,(obs.boundingBox.minX-left)*100)))
                out.append((Double(t.confidence),String(repeating:" ",count:spaces)+t.string));previous=obs.boundingBox
            } else {out.append((Double(t.confidence), t.string))}
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

// PDFKit decodes embedded fonts; retain reading order, indentation and inline bold.
func pdfLayoutLines(_ page:PDFPage) -> [(Double,String)] {
    let bounds=page.bounds(for:.mediaBox)
    guard let selection=page.selection(for:bounds) else{return []}
    let lines=selection.selectionsByLine().filter{!($0.string ?? "").trimmingCharacters(in:.whitespacesAndNewlines).isEmpty}.sorted{a,b in
        let x=a.bounds(for:page),y=b.bounds(for:page)
        return abs(x.midY-y.midY)>2 ? x.midY>y.midY : x.minX<y.minX
    }
    let left=lines.map{$0.bounds(for:page).minX}.min() ?? bounds.minX
    var result:[(Double,String)]=[],previous:CGRect?=nil
    for line in lines {
        let box=line.bounds(for:page),text=(line.string ?? "").trimmingCharacters(in:.whitespacesAndNewlines)
        if text.count<=4 && text.allSatisfy({$0.isNumber}) && (box.midY<bounds.minY+bounds.height*0.08 || box.midY>bounds.maxY-bounds.height*0.08){continue}
        let attributed=line.attributedString
        let font:NSFont?=attributed.flatMap{$0.length>0 ? $0.attribute(.font,at:0,effectiveRange:nil) as? NSFont : nil}
        let size=font?.pointSize ?? 12
        if let prev=previous,prev.minY-box.maxY>max(size*0.9,8){result.append((1.0,""))}
        var marked=""
        if let attributed=attributed,attributed.length>0 {
            var lastBold=false
            attributed.enumerateAttribute(.font,in:NSRange(location:0,length:attributed.length)){value,range,_ in
                let part=(attributed.string as NSString).substring(with:range).replacingOccurrences(of:"\n",with:"")
                let bold=(value as? NSFont)?.fontDescriptor.symbolicTraits.contains(.bold) ?? false
                let strong=bold && !part.trimmingCharacters(in:.whitespaces).isEmpty
                if strong != lastBold {marked += "**";lastBold=strong}
                marked += part
            }
            if lastBold {marked += "**"}
        }
        if marked.isEmpty{marked=text}
        let spaces=Int(max(0,min(60,((box.minX-left)/6).rounded())))
        result.append((1.0,String(repeating:" ",count:spaces)+marked.trimmingCharacters(in:.whitespacesAndNewlines)));previous=box
    }
    return result
}

// Some PDF exporters supply valid Unicode but invalid glyph geometry (notably
// zero-height parenthetical glyphs). Sorting that layer relocates whole speeches.
func pdfTextGeometryIsReliable(_ page:PDFPage) -> Bool {
    guard let text=page.string else{return false}
    let value=text as NSString
    var letters=0,invalid=0
    for i in 0..<value.length {
        let unit=value.character(at:i)
        // Avoid splitting surrogate pairs when inspecting the UTF-16 PDF index.
        guard let scalar=UnicodeScalar(Int(unit)),CharacterSet.letters.contains(scalar) else{continue}
        letters += 1
        let box=page.characterBounds(at:i)
        if !box.minX.isFinite || !box.minY.isFinite || box.height<0.5 || box.width<0.1 {invalid += 1}
    }
    return letters>=8 && (invalid<16 || invalid*100<=letters*15)
}

// OCR supplies geometry; embedded PDF text supplies spelling. Do not rewrite
// an OCR line unless its closest original line is an unambiguous near match.
func restorePdfSpelling(_ lines:[(Double,String)],_ page:PDFPage)->[(Double,String)] {
 var originals=Array(Set((page.string ?? "").components(separatedBy:.newlines).map{$0.trimmingCharacters(in:.whitespacesAndNewlines)}.filter{!$0.isEmpty}))
 var completed:[String:String]=[:]
 let open=originals.filter{($0.hasPrefix("(") || $0.hasPrefix("（")) && !$0.hasSuffix(")") && !$0.hasSuffix("）")}
 let tails=originals.filter{!$0.contains("(") && !$0.contains("（") && ($0.hasSuffix(")") || $0.hasSuffix("）")) && $0.count<=20}
 if open.count==1 && tails.count==1 {completed[open[0]]=open[0]+tails[0];originals.append(open[0]+tails[0])}
 func key(_ s:String)->[Character]{Array(s.lowercased().filter{!$0.isWhitespace && !$0.isPunctuation})}
 func distance(_ a:[Character],_ b:[Character])->Int {
  var previous=Array(0...b.count)
  for (i,c) in a.enumerated(){var row=[i+1];for (j,d) in b.enumerated(){row.append(min(row[j]+1,previous[j+1]+1,previous[j]+(c==d ? 0:1)))};previous=row}
  return previous.last ?? a.count
 }
 let candidates=originals.map{($0,key($0))}
 var result:[(Double,String)]=[];var i=0
 while i<lines.count {
  let (confidence,line)=lines[i],stripped=line.trimmingCharacters(in:.whitespaces),a=key(stripped)
  let indent=String(line.prefix{ $0==" " || $0=="\t" })
  if a.count<=6 && !a.isEmpty && i+1<lines.count {
   let next=lines[i+1].1,nextIndent=String(next.prefix{$0==" " || $0=="\t"})
   let combined=a+key(next)
   let matches=candidates.filter{$0.1==combined}
   if abs(indent.count-nextIndent.count)<=6 && matches.count==1 {result.append((confidence,indent+matches[0].0));i+=2;continue;}
  }
  if a.isEmpty{result.append((confidence,line));i+=1;continue;}
  let ranked=candidates.filter{abs($0.1.count-a.count)<=max(2,a.count/4)}.map{($0.0,distance(a,$0.1))}.sorted{$0.1<$1.1}
  if let first=ranked.first,first.1<=max(1,a.count/4),ranked.count<2 || first.1<ranked[1].1 {
   let restored=completed[first.0] ?? first.0
   result.append((confidence,indent+restored))
   if completed[first.0] != nil && i+1<lines.count && key(lines[i+1].1)==key(tails[0]) {i+=1}
  }else{result.append((confidence,line))}
  i+=1
 }
 return result
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
            if cfg.textFirst && (!cfg.layout || pdfTextGeometryIsReliable(page)) && text.unicodeScalars.filter({CharacterSet.letters.contains($0)}).count >= 8 && !text.contains("�") {
                if cfg.layout {let structured=pdfLayoutLines(page);if !structured.isEmpty{return structured}}
                return text.components(separatedBy: .newlines).filter{!$0.trimmingCharacters(in: .whitespaces).isEmpty}.map{(1.0,$0)}
            }
            if cfg.textFirst && cfg.layout {FileHandle.standardError.write("INFO page \(i+1): unreliable PDF text geometry; using OCR\n".data(using:.utf8)!)}
            guard let image = renderPdfPage(page,cfg.pdfDpi) else {return []}
            let recognized=try ocrImage(image,cfg)
            return cfg.textFirst && cfg.layout ? restorePdfSpelling(recognized,page) : recognized
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
