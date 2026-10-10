//! DOCX 导出。
//!
//! 生成 OOXML 文档，包含 `word/document.xml` 与最小必需部件。
//! 剧本块按类型映射为段落样式（场景标题加粗、人物居中、对白缩进）。

use screenplay_core::ast::{BlockType, ScreenplayDocument};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// 将剧本文档导出为 DOCX 字节
pub fn export_docx(doc: &ScreenplayDocument) -> Result<Vec<u8>, anyhow::Error> {
    let chinese = screenplay_core::typography::chinese(&doc.settings);
    let mut body = String::new();
    for block in &doc.blocks {
        let mut text = screenplay_core::typography::display_text(block.block_type, &block.text);
        if let Some(label) = screenplay_core::typography::scene_label(doc, &block.id) { text = format!("{label}\n{text}"); }
        if text.is_empty() { continue; }
        let custom = doc.settings.element_styles.get(block.block_type.as_str());
        let properties = custom.map(|s| format!("<w:rFonts w:ascii=\"{0}\" w:hAnsi=\"{0}\" w:eastAsia=\"{0}\"/><w:sz w:val=\"{1}\"/><w:b w:val=\"{2}\"/><w:i w:val=\"{3}\"/><w:color w:val=\"{4}\"/>", escape_xml(&s.font_family), (s.font_size * 2.0).round(), if block.block_type != BlockType::Parenthetical && s.bold {1} else {0}, if s.italic {1} else {0}, escape_xml(s.color.trim_start_matches('#')))).unwrap_or_default();
        let style = match block.block_type {
            BlockType::SceneHeading => "SceneHeading", BlockType::Character => "Character",
            BlockType::Dialogue => "Dialogue", BlockType::Parenthetical => "Parenthetical",
            BlockType::Transition => "Transition", _ => "Normal",
        };
        for (line_index, line) in text.lines().enumerate() {
            let label_spacing = if line_index == 0 && screenplay_core::typography::scene_label(doc, &block.id).is_some() { "<w:spacing w:after=\"0\"/>" } else { "" };
            let (name, extension) = screenplay_core::typography::character_parts(line);
            let runs = if chinese && block.block_type == BlockType::Character && extension.is_some() {
                format!("<w:r><w:tab/></w:r>{}<w:r><w:tab/></w:r>{}", run(name, &properties), run(extension.unwrap(), &format!("{properties}<w:b w:val=\"0\"/><w:rFonts w:eastAsia=\"宋体\"/><w:sz w:val=\"{}\"/>", (doc.settings.font_size * 0.85 * 2.0).round())))
            } else if chinese && ["镜头切换。", "镜头切换.", "蒙太奇。", "蒙太奇."].iter().any(|p| line.starts_with(p)) {
                let (i, separator) = line.char_indices().find(|(_, c)| *c == '。' || *c == '.').unwrap();
                let n = if line.starts_with("蒙太奇") { i } else { i + separator.len_utf8() };
                format!("{}{}", run(&line[..n], &format!("{properties}<w:b/>")), run(&line[n..], &properties))
            } else { run(line, &properties) };
            let tabs = if chinese && block.block_type == BlockType::Character && extension.is_some() {
                let content = doc.settings.paper.page_width_inch - doc.settings.paper.margin_left_inch - doc.settings.paper.margin_right_inch;
                let center = (content * 720.0).round();
                let right = center + screenplay_core::pagination::text_units(name) * doc.settings.font_size * 5.0 + 60.0;
                format!("<w:jc w:val=\"left\"/><w:tabs><w:tab w:val=\"center\" w:pos=\"{center}\"/><w:tab w:val=\"left\" w:pos=\"{}\"/></w:tabs>", right.round())
            } else { String::new() };
            let custom_leading = custom.map(|s| format!("<w:spacing w:line=\"{}\" w:lineRule=\"exact\"/>", (s.font_size.max(doc.settings.font_size) * doc.settings.line_height * 20.0).round())).unwrap_or_default();
            body.push_str(&format!("<w:p><w:pPr><w:pStyle w:val=\"{style}\"/>{tabs}{label_spacing}{custom_leading}</w:pPr>{runs}</w:p>"));
        }
    }
    let p = &doc.settings.paper;
    let twips = |inch: f64| (inch * 1440.0).round() as i64;
    let document_xml = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}
<w:sectPr><w:pgSz w:w="{}" w:h="{}"/><w:pgMar w:top="{}" w:right="{}" w:bottom="{}" w:left="{}"/></w:sectPr>
</w:body></w:document>"#, twips(p.page_width_inch), twips(p.page_height_inch), twips(p.margin_top_inch), twips(p.margin_right_inch), twips(p.margin_bottom_inch), twips(p.margin_left_inch));
    let size = (doc.settings.font_size * 2.0).round() as i64;
    let small = (doc.settings.font_size * 0.85 * 2.0).round() as i64;
    let leading = (doc.settings.font_size * doc.settings.line_height * 20.0).round() as i64;
    let latin = if chinese { "Times New Roman" } else { "Courier New" };
    let east = if chinese { "宋体" } else { "等线" };
    let transition_align = if chinese { "left" } else { "right" };
    let parent_align = if chinese { "<w:jc w:val=\"center\"/>" } else { "" };
    let styles_xml = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:pPr><w:spacing w:line="{leading}" w:lineRule="exact" w:after="{leading}"/></w:pPr><w:rPr><w:rFonts w:ascii="{latin}" w:hAnsi="{latin}" w:eastAsia="{east}"/><w:sz w:val="{size}"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="SceneHeading"><w:name w:val="Scene Heading"/><w:basedOn w:val="Normal"/><w:pPr><w:keepNext/></w:pPr><w:rPr><w:b/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Character"><w:name w:val="Character"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/><w:keepNext/><w:spacing w:after="0"/></w:pPr><w:rPr><w:b/><w:rFonts w:eastAsia="黑体"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Dialogue"><w:name w:val="Dialogue"/><w:basedOn w:val="Normal"/><w:pPr><w:ind w:left="1440" w:right="1440"/><w:spacing w:after="0"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Parenthetical"><w:name w:val="Parenthetical"/><w:basedOn w:val="Normal"/><w:pPr>{parent_align}<w:ind w:left="1440" w:right="1440"/><w:keepNext/><w:spacing w:after="0"/></w:pPr><w:rPr><w:sz w:val="{}"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Transition"><w:name w:val="Transition"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="{transition_align}"/></w:pPr><w:rPr><w:b/></w:rPr></w:style>
