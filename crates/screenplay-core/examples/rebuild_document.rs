//! Offline repair helper: validate repaired AST without replacing any project container.
use screenplay_core::{ast::ScreenplayDocument, parser::parse_scene_heading};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let mut doc: ScreenplayDocument = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    for block in &mut doc.blocks {
        if block.block_type == screenplay_core::ast::BlockType::SceneHeading {
            let number = block.scene_heading.as_ref().and_then(|h| h.scene_number.clone());
            let (mut heading, _) = parse_scene_heading(&block.text).ok_or_else(|| format!("Invalid heading: {}", block.text))?;
            heading.scene_number = number.or(heading.scene_number);
            block.scene_heading = Some(heading);
        }
    }
    doc.rebuild_derived();
    doc.validate()?;
    std::fs::write(&args[2], serde_json::to_vec_pretty(&doc)?)?;
    if let Some(html) = args.get(3) { std::fs::write(html, screenplay_core::serializer::to_html(&doc))?; }
    println!("revision={} scenes={} blocks={} characters={} pages={}", doc.revision, doc.scenes.len(), doc.blocks.len(), doc.characters.len(), screenplay_core::pagination::paginate(&doc).total_pages);
    Ok(())
}
