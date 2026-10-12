//! 确定性剧本解析器（完全离线）。
//!
//! 这是「Hybrid Pipeline」的第一层：能确定性完成的绝不交给大模型。
//! 流水线：文字提取 → Unicode 规范化 → 换行恢复 → 页眉页脚/水印去噪 →
//! 段落重建 → 场景 → 人物 → 对白 → 括注 → 动作 → 转场 → AST。

use crate::ast::*;
use crate::denoise;
use crate::dict::*;

#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    pub title: Option<String>,
    pub denoise: bool,
    pub known_characters: Vec<String>,
    /// 已按布局标注类型的行（PDF 坐标推断结果），key = 段落文本
    pub layout_hints: Vec<(String, BlockType)>,
}

#[derive(Debug, Clone)]
pub struct ParseOutcome {
    pub document: ScreenplayDocument,
    pub low_confidence: usize,
    pub removed_noise: Vec<String>,
}

impl ParseOutcome {
    pub fn scene_count(&self) -> usize {
        self.document.scenes.len()
    }
    pub fn character_count(&self) -> usize {
        self.document.characters.len()
    }
    pub fn dialogue_count(&self) -> usize {
        self.document
            .blocks
            .iter()
            .filter(|b| b.block_type == BlockType::Dialogue)
            .count()
    }
}

/// Unicode 规范化 + 换行恢复
pub fn normalize(input: &str) -> String {
    let s = input
        .replace("\u{feff}", "")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace("\u{00a0}", " ")
        .replace("\u{3000}", " ");
    // 全角空格已在上面处理；去除行尾空白
    s.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 去除 Markdown 装饰但保留内容
pub fn strip_markdown(line: &str) -> String {
    let mut s = line.trim_start().to_string();
    // heading
    while s.starts_with('#') {
        s.remove(0);
        s = s.trim_start().to_string();
    }
    // blockquote
    s = s.trim_start_matches('>').trim_start().to_string();
    // list markers
    for p in ["- ", "* ", "+ "] {
        if s.starts_with(p) {
            s = s[p.len()..].to_string();
            break;
        }
    }
    // ordered list "1. " / "1、" —— 仅当后面不是场景编号模式时
    s = strip_leading_ordered_marker(s);
    s = s.replace("**", "").replace("__", "");
    // hr
    if s.chars().all(|c| c == '-' || c == '*' || c == '_') && s.chars().count() >= 3 {
        return String::new();
    }
    s
}

fn strip_leading_ordered_marker(s: String) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i > 4 {
        return s;
    }
    if i < chars.len() && (chars[i] == '.' || chars[i] == '、' || chars[i] == ')' || chars[i] == '）')
    {
        let rest: String = chars[i + 1..].iter().collect();
        let rest_trimmed = rest.trim_start();
        // 若是场景编号（如 "1.外 镇北堡" / "1. INT. TAVERN"），保留编号给场景正则捕获
        if crate::dict::int_ext_prefixes()
            .iter()
            .any(|p| rest_trimmed.starts_with(p))
        {
            return s;
        }
        return rest_trimmed.to_string();
    }
    s
}

/// 剥离动作前缀符号
pub fn strip_action_prefix(s: &str) -> String {
    let t = s.trim_start();
    match t.chars().next() {
        Some(c) if ACTION_PREFIXES.contains(&c) => t[c.len_utf8()..].trim_start().to_string(),
        _ => t.to_string(),
    }
}

fn has_action_prefix(s: &str) -> bool {
    s.trim_start()
        .chars()
        .next()
        .is_some_and(|c| ACTION_PREFIXES.contains(&c))
}

/// 解析场景标题
pub fn parse_scene_heading(line: &str) -> Option<(SceneHeadingData, String)> {
    let cleaned = strip_markdown(line);
    let cleaned=cleaned.strip_prefix('.').unwrap_or(&cleaned).trim();
    let caps = scene_regex().captures(cleaned)?;
    let int_ext_str = caps.name("int_ext")?.as_str().replace(' ', "");
    let mut int_ext = int_ext_from_prefix(&int_ext_str)?;
    let rest = caps.name("rest")?.as_str().trim();
    if rest.is_empty() {
        return None;
    }

    // A compound location can explicitly cross interior/exterior within one heading.
    if (int_ext == IntExt::Ext && rest.contains("/内景")) || (int_ext == IntExt::Int && rest.contains("/外景")) {
        int_ext = IntExt::IntExt;
    }
    let zh = has_chinese(&cleaned);
    let (location, time_of_day) = split_location_time(rest, zh);

    if location.is_empty() && time_of_day.is_empty() {
        return None;
    }

    let data = SceneHeadingData {
        scene_number: caps.name("num").map(|m| m.as_str().to_string()),
        int_ext,
        location,
        time_of_day,
        display_language: if zh { "zh".into() } else { "en".into() },
    };

    // 规范化显示文本
    let display = format_scene_heading(&data);
    Some((data, display))
}