</w:styles>"#, if chinese { small } else { size });

    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;

    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("[Content_Types].xml", opts)?;
        zip.write_all(content_types.as_bytes())?;
        zip.start_file("_rels/.rels", opts)?;
        zip.write_all(rels.as_bytes())?;
        zip.start_file("word/document.xml", opts)?;
        zip.write_all(document_xml.as_bytes())?;
        zip.start_file("word/_rels/document.xml.rels", opts)?;
        zip.write_all(doc_rels.as_bytes())?;
        zip.start_file("word/styles.xml", opts)?;
        zip.write_all(styles_xml.as_bytes())?;
        zip.finish()?;
    }
    Ok(buf)
}

fn run(text: &str, properties: &str) -> String {
    format!("<w:r><w:rPr>{properties}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>", escape_xml(text))
}
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenplay_core::ast::ScreenplayBlock;

    #[test]
    fn t01_export_basic_docx() {
        let mut doc = ScreenplayDocument::blank("Test");
        doc.blocks.push(ScreenplayBlock::new(
            BlockType::SceneHeading,
            "EXT. BAR - NIGHT",
        ));
        doc.blocks.push(ScreenplayBlock::new(BlockType::Character, "JOHN"));
        doc.blocks.push(ScreenplayBlock::new(BlockType::Dialogue, "Hello."));

        let bytes = export_docx(&doc).unwrap();
        assert!(bytes.len() > 100);
        // DOCX 是 ZIP，检查 ZIP 头
        assert_eq!(&bytes[0..2], b"PK");
    }
}

#[cfg(test)] mod reference_tests {
    use super::*;
    use std::io::Read;
    #[test] fn chinese_styles_and_voice_run_survive_word_export() {
        let mut doc = ScreenplayDocument::blank("虚构验收");
        doc.settings.typography = "chinese-reading".into();
        doc.settings.font_size = 14.0;
        doc.settings.paper = screenplay_core::ast::PaperSettings::a4();
        doc.blocks.push(screenplay_core::ast::ScreenplayBlock::new(BlockType::Character, "小林（画外音）"));
        doc.blocks.push(screenplay_core::ast::ScreenplayBlock::new(BlockType::Parenthetical, "轻声"));
        doc.blocks.push(screenplay_core::ast::ScreenplayBlock::new(BlockType::Shot, "镜头切换。小林放下书。"));
        doc.blocks.push(screenplay_core::ast::ScreenplayBlock::new(BlockType::Action, "蒙太奇。画面上是各种报纸。"));
        let bytes = export_docx(&doc).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut xml = String::new(); zip.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
        assert!(xml.contains("<w:b/></w:rPr><w:t xml:space=\"preserve\">蒙太奇</w:t>"));
        assert!(xml.contains(">。画面上是各种报纸。</w:t>"));
        assert!(xml.contains("（轻声）"));
        assert!(xml.contains("w:b w:val=\"0\""));
        assert!(xml.contains("<w:tab w:val=\"center\""));
        assert!(xml.contains("w:w=\"11909\""));
        assert!(xml.contains("<w:b/></w:rPr><w:t xml:space=\"preserve\">镜头切换。"));
        let mut styles = String::new(); zip.by_name("word/styles.xml").unwrap().read_to_string(&mut styles).unwrap();
        assert!(styles.contains("w:eastAsia=\"宋体\""));
        assert!(styles.contains("w:eastAsia=\"黑体\""));
        assert!(styles.contains("w:sz w:val=\"24\""));
    }
}
#[cfg(test)] mod custom_tests {
    use super::*;
    use std::io::Read;
    #[test] fn custom_element_properties_are_in_word_runs() {
        let mut doc = ScreenplayDocument::blank("自定义样式");
        doc.blocks.push(screenplay_core::ast::ScreenplayBlock::new(BlockType::Dialogue, "测试对白"));
        doc.settings.element_styles.insert("dialogue".into(), screenplay_core::ast::ElementStyle { font_family: "PingFang SC".into(), font_size: 18.0, bold: false, italic: true, color: "#2355aa".into() });
        let mut zip = zip::ZipArchive::new(Cursor::new(export_docx(&doc).unwrap())).unwrap();
        let mut xml = String::new(); zip.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
        for property in ["w:eastAsia=\"PingFang SC\"", "w:sz w:val=\"36\"", "w:i w:val=\"1\"", "w:color w:val=\"2355aa\""] { assert!(xml.contains(property)); }
    }
}
