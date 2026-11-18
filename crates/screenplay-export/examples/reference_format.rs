//! Reproducible, fictional format fixture. Never uses the supplied book's prose.
use screenplay_core::{ast::*, parser::{parse, ParseOptions}, pagination};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("output directory"));
    std::fs::create_dir_all(&out)?;
    let script = "12-日-书房-内\n\n小林把旧书放在桌上。窗帘轻轻摆动。\n\n小林（画外音）：门外的人已经离开了。\n\n小林（轻声）：等雨停了，我们再出发。\n\n镜头切换。小林合上书，走到窗边。\n\n镜头推向窗外的雨滴。\n\n转场。\n\n13-夜-小巷-外\n\n路灯映在积水中。";
    let mut doc = parse(script, &ParseOptions { denoise: false, ..Default::default() }).document;
    // 与应用内「自动统一场景标题」一致：按默认中文模板重写场景头显示文本。
    for block in &mut doc.blocks {
        if block.block_type == BlockType::SceneHeading {
            if let Some((display, _)) = screenplay_core::heading_format::format_heading(&block.text, &doc.settings) {
                block.text = display;
            }
        }
    }
    doc.title_page.title = "虚构排版验收".into();
    doc.settings.include_title_page = false;
    for (profile, size, leading, family) in [
        ("hollywood", 12.0, 1.0, "Courier, monospace"),
        ("chinese-reading", 14.0, 1.6, "Songti SC, SimSun, serif"),
        ("chinese-general", 12.0, 1.5, "Songti SC, SimSun, serif"),
        ("chinese-production", 12.0, 1.5, "Courier New, Songti SC, SimSun, serif"),
    ] {
        doc.settings.typography = profile.into();
        doc.settings.font_size = size;
        doc.settings.line_height = leading;
        doc.settings.font_family = family.into();
        doc.settings.paper = PaperSettings::a4();
        for (suffix, show) in [("numbers-off", false), ("numbers-on", true)] {
            doc.settings.show_scene_numbers = show;
            let base = format!("{profile}-{suffix}");
            std::fs::write(out.join(format!("{base}.pdf")), screenplay_export::pdf::export_pdf(&doc)?)?;
            std::fs::write(out.join(format!("{base}.docx")), screenplay_export::docx::export_docx(&doc)?)?;
            std::fs::write(out.join(format!("{base}.html")), screenplay_core::serializer::to_html(&doc))?;
            std::fs::write(out.join(format!("{base}.json")), serde_json::to_vec_pretty(&pagination::paginate(&doc))?)?;
        }
    }
    doc.settings.show_scene_numbers = false;
    doc.blocks.clear();
    doc.blocks.push(ScreenplayBlock::new(BlockType::Character, "小林（画外音）"));
    let long: String = (0..300).map(|i| format!("第{i:03}句：雨落在窗台上，屋里的人整理旧书，等候远方来信。\n")).collect();
    doc.blocks.push(ScreenplayBlock::new(BlockType::Dialogue, long));
    std::fs::write(out.join("chinese-long.pdf"), screenplay_export::pdf::export_pdf(&doc)?)?;
    std::fs::write(out.join("chinese-long.json"), serde_json::to_vec_pretty(&pagination::paginate(&doc))?)?;
    println!("fictional screenplay: PDF/DOCX/HTML and pagination fixtures exported");
    Ok(())
}