/// 拆分地点与时间（最长时间词优先）
fn split_location_time(rest: &str, zh: bool) -> (String, String) {
    if zh {
        if let Some(body) = rest.strip_suffix("——蒙太奇") {
            let (location, time) = split_location_time(body, zh);
            if !time.is_empty() { return (location, format!("{time}——蒙太奇")); }
        }
    }
    // 先按显式分隔符切
    for sep in ["——", " - ", " – ", " — ", " · ", "・", "-", "–", "—", "·", "，", ","] {
        if let Some(idx) = rest.rfind(sep) {
            let (l, t) = rest.split_at(idx);
            let l = l.trim().trim_end_matches(['.', '．']).trim();
            let t = t[sep.len()..].trim();
            if !l.is_empty() && !t.is_empty() && looks_like_time(t, zh) {
                return (l.to_string(), t.to_string());
            }
        }
    }

    // 中文：空格分隔取最后一段
    if zh {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() >= 2 {
            let last = parts[parts.len() - 1];
            if looks_like_time(last, true) {
                return (parts[..parts.len() - 1].join(" ").trim_end_matches([',','，']).trim().to_string(), last.to_string());
            }
        }
        // 无空格：后缀匹配时间词
        for t in TIME_ZH.iter().filter(|t| t.chars().count() >= 2) {
            if rest.ends_with(t) {
                let l = rest[..rest.len() - t.len()].trim();
                if !l.is_empty() {
                    return (l.to_string(), t.to_string());
                }
            }
        }
        return (rest.trim().to_string(), String::new());
    }

    // 英文：后缀匹配
    let upper = rest.to_ascii_uppercase();
    for t in TIME_EN {
        if upper.ends_with(t) {
            let cut = rest.len() - t.len();
            let l = rest[..cut].trim();
            if !l.is_empty() {
                return (l.to_string(), t.to_string());
            }
        }
    }
    (rest.trim().to_string(), String::new())
}

fn looks_like_time(s: &str, zh: bool) -> bool {
    if zh {
        TIME_ZH.contains(&s) || ["次日", "翌日", "当日", "同日"].iter().any(|prefix| s.strip_prefix(prefix).is_some_and(|rest| TIME_ZH.contains(&rest))) || TIME_EN.iter().any(|t| s.eq_ignore_ascii_case(t))
    } else {
        let u = s.to_ascii_uppercase();
        TIME_EN.iter().any(|t| u == *t)
    }
}

/// 规范化场景标题显示文本
pub fn format_scene_heading(d: &SceneHeadingData) -> String {
    let prefix = if d.display_language == "zh" {
        d.int_ext.to_zh()
    } else {
        d.int_ext.to_en()
    };
    let sep = if d.display_language == "zh" { ". " } else { " - " };
    if d.time_of_day.is_empty() {
        format!("{}. {}", prefix, d.location)
    } else {
        format!("{}{}{}", prefix, sep, d.location)
            + &format!("{}{}", if d.display_language == "zh" { " " } else { " - " }, d.time_of_day)
    }
}

pub(crate) fn is_transition(line: &str) -> bool {
    let t = line.trim().trim_end_matches(['：', ':', '。', '.']);
    if t.is_empty() {
        return false;
    }
    if TRANSITIONS_ZH.iter().any(|x| t == *x) {
        return true;
    }
    let u = t.to_ascii_uppercase();
    TRANSITIONS_EN.iter().any(|x| u == x.trim_end_matches(':'))
}

pub(crate) fn is_shot(line: &str) -> bool {
    let t = line.trim().trim_end_matches(['。', '.']);
    // Camera movement inside narrative is action, not a standalone shot label.
    if SHOTS_ZH.iter().any(|s| t == *s || t.strip_prefix(s).is_some_and(|rest| rest.starts_with(['：', ':']))) {
        return true;
    }
    let u = t.to_ascii_uppercase();
    SHOTS_EN.iter().any(|s| u.starts_with(s))
}

struct DialogueParts {
    character: String,
    parenthetical: Option<String>,
    dialogue: String,
}

/// 中文冒号对白：`人物：对白` / `人物（括注）：对白`
fn parse_zh_dialogue(line: &str) -> Option<DialogueParts> {
    // 找第一个全角或半角冒号
    let idx = line.find(['：', ':'])?;
    let (head, tail) = line.split_at(idx);
    // tail 以冒号开头，需跳过整个冒号字符的字节长度（全角 3 字节，半角 1 字节）
    let colon_len = tail.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    let dialogue = tail[colon_len..].trim().to_string();
    if dialogue.is_empty() {
        return None;
    }
    let mut head = head.trim().to_string();
    if head.is_empty() {
        return None;
    }

    // 提取括注
    let mut parenthetical = None;
    for (o, c) in [('（', '）'), ('(', ')'), ('【', '】'), ('[', ']')] {
        if let Some(oi) = head.find(o) {
            if head.ends_with(c) {
                parenthetical = Some(head[oi + o.len_utf8()..head.len() - c.len_utf8()].trim().to_string());
                head = head[..oi].trim().to_string();
                break;
            }
        }
    }

    let character = head.trim().to_string();
    if character.is_empty() {
        return None;
    }
    Some(DialogueParts {
        character,
        parenthetical,
        dialogue,
    })
}

