//! Project-wide scene heading punctuation, without translating locations or times.
use crate::{ast::*, parser::parse_scene_heading};

pub fn valid_template(template: &str) -> bool {
    if template.len() > 512 || template.contains(['\n','\r']) { return false; }
    let mut rest = template.to_string();
    for token in ["{type}", "{location}", "{time}"] {
        if template.matches(token).count() != 1 { return false; }
        rest = rest.replace(token, "");
    }
    !rest.contains(['{','}'])
}

/// Recognize a user's own template before the general screenplay parser changes punctuation.
pub fn parse_configured_heading(text:&str, settings:&ScreenplaySettings)->Option<SceneHeadingData> {
    if !settings.scene_heading_auto || !valid_template(&settings.scene_heading_template) { return None; }
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String,regex::Regex>>> = std::sync::OnceLock::new();
    let re={
        let mut cache=CACHE.get_or_init(Default::default).lock().ok()?;
        if let Some(re)=cache.get(&settings.scene_heading_template) {re.clone()} else {
    let mut pattern=String::from("(?i)^\\s*");let mut rest=settings.scene_heading_template.as_str();
    while let Some(at)=rest.find('{') {
        pattern.push_str(&regex::escape(&rest[..at]));
        let end=rest[at..].find('}')?+at;
        let name=match &rest[at..=end] {"{type}"=>"kind","{location}"=>"location","{time}"=>"time",_=>return None};
        pattern.push_str(&format!("(?P<{name}>.+?)"));rest=&rest[end+1..];
    }
    pattern.push_str(&regex::escape(rest));pattern.push_str("\\s*$");
    let re=regex::Regex::new(&pattern).ok()?;
    if cache.len()>=32 {cache.clear();}
    cache.insert(settings.scene_heading_template.clone(),re.clone());re
        }
    };let caps=re.captures(text.trim().trim_start_matches('.').trim())?;
    let kind=caps.name("kind")?.as_str().trim();let location=caps.name("location")?.as_str().trim();let time=caps.name("time")?.as_str().trim();
    let (data,_)=parse_scene_heading(&format!("{kind}。{location}——{time}"))?;
    if data.location!=location||data.time_of_day!=time {return None;}
    Some(data)
}

pub fn format_heading(text: &str, settings: &ScreenplaySettings) -> Option<(String, SceneHeadingData)> {
    if !settings.scene_heading_auto || text.len()>4096 {return format_heading_uncached(text,settings);}
    type ResultCache=std::collections::HashMap<(String,String,String),Option<(String,SceneHeadingData)>>;
    static CACHE:std::sync::OnceLock<std::sync::Mutex<ResultCache>>=std::sync::OnceLock::new();
    let key=(settings.scene_heading_template.clone(),settings.scene_heading_language.clone(),text.into());
    let cache=CACHE.get_or_init(Default::default);
    if let Some(result)=cache.lock().ok().and_then(|c|c.get(&key).cloned()){return result;}
    let result=format_heading_uncached(text,settings);
    if let Ok(mut c)=cache.lock(){if c.len()>=2048{c.clear();}c.insert(key,result.clone());}
    result
}

