//! 场景识别词典

use regex::Regex;
use std::sync::OnceLock;

use crate::ast::IntExt;

/// 中文时间词（长词在前，便于最长匹配）
pub const TIME_ZH: &[&str] = &[
    "清晨", "早晨", "上午", "中午", "下午", "黄昏", "傍晚", "深夜", "夜晚", "黎明", "白天",
    "入夜", "稍后", "连续", "夜晚", "日", "夜", "晨",
];

pub const TIME_EN: &[&str] = &[
    "MOMENTS LATER",
    "CONTINUOUS",
    "MORNING",
    "AFTERNOON",
    "EVENING",
    "DAWN",
    "DUSK",
    "LATER",
    "NIGHT",
    "DAY",
    "SAME",
];

pub const TRANSITIONS_ZH: &[&str] = &[
    "切至", "淡入", "淡出", "黑场", "闪白", "闪回", "叠化", "快切", "硬切", "转场", "溶入", "溶出",
];

pub const TRANSITIONS_EN: &[&str] = &[
    "CUT TO BLACK",
    "FADE TO BLACK",
    "SMASH CUT TO",
    "MATCH CUT TO",
    "JUMP CUT TO",
    "DISSOLVE TO",
    "CUT TO",
    "FADE IN",
    "FADE OUT",
    "SMASH CUT",
    "MATCH CUT",
    "JUMP CUT",
    "WIPE TO",
];

/// 动作前缀标记
pub const ACTION_PREFIXES: &[char] = &['△', '▲', '•', '▪', '◦', '*', '·'];

/// 镜头词
pub const SHOTS_ZH: &[&str] = &["大特写", "特写", "近景", "中景", "全景", "远景", "俯拍", "仰拍", "航拍", "镜头"];

pub const SHOTS_EN: &[&str] = &[
    "EXTREME CLOSE UP",
    "CLOSE ON",
    "CLOSE UP",
    "WIDE SHOT",
    "MEDIUM SHOT",
    "MEDIUM CLOSE UP",
    "OVER THE SHOULDER",
    "TWO SHOT",
    "INSERT SHOT",
    "POV",
    "INSERT",
];

/// 中文冒号前缀中，明确 NOT 人物对白的标签
/// 说明书第 22 条：`时间：隆庆五年` 不能识别为人物对白
pub const NON_CHARACTER_LABELS_ZH: &[&str] = &[
    "时间", "地点", "备注", "说明", "标题", "作者", "编剧", "日期", "版本", "场景", "人物",
    "道具", "服装", "化妆", "造型", "场记", "导演", "摄影", "美术", "灯光", "录音", "音乐",
    "音效", "字幕", "旁白", "画外音", "集数", "时长", "类型", "状态", "类型设计", "颜色", "目标",
    "集", "幕", "场", "镜", "注意", "警告", "提示",
];

pub const NON_CHARACTER_LABELS_EN: &[&str] = &[
    "NOTE", "TODO", "FYI", "WARNING", "TITLE", "AUTHOR", "DATE", "VERSION", "SCENE", "CHARACTER",
    "PROP", "COSTUME", "MAKEUP", "DIRECTOR", "DOP", "INT", "EXT",
];

/// 内外景前缀（长词在前）
pub fn int_ext_prefixes() -> &'static [&'static str] {
    &[
        "INT/EXT", "EXT/INT", "I/E.", "I/E", "INT.", "EXT.", "INT", "EXT", "内/外", "外/内", "内．外",
        "外．内", "内外", "外内", "内景", "外景", "室内", "室外", "内", "外",
    ]
}

pub fn int_ext_from_prefix(s: &str) -> Option<IntExt> {
    let u = s.to_ascii_uppercase();
    Some(match u.as_str() {
        "INT" | "INT." | "内" | "内景" | "室内" => IntExt::Int,
        "EXT" | "EXT." | "外" | "外景" | "室外" => IntExt::Ext,
        "INT/EXT" | "EXT/INT" | "I/E" | "I/E." | "内/外" | "外/内" | "内外" | "外内" | "内．外"
        | "外．内" | "内景/外景" | "外景/内景" => IntExt::IntExt,
        _ => return None,
    })
}

pub fn has_chinese(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// 场景标题整体正则：可选编号 + 内外景 + 地点 + 时间
pub fn scene_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?x)
            ^\s*
            (?:(?P<num>\d{1,4}[A-Za-z]?|[零〇一二三四五六七八九十百千万]{1,12})\s*[.、,)）:：]?\s*)?      # 可选场景编号 "1." 或 "1 "
            (?P<int_ext>内景\s*/\s*外景|外景\s*/\s*内景|内\s*/\s*外|外\s*/\s*内|内\s*[．.]\s*外|外\s*[．.]\s*内|内外|外内|内景|外景|室内|室外|内|外|INT\s*/\s*EXT|EXT\s*/\s*INT|I\s*/\s*E|INT|EXT)
            (?:\s*[.．。、,，:：·・\-—–]\s*|\s+)                      # 分隔符
            (?P<rest>.+?)                                 # 地点 + 时间
            \s*$"#,
        )
        .unwrap()
    })
}

/// 英文全大写人物行（不含数字、不含句点结尾）
pub fn is_english_character_line(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.chars().count() > 40 {
        return false;
    }
    if t.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if t.contains('。') || t.contains('，') {
        return false;
    }
    // 必须是全部大写字母 / 空格 / . ' - ( ) /
    t.chars()
        .all(|c| c.is_ascii_uppercase() || " .'-()/".contains(c))
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

/// 括注行：（…）或 (…)
pub fn is_parenthetical_line(s: &str) -> bool {
    let t = s.trim();
    if t.chars().count() > 40 {
        return false;
    }
    (t.starts_with('（') && t.ends_with('）'))
        || (t.starts_with('(') && t.ends_with(')'))
        || (t.starts_with('【') && t.ends_with('】'))
        || (t.starts_with('[') && t.ends_with(']'))
}

pub fn strip_parens(s: &str) -> String {
    let t = s.trim();
    let t = t
        .trim_start_matches(['（', '(', '【', '['])
        .trim_end_matches(['）', ')', '】', ']']);
    t.trim().to_string()
}