/// 判断 `xxx：` 是否是人物对白（说明书第 22 条：不能误识别普通冒号）
fn is_character_dialogue(parts: &DialogueParts, known: &[String]) -> Option<f32> {
    let name = &parts.character;

    // 1. 明确排除表
    if NON_CHARACTER_LABELS_ZH.contains(&name.as_str()) {
        return None;
    }
    if NON_CHARACTER_LABELS_EN.contains(&name.to_ascii_uppercase().as_str()) {
        return None;
    }
    // 2. 长度：中文人名一般 2-6 字；英文 2-20 字符
    let n = name.chars().count();
    let too_long = if has_chinese(name) { n > 8 } else { n > 24 };
    if too_long {
        return None;
    }
    // 单字且非已知人物，且是对白 —— 中文单字人名很少见，降低置信度但不排除
    // 3. 名称里不能包含句读
    if name.contains(['。', '，', '！', '？', '；', '、', '…']) {
        return None;
    }
    // 4. 名称不能是纯数字
    if name.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // 5. 已知人物 → 高置信度
    if known.iter().any(|k| k == name) {
        return Some(0.97);
    }
    // 6. 带括注 → 略高（`剑客（内心独白）：` 是强信号）
    let base = if parts.parenthetical.is_some() { 0.92 } else { 0.84 };
    // 7. 名字长度适中更可信
    let score: f32 = if (2..=5).contains(&n) { base } else { base - 0.10 };
    Some(score.clamp(0.0, 1.0))
}

/// 判断一行是否像独立人物名（用于「人名 + 括注 + 对白」的独立行模式）
fn looks_like_character_name(s: &str) -> bool {
    let name = s.trim();
    if name.is_empty() {
        return false;
    }
    if NON_CHARACTER_LABELS_ZH.contains(&name) {
        return false;
    }
    if NON_CHARACTER_LABELS_EN.contains(&name.to_ascii_uppercase().as_str()) {
        return false;
    }
    // 纯数字不是人名
    if name.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // 不能包含句读标点
    if name.contains(['。', '，', '！', '？', '；', '…']) {
        return false;
    }
    let n = name.chars().count();
    if has_chinese(name) {
        (2..=8).contains(&n)
    } else {
        (2..=24).contains(&n) && is_english_character_line(name)
    }
}