fn format_heading_uncached(text: &str, settings: &ScreenplaySettings) -> Option<(String, SceneHeadingData)> {
    if !settings.scene_heading_auto || !valid_template(&settings.scene_heading_template) { return None; }
    let mut data = parse_configured_heading(text,settings).or_else(||parse_scene_heading(text).map(|v|v.0))?;
    // An incomplete heading is still being typed; leave it alone.
    if data.location.is_empty() || data.time_of_day.is_empty() { return None; }
    let prefix = match (&data.int_ext, settings.scene_heading_language.as_str()) {
        (IntExt::Int,"zh") => "内景", (IntExt::Ext,"zh") => "外景", (IntExt::IntExt,"zh") => "内景/外景",
        (IntExt::Int,_) => "INT", (IntExt::Ext,_) => "EXT", (IntExt::IntExt,_) => "INT/EXT",
        _ => return None,
    };
    // Do not translate or simplify DAY, 白天, 次日早晨, montage or location details.
    data.display_language = settings.scene_heading_language.clone();
    let template = &settings.scene_heading_template;
    let mut output = String::new();
    let mut rest = template.as_str();
    while let Some(at) = rest.find('{') {
        output.push_str(&rest[..at]);
        let end = rest[at..].find('}')? + at;
        output.push_str(match &rest[at..=end] { "{type}" => prefix, "{location}" => &data.location, "{time}" => &data.time_of_day, _ => return None });
        rest = &rest[end+1..];
    }
    output.push_str(rest);
    Some((output, data))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consistent_punctuation_preserves_words_and_incomplete_input() {
        let s=ScreenplaySettings::default();
        for (input,want) in [("内，咖啡馆 白天","内景。咖啡馆——白天"),("外景。仓库—夜晚","外景。仓库——夜晚"),("int. 酒吧 夜","内景。酒吧——夜"),("外景。客轮缓缓驶入纽约港——次日早晨","外景。客轮缓缓驶入纽约港——次日早晨")] {
            assert_eq!(format_heading(input,&s).unwrap().0,want);
        }
        assert!(format_heading("INT. OFFICE",&s).is_none());
        assert!(format_heading("这里暂时还没想好",&s).is_none());
        let mut custom=s;custom.scene_heading_template="{location} / {type} / {time}".into();custom.scene_heading_language="en".into();
        assert_eq!(format_heading("内景。酒吧——夜",&custom).unwrap().0,"酒吧 / INT / 夜");
        assert!(!valid_template("{location}"));
        assert!(!valid_template("{type}{location}{time}{bad}"));
    }
}

#[cfg(test)]
mod paper_regressions {
    use super::*;
    #[test]
    fn paper_normalizes_legacy_and_comma_headings_without_editing_source() {
        let mut d=ScreenplayDocument::blank("format");
        for input in ["外景。咖啡馆——夜","内景。仓库—日","外．城堡外，日","内 城堡外，日"] { d.blocks.push(ScreenplayBlock::new(BlockType::SceneHeading,input)); }
        let before=d.blocks.clone();
        let paper=crate::pagination::paginate(&d);
        let texts:Vec<_>=paper.pages.iter().flat_map(|p|p.blocks.iter().map(|b|b.text.as_str())).collect();
        assert_eq!(texts,vec!["外景。咖啡馆——夜","内景。仓库——日","外景。城堡外——日","内景。城堡外——日"]);
        assert_eq!(d.blocks,before);
        assert_eq!(format_heading("外. 城堡外, 日",&d.settings).unwrap().0,"外景。城堡外——日");
    }
}

#[cfg(test)]
mod numbered_custom_regression {
    use super::*;
    #[test]
    fn numbered_custom_scene_roundtrips_without_becoming_action() {
        let mut d=ScreenplayDocument::blank("custom");
        let mut b=ScreenplayBlock::new(BlockType::SceneHeading,"咖啡馆 / 内景 / 日");
        b.scene_heading=Some(SceneHeadingData{scene_number:Some("12A".into()),..Default::default()});
        d.blocks.push(b);
        let source=crate::serializer::to_compact_source(&d);
        let parsed=crate::parse(&source,&crate::ParseOptions{denoise:false,..Default::default()}).document;
        assert_eq!(parsed.blocks[0].block_type,BlockType::SceneHeading);
        assert_eq!(parsed.blocks[0].text,"咖啡馆 / 内景 / 日");
        assert_eq!(parsed.blocks[0].scene_heading.as_ref().unwrap().scene_number.as_deref(),Some("12A"));
    }
}
#[cfg(test)] mod custom_order_tests {
 use super::*;
 #[test] fn configurable_order_parses_its_own_output() {
  let mut s=ScreenplaySettings::default();s.scene_heading_template="{type} {time} {location}".into();
  let text=format_heading("内景。咖啡馆——白天",&s).unwrap().0;
  assert_eq!(text,"内景 白天 咖啡馆");
  let data=parse_configured_heading(&text,&s).unwrap();assert_eq!(data.location,"咖啡馆");assert_eq!(data.time_of_day,"白天");
  assert_eq!(format_heading(&text,&s).unwrap().0,text);
 }
}