/// 主解析入口
pub fn parse(text: &str, opts: &ParseOptions) -> ParseOutcome {
    let normalized = normalize(text);

    let markdown = crate::markdown::prepare(&normalized);
    let (body, removed_noise) = if opts.denoise && markdown.is_none() {
        let d = denoise::denoise(&normalized);
        (d.text, d.removed)
    } else {
        (normalized, Vec::new())
    };

    // 段落 = 空行分隔
    let paragraphs: Vec<String> = if let Some(md) = &markdown { md.lines.iter().map(|(s,_)| s.clone()).collect() } else {
        body.lines().filter(|l| !l.trim().is_empty()).map(|l| l.trim().to_string()).collect()
    };

    // OCR/AI may isolate the Chinese inline montage label. Keep it with its
    // following action, but never merge across a scene, cue, or explicit transition.
    let mut joined = Vec::new();
    let mut at = 0;
    while at < paragraphs.len() {
        let label = paragraphs[at].trim().trim_start_matches(['!', '>']).trim_matches('*').trim_end_matches(['。', '.']);
        if label == "蒙太奇" && at + 1 < paragraphs.len() {
            let next = paragraphs[at + 1].trim();
            let action = next.strip_prefix('!').unwrap_or(next).trim();
            if !next.starts_with(['@', '>', '.', '#', '(' , '（'])
                && parse_scene_heading(action).is_none()
                && (next.starts_with('!') || action.contains(['。', '，', '：', '！', '？', '.', ',']))
            {
                joined.push(format!("!蒙太奇。{action}"));
                at += 2;
                continue;
            }
        }
        joined.push(paragraphs[at].clone());
        at += 1;
    }
    let paragraphs = joined;

    let hints: std::collections::HashMap<&str, BlockType> = opts
        .layout_hints
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();

    let mut blocks: Vec<ScreenplayBlock> = Vec::new();
    let mut known: Vec<String> = opts.known_characters.clone();
    let mut last_type: Option<BlockType> = None;
    let mut low_confidence = 0usize;
    let mut current_scene: Option<String> = None;

    // push 返回新 block 在 blocks 中的下标，调用方通过 blocks[idx] 修改，
    // 避免「push 克隆后修改不生效」的 bug。current_scene / last_type 作为参数传入。
    let mut push = |blocks: &mut Vec<ScreenplayBlock>,
                    scene: &Option<String>,
                    last_type: &mut Option<BlockType>,
                    bt: BlockType,
                    text: String,
                    conf: f32|
     -> usize {
        let mut b = ScreenplayBlock::new(bt, text);
        b.confidence = conf;
        b.scene_id = scene.clone();
        if conf < 0.75 {
            low_confidence += 1;
        }
        blocks.push(b);
        *last_type = Some(bt);
        blocks.len() - 1
    };

    let mut i = 0usize;
    while i < paragraphs.len() {
        let raw = paragraphs[i].clone();

        if let Some(forced) = markdown.as_ref().and_then(|md| md.lines.get(i)).and_then(|(_,t)| *t) {
            let confidence = if forced == BlockType::Dialogue && last_type == Some(BlockType::Dialogue) { 0.70 } else { 0.95 };
            push(&mut blocks, &current_scene, &mut last_type, forced, raw.clone(), confidence);
            if forced == BlockType::Character { known.push(crate::typography::character_parts(&raw).0.to_string()); }
            i += 1;
            continue;
        }

        // Reading editions put a scene number on a separate line above the heading.
        if let Some(number) = raw.strip_prefix('第').and_then(|s| s.strip_suffix('场')) {
            let valid = !number.is_empty() && number.chars().all(|c| c.is_ascii_alphanumeric() || "零〇一二三四五六七八九十百千万".contains(c));
            if valid {
                if let Some((mut data, display)) = paragraphs.get(i + 1).and_then(|line| parse_scene_heading(line)) {
                    data.scene_number = Some(number.to_string());
                    let idx = push(&mut blocks, &current_scene, &mut last_type, BlockType::SceneHeading, display, 0.99);
                    blocks[idx].scene_heading = Some(data);
                    current_scene = Some(blocks[idx].id.clone());
                    blocks[idx].scene_id = current_scene.clone();
                    i += 2;
                    continue;
                }
            }
        }

        // PDF 布局提示优先（坐标已判定类型）
        if let Some(forced) = hints.get(raw.as_str()) {
            let idx = push(&mut blocks, &current_scene, &mut last_type, *forced, raw.clone(), 0.9);
            if *forced == BlockType::SceneHeading {
                current_scene = Some(blocks[idx].id.clone());
            }
            if *forced == BlockType::Character {
                known.push(raw.trim().to_string());
            }
            i += 1;
            continue;
        }

        // 1) Fountain 显式标记
        if let Some(text)=raw.strip_prefix('>').filter(|s|!s.trim().is_empty()) {
            push(&mut blocks,&current_scene,&mut last_type,BlockType::Transition,text.trim().to_string(),0.99);i+=1;continue;
        }
        if let Some(text)=raw.strip_prefix('!').filter(|s|!s.trim().is_empty()) {
            let kind=if is_shot(text.trim()){BlockType::Shot}else{BlockType::Action};
            push(&mut blocks,&current_scene,&mut last_type,kind,text.trim().to_string(),0.99);i+=1;continue;
        }
        if raw.starts_with('@') {
            let name = raw[1..].trim().to_string();
            if !name.is_empty() {
                if let Some(parts)=parse_zh_dialogue(&format!("{name}：占位")).filter(|parts|parts.parenthetical.as_deref().is_some_and(|p|!crate::typography::is_delivery(p))) {
                    push(&mut blocks,&current_scene,&mut last_type,BlockType::Character,parts.character.clone(),0.99);
                    push(&mut blocks,&current_scene,&mut last_type,BlockType::Parenthetical,parts.parenthetical.unwrap(),0.99);
                    known.push(parts.character);
                } else {
                    push(&mut blocks, &current_scene, &mut last_type, BlockType::Character, name.clone(), 0.99);
                    known.push(name);
                }
            }
            i += 1;
            continue;
        }
        if raw.starts_with("===") {
            push(&mut blocks, &current_scene, &mut last_type, BlockType::PageBreak, String::new(), 1.0);
            i += 1;
            continue;
        }
        if let Some(s) = raw.strip_prefix('#') {
            if let Some(t) = s.strip_prefix('#') {
                let t = t.trim();
                if !t.is_empty() {
                    // ## 后可能是场景标题（如「## 1. 外·镇北堡城门·黄昏」）
                    if let Some((data, display)) = parse_scene_heading(t) {
                        let idx = push(&mut blocks, &current_scene, &mut last_type, BlockType::SceneHeading, display, 0.98);
                        blocks[idx].scene_heading = Some(data);
                        current_scene = Some(blocks[idx].id.clone());
                        blocks[idx].scene_id = current_scene.clone();
                        i += 1;
                        continue;
                    }
                    push(&mut blocks, &current_scene, &mut last_type, BlockType::Section, t.to_string(), 0.99);
                    i += 1;
                    continue;
                }
            }
        }
        if raw.starts_with('=') && raw.len() > 1 {
            push(
                &mut blocks,
                &current_scene,
                &mut last_type,
                BlockType::Synopsis,
                raw[1..].trim().to_string(),
                0.99,
            );
            i += 1;
            continue;
        }
        if raw.starts_with("[[") && raw.ends_with("]]") && raw.len() > 4 {
            push(
                &mut blocks,
                &current_scene,
                &mut last_type,
                BlockType::Note,
                raw[2..raw.len() - 2].trim().to_string(),
                0.99,
            );
            i += 1;
            continue;
        }

        // Fountain forced scene heading supports custom project templates.
        if raw.starts_with('.') && !raw.starts_with("..") && !raw[1..].trim().is_empty() {
            let body=raw[1..].trim();
            let numbered=body.split_once(". ").filter(|(n,_)|!n.is_empty()&&n.len()<=5&&n.chars().next().is_some_and(|c|c.is_ascii_digit())&&n.chars().all(|c|c.is_ascii_alphanumeric()));
            let heading=numbered.map(|(_,t)|t).unwrap_or(body);
            let idx=push(&mut blocks,&current_scene,&mut last_type,BlockType::SceneHeading,heading.to_string(),1.0);
            let mut data=parse_scene_heading(heading).map(|v|v.0).unwrap_or_default();
            if let Some((n,_))=numbered {data.scene_number=Some(n.into());}
            blocks[idx].scene_heading=Some(data);
            current_scene=Some(blocks[idx].id.clone());blocks[idx].scene_id=current_scene.clone();i+=1;continue;
        }
        // 2) 场景标题
        if let Some((data, display)) = parse_scene_heading(&raw) {
            let idx = push(&mut blocks, &current_scene, &mut last_type, BlockType::SceneHeading, display, 0.98);
            blocks[idx].scene_heading = Some(data);
            current_scene = Some(blocks[idx].id.clone());
            blocks[idx].scene_id = current_scene.clone();
            i += 1;
            continue;
        }

        // 3) 转场
        if is_transition(&raw) {
            push(&mut blocks, &current_scene, &mut last_type, BlockType::Transition, raw.clone(), 0.95);
            i += 1;
            continue;
        }

        // 4) 镜头
        if is_shot(&raw) {
            push(&mut blocks, &current_scene, &mut last_type, BlockType::Shot, raw.clone(), 0.90);
            i += 1;
            continue;
        }

        // 5) 动作前缀 △
        if has_action_prefix(&raw) {
            push(
                &mut blocks,
                &current_scene,
                &mut last_type,
                BlockType::Action,
                strip_action_prefix(&raw),
                0.95,
            );
            i += 1;
            continue;
        }

        // Voice qualifiers are part of the character cue, not a performance block.
        let (cue_name, qualifier) = crate::typography::character_parts(&raw);
        if qualifier.is_some() && looks_like_character_name(cue_name) {
            push(&mut blocks, &current_scene, &mut last_type, BlockType::Character, raw.trim().to_string(), 0.96);
            known.push(cue_name.to_string());
            i += 1;
            continue;
        }

        // 6) 独立人物名 + 后续括注 → Character
        //    如「老板娘\n（轻声）\n你变了。」中的「老板娘」
        if looks_like_character_name(&raw)
            && i + 1 < paragraphs.len()
            && is_parenthetical_line(&paragraphs[i + 1])
        {
            push(&mut blocks, &current_scene, &mut last_type, BlockType::Character, raw.trim().to_string(), 0.88);
            known.push(raw.trim().to_string());
            i += 1;
            continue;
        }

        // 7) 括注独立行（跟在 character 之后）
        if is_parenthetical_line(&raw) && matches!(last_type, Some(BlockType::Character)) {
            if crate::typography::is_delivery(&strip_parens(&raw)) {
                if let Some(cue) = blocks.last_mut() { cue.text.push_str(&format!("（{}）", strip_parens(&raw))); }
                i += 1;
                continue;
            }
            push(
                &mut blocks,
                &current_scene,
                &mut last_type,
                BlockType::Parenthetical,
                strip_parens(&raw),
                0.92,
            );
            i += 1;
            continue;
        }

        // 7) 中文冒号对白
        if let Some(parts) = parse_zh_dialogue(&raw) {
            if let Some(conf) = is_character_dialogue(&parts, &known) {
                let c_idx = push(&mut blocks, &current_scene, &mut last_type, BlockType::Character, if parts.parenthetical.as_deref().is_some_and(crate::typography::is_delivery) { format!("{}（{}）", parts.character, parts.parenthetical.as_deref().unwrap()) } else { parts.character.clone() }, conf);
                known.push(parts.character.clone());
                let cid = blocks[c_idx].id.clone();
                if let Some(p) = parts.parenthetical.as_ref().filter(|p| !crate::typography::is_delivery(p)) {
                    let p_idx = push(&mut blocks, &current_scene, &mut last_type, BlockType::Parenthetical, p.clone(), conf);
                    blocks[p_idx].character_id = Some(cid.clone());
                }
                let d_idx = push(&mut blocks, &current_scene, &mut last_type, BlockType::Dialogue, parts.dialogue.clone(), conf);
                blocks[d_idx].character_id = Some(cid);
                blocks[c_idx].confidence = conf;
                i += 1;
                continue;
            }
        }

        // 8) 英文大写人物行 + 后续对白
        if is_english_character_line(&raw) {
            // 下一非空行是括注或普通文本 → 视为 character
            if i + 1 < paragraphs.len() {
                let next = &paragraphs[i + 1];
                let next_is_dialogue = !is_parenthetical_line(next)
                    && parse_scene_heading(next).is_none()
                    && !is_transition(next);
                if next_is_dialogue || is_parenthetical_line(next) {
                    push(&mut blocks, &current_scene, &mut last_type, BlockType::Character, raw.trim().to_string(), 0.92);
                    known.push(raw.trim().to_string());
                    i += 1;
                    continue;
                }
            } else {
                push(&mut blocks, &current_scene, &mut last_type, BlockType::Character, raw.trim().to_string(), 0.75);
                known.push(raw.trim().to_string());
                i += 1;
                continue;
            }
        }

        // 9) 紧跟 character/parenthetical 之后 → dialogue
        if matches!(
            last_type,
            Some(BlockType::Character) | Some(BlockType::Parenthetical)
        ) {
            push(&mut blocks, &current_scene, &mut last_type, BlockType::Dialogue, raw.clone(), 0.90);
            i += 1;
            continue;
        }

        // 10) 括注独立行（其他情况）
        if is_parenthetical_line(&raw) {
            push(
                &mut blocks,
                &current_scene,
                &mut last_type,
                BlockType::Parenthetical,
                strip_parens(&raw),
                0.80,
            );
            i += 1;
            continue;
        }

        // 11) 默认：叙述 → Action（低置信度，交给用户确认）
        push(&mut blocks, &current_scene, &mut last_type, BlockType::Action, strip_markdown(&raw), 0.62);
        i += 1;
    }

    let mut doc = ScreenplayDocument::blank(opts.title.as_deref().unwrap_or("未命名剧本"));
    if let Some(md) = markdown {
        if let Some(title) = md.title { doc.metadata.title = title.clone(); doc.title_page.title = title; }
        if let Some(author) = md.author { doc.metadata.author = author.clone(); doc.title_page.author = Some(author); }
        doc.story_bible = md.bible;
    }
    doc.blocks = blocks;
    doc.rebuild_derived();
    doc.import_info = Some(ImportInfo {
        source_type: SourceType::Clipboard,
        source_name: None,
        imported_at: now_iso(),
        original_text: text.to_string(),
        removed_noise: removed_noise.clone(),
        preserve_original: true,
    });

    ParseOutcome {
        document: doc,
        low_confidence,
        removed_noise,
    }
}

/// 重新计算所有 block 的 source range（右侧自由文本）
/// 必须在完整生成右侧文本后调用
pub fn recompute_source_ranges(doc: &mut ScreenplayDocument, text: &str) {
    let bytes = text.as_bytes();
    let mut cursor = 0usize;
    let n = doc.blocks.len();

    // 第一遍：计算每个 block 的 source range（避免同时借用 doc.blocks 的可变与不可变）
    let mut ranges: Vec<Option<SourceRange>> = Vec::with_capacity(n);
    for idx in 0..n {
        let block_text = doc.blocks[idx].text.clone();
        let range = match find_paragraph(bytes, cursor, &block_text) {
            Some((start, end)) => {
                cursor = end;
                Some(SourceRange {
                    from: start,
                    to: end,
                    syntax: None,
                })
            }
            None => None,
        };
        ranges.push(range);
    }

    // 应用 range
    for (idx, r) in ranges.into_iter().enumerate() {
        if let Some(r) = r {
            doc.blocks[idx].source = Some(r);
        }
    }

    // 第二遍：合并 character(+parenthetical)+dialogue 的 inline 语法标记
    let mut i = 0usize;
    while i + 1 < n {
        if doc.blocks[i].block_type == BlockType::Character {
            let has_paren = doc.blocks
                .get(i + 1)
                .is_some_and(|x| x.block_type == BlockType::Parenthetical);
            let d_idx = if has_paren { i + 2 } else { i + 1 };
            if doc.blocks
                .get(d_idx)
                .is_some_and(|x| x.block_type == BlockType::Dialogue)
            {
                if let Some(s) = doc.blocks[i].source.as_ref() {
                    let sr = SourceRange {
                        from: s.from,
                        to: s.to,
                        syntax: Some("inline-dialogue".into()),
                    };
                    for k in i..=d_idx {
                        doc.blocks[k].source = Some(sr.clone());
                    }
                    i = d_idx + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
}

/// 在 text 中从 cursor 起找到包含 needle 的行范围（字节偏移）
fn find_paragraph(bytes: &[u8], cursor: usize, needle: &str) -> Option<(usize, usize)> {
    if needle.trim().is_empty() {
        return None;
    }
    // 简化：找到 needle 的首次出现（从 cursor 起）
    let hay = std::str::from_utf8(bytes).ok()?;
    let idx = hay.get(cursor..)?.find(needle)?;
    let start = cursor + idx;
    let end = start + needle.len();
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(out: &ParseOutcome) -> Vec<(BlockType, String)> {
        out.document
            .blocks
            .iter()
            .map(|b| (b.block_type, b.text.clone()))
            .collect()
    }

    fn p(text: &str) -> ParseOutcome {
        parse(
            text,
            &ParseOptions {
                denoise: false,
                ..Default::default()
            },
        )
    }

    // ---- T1 Parser ----
    #[test] fn forced_cue_splits_acting_note_but_preserves_voice_extension() {
        let doc=p("@小林（轻声）\n等等。\n\n@小林（画外音）\n下雨了。").document;
        assert_eq!(doc.blocks.iter().map(|b|b.block_type).collect::<Vec<_>>(),vec![BlockType::Character,BlockType::Parenthetical,BlockType::Dialogue,BlockType::Character,BlockType::Dialogue]);
        assert_eq!(doc.blocks[0].text,"小林");assert_eq!(doc.blocks[1].text,"轻声");assert_eq!(doc.blocks[3].text,"小林（画外音）");
    }
    #[test]
    fn agent_fountain_markers_parse_professional_elements(){
        let o=p(".外景 城门 - 日\n\n!荒原。\n\n@剑客\n（低声）\n我来了。\n\n>画面渐黑。\n\n!镜头推向城门。");
        assert_eq!(o.document.blocks.iter().map(|b|b.block_type).collect::<Vec<_>>(),vec![BlockType::SceneHeading,BlockType::Action,BlockType::Character,BlockType::Parenthetical,BlockType::Dialogue,BlockType::Transition,BlockType::Action]);
    }

    #[test]
    fn ambiguous_action_is_counted_for_import_review() {
        let outcome = p("一段无法判定结构的叙述");
        assert_eq!(outcome.document.blocks[0].block_type, BlockType::Action);
        assert!((outcome.document.blocks[0].confidence - 0.62).abs() < f32::EPSILON);
        assert_eq!(outcome.low_confidence, 1);
    }


    #[test]
    fn p01_zh_scene_with_number() {
        let o = p("1.外 镇北堡某街巷 黄昏");
        assert_eq!(o.document.blocks.len(), 1);
        let b = &o.document.blocks[0];
        assert_eq!(b.block_type, BlockType::SceneHeading);
        let d = b.scene_heading.as_ref().unwrap();
        assert_eq!(d.int_ext, IntExt::Ext);
        assert_eq!(d.location, "镇北堡某街巷");
        assert_eq!(d.time_of_day, "黄昏");
        assert_eq!(d.scene_number.as_deref(), Some("1"));
    }

    #[test]
    fn p02_zh_scene_int() {
        let o = p("内 酒肆 夜");
        let d = o.document.blocks[0].scene_heading.as_ref().unwrap();
        assert_eq!(d.int_ext, IntExt::Int);
        assert_eq!(d.location, "酒肆");
        assert_eq!(d.time_of_day, "夜");
    }

    #[test]
    fn p03_zh_scene_int_ext() {
        let o = p("内/外 马车 夜");
        let d = o.document.blocks[0].scene_heading.as_ref().unwrap();
        assert_eq!(d.int_ext, IntExt::IntExt);
        assert_eq!(d.location, "马车");
    }

    #[test]
    fn p04_action_prefix() {
        let o = p("△ 剑客走进酒肆。");
        assert_eq!(o.document.blocks[0].block_type, BlockType::Action);
        assert_eq!(o.document.blocks[0].text, "剑客走进酒肆。");
    }

    #[test]
    fn p05_character_parenthetical_dialogue() {
        let o = p("剑客（内心独白）：我是一个爱做梦的人。");
        let t = types(&o);
        assert_eq!(t.len(), 2, "{:?}", t);
        assert_eq!(t[0], (BlockType::Character, "剑客（内心独白）".into()));
        assert_eq!(t[1], (BlockType::Dialogue, "我是一个爱做梦的人。".into()));
    }

    #[test]
    fn p06_character_dialogue() {
        let o = p("老板娘：昨日，去了买卖城。");
        let t = types(&o);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0], (BlockType::Character, "老板娘".into()));
        assert_eq!(t[1], (BlockType::Dialogue, "昨日，去了买卖城。".into()));
    }

    #[test]
    fn p07_time_label_is_not_dialogue() {
        // 说明书第 22 条核心用例
        let o = p("时间：隆庆五年");
        assert_eq!(o.document.blocks.len(), 1);
        assert_eq!(
            o.document.blocks[0].block_type,
            BlockType::Action,
            "时间：隆庆五年 不应被识别为人物对白"
        );
    }

    #[test]
    fn p07b_more_non_character_labels() {
        for line in [
            "地点：镇北堡",
            "备注：本故事发生在宁夏",
            "标题：镇北堡往事",
            "人物：剑客、老板娘",
        ] {
            let o = p(line);
            assert_eq!(
                o.document.blocks[0].block_type,
                BlockType::Action,
                "{line} 不应被识别为人物对白"
            );
        }
    }

    #[test]
    fn p08_en_scene() {
        let o = p("INT. TAVERN - NIGHT");
        let b = &o.document.blocks[0];
        assert_eq!(b.block_type, BlockType::SceneHeading);
        let d = b.scene_heading.as_ref().unwrap();
        assert_eq!(d.int_ext, IntExt::Int);
        assert_eq!(d.location, "TAVERN");
        assert_eq!(d.time_of_day, "NIGHT");
        assert_eq!(d.display_language, "en");
    }

    #[test]
    fn p09_fountain_at_character() {
        let o = p("@剑客\n我是一个爱做梦的人。");
        let t = types(&o);
        assert_eq!(t[0].0, BlockType::Character);
        assert_eq!(t[1].0, BlockType::Dialogue);
    }

    #[test]
    fn p10_markdown_scene() {
        let o = p("## 1. 外·镇北堡城门·黄昏");
        let b = &o.document.blocks[0];
        assert_eq!(b.block_type, BlockType::SceneHeading);
        let d = b.scene_heading.as_ref().unwrap();
        assert_eq!(d.location, "镇北堡城门");
        assert_eq!(d.time_of_day, "黄昏");
    }

    #[test]
    fn p11_transition() {
        for t in ["切至：", "淡入：", "CUT TO:"] {
            let o = p(t);
            assert_eq!(o.document.blocks[0].block_type, BlockType::Transition, "{t}");
        }
    }

    #[test]
    fn p12_parenthetical_standalone() {
        let o = p("老板娘\n（轻声）\n你变了。");
        let t = types(&o);
        assert_eq!(t[0].0, BlockType::Character);
        assert_eq!(t[1].0, BlockType::Parenthetical);
        assert_eq!(t[2].0, BlockType::Dialogue);
    }

    #[test]
    fn p13_english_character_line() {
        let o = p("SWORDSMAN\nI am a dreamer.");
        let t = types(&o);
        assert_eq!(t[0], (BlockType::Character, "SWORDSMAN".into()));
        assert_eq!(t[1], (BlockType::Dialogue, "I am a dreamer.".into()));
    }

    #[test]
    fn full_zh_scene_produces_all_elements() {
        let text = "\
外 镇北堡城门 黄昏

风很大。

剑客牵马走进城门。

剑客（内心独白）：我是一个爱做梦的人。

老板娘：你夫君呢？

剑客没有说话。

老板娘：昨日去了买卖城。";
        let o = p(text);
        let t = types(&o);
        let want = vec![
            BlockType::SceneHeading,
            BlockType::Action,
            BlockType::Action,
            BlockType::Character,
            BlockType::Dialogue,
            BlockType::Character,
            BlockType::Dialogue,
            BlockType::Action,
            BlockType::Character,
            BlockType::Dialogue,
        ];
        let got: Vec<BlockType> = t.iter().map(|(bt, _)| *bt).collect();
        assert_eq!(got, want);
        assert_eq!(o.document.scenes.len(), 1);
        assert_eq!(o.document.characters.len(), 2);
    }

    #[test]
    fn no_text_loss() {
        let text = "外 镇北堡城门 黄昏\n\n风从荒原吹来。\n\n剑客（内心独白）：我是一个爱做梦的人。";
        let o = p(text);
        let total: usize = o.document.blocks.iter().map(|b| b.text.chars().count()).sum();
        assert!(total > 0);
        // 关键内容都在
        assert!(o.document.blocks.iter().any(|b| b.text.contains("风从荒原吹来")));
        assert!(o.document
            .blocks
            .iter()
            .any(|b| b.text.contains("我是一个爱做梦的人")));
    }
}

#[cfg(test)] mod photographed_format_tests {
    use super::*;
    #[test] fn delivery_qualifiers_stay_beside_character_and_share_entity() {
        let input = "内景 书房 - 日\n\n小林（画外音）：窗外有人。\n\n小林（轻声）：别开门。\n\n转场。\n\n镜头切换。小林把书放到桌上。";
        let doc = parse(input, &ParseOptions { denoise: false, ..Default::default() }).document;
        let cues: Vec<_> = doc.blocks.iter().filter(|b| b.block_type == BlockType::Character).collect();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "小林（画外音）");
        assert_eq!(doc.characters.len(), 1);
        assert_eq!(doc.characters[0].name, "小林");
        assert_eq!(cues[0].character_id, cues[1].character_id);
        assert!(doc.blocks.iter().any(|b| b.block_type == BlockType::Parenthetical && b.text == "轻声"));
        assert!(!doc.blocks.iter().any(|b| b.block_type == BlockType::Parenthetical && b.text == "画外音"));
        assert!(doc.blocks.iter().any(|b| b.block_type == BlockType::Transition && b.text == "转场。"));
        assert!(!doc.blocks.iter().any(|b| b.block_type == BlockType::Transition && b.text.contains("小林把书")));
        let source = crate::serializer::to_compact_source(&doc);
        let again = parse(&source, &ParseOptions { denoise: false, ..Default::default() }).document;
        assert!(again.blocks.iter().any(|b| b.block_type == BlockType::Character && b.text == "小林（画外音）"));
    }
    #[test] fn standalone_cue_and_separate_voice_line_are_normalized() {
        let doc = parse("小林\n（画外音）\n窗外有人。", &ParseOptions { denoise: false, ..Default::default() }).document;
        assert_eq!(doc.blocks[0].block_type, BlockType::Character);
        assert_eq!(doc.blocks[0].text, "小林（画外音）");
        assert_eq!(doc.blocks[1].block_type, BlockType::Dialogue);
    }
}


#[cfg(test)]
mod montage_tests {
    use super::*;
    #[test]
    fn isolated_montage_label_is_inline_with_action() {
        let result=parse("内景。报社——日\n\n!蒙太奇。\n\n!画面上是各种报纸的头版标题。",&ParseOptions::default());
        assert_eq!(result.document.blocks.len(),2);
        assert_eq!(result.document.blocks[1].text,"蒙太奇。画面上是各种报纸的头版标题。");
        assert_eq!(result.document.blocks[1].block_type,BlockType::Action);
    }
    #[test]
    fn montage_does_not_consume_next_scene_or_character() {
        for next in ["外景。街头——夜","@小林"] {
            let result=parse(&format!("!蒙太奇。\n\n{next}"),&ParseOptions::default());
            assert_eq!(result.document.blocks.len(),2);
        }
    }
}

#[cfg(test)]
mod narrative_camera_tests {
    use super::*;
    #[test]
    fn camera_narrative_remains_action_in_plain_and_forced_source() {
        for text in ["镜头推向窗外的雨滴。", "镜头转向门口，她走了出去。", "特写她握紧的手。", "全景中，一辆汽车驶入。"] {
            for prefix in ["", "!"] {
                let doc = parse(&format!("外景。街道——日\n\n{prefix}{text}"), &ParseOptions {denoise:false,..Default::default()}).document;
                assert_eq!(doc.blocks[1].block_type, BlockType::Action, "{prefix}{text}");
                assert_eq!(doc.blocks[1].text, text);
            }
        }
        assert!(is_shot("特写：钥匙"));
        assert!(is_shot("近景。"));
        assert!(is_shot("CLOSE ON THE KEY"));
    }
    #[test]
    fn full_chinese_mixed_scene_prefix_is_recognized() {
        for prefix in ["外景/内景", "内景 / 外景"] {
            let (data, _) = parse_scene_heading(&format!("48. {prefix}。教堂外的街道——夜晚")).unwrap();
            assert_eq!(data.int_ext, IntExt::IntExt);
            assert_eq!(data.scene_number.as_deref(), Some("48"));
        }
        let (data, _) = parse_scene_heading("外景。轮船/内景。海关——白天").unwrap();
        assert_eq!(data.int_ext, IntExt::IntExt);
        let (data, _) = parse_scene_heading("内景。教堂——白天——蒙太奇").unwrap();
        assert_eq!(data.location, "教堂");
        assert_eq!(data.time_of_day, "白天——蒙太奇");
    }
}
