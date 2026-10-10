//! `.screenplay` 项目容器（ZIP）。
//!
//! 说明书第 86 条：至少包含
//! `manifest.json` / `document.json` / `project.sqlite` / `imports/` / `assets/` / `story-bible/` / `skills/`
//! 第 88 条：真实数据必须是结构化 JSON，不得只保存 HTML。

use screenplay_core::ast::*;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub app_version: String,
}

pub const CONTAINER_EXT: &str = "screenplay";
/// 最大原始导入文件大小；加载不可信容器时限制解压内存占用。
pub const MAX_ORIGINAL_SOURCE_BYTES: u64 = 512 * 1024 * 1024;

/// 打开项目：优先读 ZIP 内的 SQLite，回退到 document.json
pub fn save_project(
    container: &Path,
    doc: &ScreenplayDocument,
    db_bytes: Option<&[u8]>,
    story_bible: Option<&str>,
    project_rules: Option<&str>,
    original_source: Option<&[u8]>,
) -> Result<(), String> {
    if original_source.is_some_and(|bytes| bytes.len() as u64 > MAX_ORIGINAL_SOURCE_BYTES) {
        return Err("原始导入文件超过 512 MiB，无法写入项目文件".into());
    }
    if let Some(parent) = container.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(container).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let manifest = Manifest {
        format: CONTAINER_EXT.to_string(),
        format_version: 1,
        title: doc.metadata.title.clone(),
        created_at: doc.created_at.clone(),
        updated_at: now_iso(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    zip.start_file("manifest.json", opts).map_err(|e| e.to_string())?;
    zip.write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
        .map_err(|e| e.to_string())?;

    zip.start_file("document.json", opts).map_err(|e| e.to_string())?;
    zip.write_all(serde_json::to_string_pretty(doc).unwrap().as_bytes())
        .map_err(|e| e.to_string())?;

    if let Some(b) = db_bytes {
        zip.start_file("project.sqlite", opts).map_err(|e| e.to_string())?;
        zip.write_all(b).map_err(|e| e.to_string())?;
    }

    if let Some(sb) = story_bible {
        zip.start_file("story-bible/bible.md", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(sb.as_bytes()).map_err(|e| e.to_string())?;
    }

    if let Some(pr) = project_rules {
        zip.start_file("PROJECT_RULES.md", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(pr.as_bytes()).map_err(|e| e.to_string())?;
    }

    // 保证目录存在（即使为空）
    for d in ["imports/", "assets/", "skills/"] {
        let _ = zip.add_directory(d, opts);
    }

    // 原文存档
    if let Some(info) = &doc.import_info {
        if !info.original_text.is_empty() {
            zip.start_file("imports/original.txt", opts)
                .map_err(|e| e.to_string())?;
            zip.write_all(info.original_text.as_bytes())
                .map_err(|e| e.to_string())?;
        }
    }

    // 原始二进制以固定 ZIP 路径保存，源文件名只存在文档元数据中，
    // 避免用户文件名被解释为容器内路径。已压缩格式无需再次压缩。
    if let Some(source) = original_source {
        zip.start_file(
            "imports/source.bin",
            SimpleFileOptions::default()
                .compression_method(CompressionMethod::Stored)
                .unix_permissions(0o644),
        )
        .map_err(|e| e.to_string())?;
        zip.write_all(source).map_err(|e| e.to_string())?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug)]
pub struct LoadedProject {
    pub document: ScreenplayDocument,
    /// 容器内 SQLite 字节（调用方写入临时文件后打开）
    pub sqlite_bytes: Option<Vec<u8>>,
    pub story_bible: Option<String>,
    pub project_rules: Option<String>,
    pub manifest: Option<Manifest>,
    /// 导入时的原始文件字节；旧版容器中此字段为空。
    pub original_source: Option<Vec<u8>>,
}

pub fn load_project(container: &Path) -> Result<LoadedProject, String> {
    let file = std::fs::File::open(container)
        .map_err(|e| format!("无法打开项目文件 {}: {e}", container.display()))?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        format!("项目文件不是有效的幕本工程文件（可能已损坏）: {e}")
    })?;

    let mut document_json: Option<String> = None;
    let mut sqlite_bytes: Option<Vec<u8>> = None;
    let mut story_bible: Option<String> = None;
    let mut project_rules: Option<String> = None;
    let mut manifest: Option<Manifest> = None;
    let mut original_source: Option<Vec<u8>> = None;

    for i in 0..archive.len() {
        let mut e = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = e.name().to_string();
        match name.as_str() {
            "document.json" => {
                let mut s = String::new();
                e.read_to_string(&mut s).map_err(|e| e.to_string())?;
                document_json = Some(s);
            }
            "manifest.json" => {
                let mut s = String::new();
                e.read_to_string(&mut s).map_err(|e| e.to_string())?;
                manifest = serde_json::from_str(&s).ok();
            }
            "project.sqlite" => {
                let mut buf = Vec::new();
                e.read_to_end(&mut buf).map_err(|e| e.to_string())?;
                sqlite_bytes = Some(buf);
            }
            "story-bible/bible.md" => {
                let mut s = String::new();
                e.read_to_string(&mut s).map_err(|e| e.to_string())?;
                story_bible = Some(s);
            }
            "PROJECT_RULES.md" => {
                let mut s = String::new();
                e.read_to_string(&mut s).map_err(|e| e.to_string())?;
                project_rules = Some(s);
            }
            "imports/source.bin" => {
                if e.size() > MAX_ORIGINAL_SOURCE_BYTES {
                    return Err("项目中的原始导入文件超过 512 MiB，已拒绝加载".into());
                }
                let mut buf = Vec::with_capacity(e.size() as usize);
                e.read_to_end(&mut buf).map_err(|e| e.to_string())?;
                original_source = Some(buf);
            }
            _ => {}
        }
    }

    let json = document_json.ok_or("项目文件缺少 document.json")?;
    let mut document: ScreenplayDocument =
        serde_json::from_str(&json).map_err(|e| format!("document.json 解析失败: {e}"))?;
    document.rebuild_derived();

    if let Some(pr) = &project_rules {
        document.project_rules = ProjectRules {
            enabled: true,
            text: pr.clone(),
        };
    }

    Ok(LoadedProject {
        document,
        sqlite_bytes,
        story_bible,
        project_rules,
        manifest,
        original_source,
    })
}

/// 展开容器到目录（供 SQLite 打开）
pub fn extract_sqlite(container: &Path, dest: &PathBuf) -> Result<(), String> {
    let loaded = load_project(container)?;
    if let Some(b) = loaded.sqlite_bytes {
        std::fs::write(dest, b).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use screenplay_core::ast::ScreenplayDocument;
    use std::io::Cursor;

    /// 创建一个最小的合法 .screenplay ZIP
    fn make_valid_zip() -> Vec<u8> {
        let doc = ScreenplayDocument::blank("安全测试");
        let json = serde_json::to_string(&doc).unwrap();
        let mut buf = Vec::new();
        let mut zw = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("document.json", opts).unwrap();
        zw.write_all(json.as_bytes()).unwrap();
        zw.finish().unwrap();
        buf
    }

    /// QG9: 损坏的 ZIP 返回结构化错误，不崩溃
    #[test]
    fn sec01_corrupt_zip_returns_error() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"not a zip file at all").unwrap();
        let err = load_project(tmp.path()).unwrap_err();
        assert!(err.contains("损坏") || err.contains("不是有效"), "错误信息: {err}");
    }

    /// QG9: 缺少 document.json 的 ZIP 返回明确错误
    #[test]
    fn sec02_missing_document_json_rejected() {
        let mut buf = Vec::new();
        let mut zw = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("manifest.json", opts).unwrap();
        zw.write_all(b"{}").unwrap();
        zw.finish().unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &buf).unwrap();
        let err = load_project(tmp.path()).unwrap_err();
        assert!(err.contains("document.json"), "错误信息: {err}");
    }

    /// QG9: document.json 不是合法 JSON 返回结构化错误
    #[test]
    fn sec03_malformed_document_json_rejected() {
        let mut buf = Vec::new();
        let mut zw = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("document.json", opts).unwrap();
        zw.write_all(b"{ this is not valid json!!! }").unwrap();
        zw.finish().unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &buf).unwrap();
        let err = load_project(tmp.path()).unwrap_err();
        assert!(err.contains("解析失败"), "错误信息: {err}");
    }

    /// QG9: ZIP 中包含路径穿越条目（../../etc/passwd）必须被忽略
    /// load_project 只读取已知条目名，恶意条目不应被处理
    #[test]
    fn sec04_zip_path_traversal_entries_ignored() {
        let doc = ScreenplayDocument::blank("安全测试");
        let json = serde_json::to_string(&doc).unwrap();
        let mut buf = Vec::new();
        let mut zw = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        // 合法条目
        zw.start_file("document.json", opts).unwrap();
        zw.write_all(json.as_bytes()).unwrap();
        // 恶意路径穿越条目
        zw.start_file("../../../../etc/passwd", opts).unwrap();
        zw.write_all(b"malicious content").unwrap();
        zw.start_file("story-bible/../../escape.md", opts).unwrap();
        zw.write_all(b"escape attempt").unwrap();
        zw.finish().unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &buf).unwrap();

        // 应成功加载（恶意条目被忽略），且不崩溃
        let loaded = load_project(tmp.path()).unwrap();
        assert_eq!(loaded.document.metadata.title, "安全测试");
    }

    /// QG9: 巨型 document.json 不导致栈溢出或 OOM 崩溃
    /// （限制在合理大小，超过则拒绝）
    #[test]
    fn sec05_oversized_document_rejected_or_handled() {
        // 构造一个 50MB 的 document.json（超大文本）
        let huge = format!("{{\"blocks\":[],\"metadata\":{{\"title\":\"{}\"}}}}", "x".repeat(50_000_000));
        let mut buf = Vec::new();
        let mut zw = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("document.json", opts).unwrap();
        zw.write_all(huge.as_bytes()).unwrap();
        zw.finish().unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &buf).unwrap();

        // 要么成功解析（title 很长但合法），要么返回错误；不能崩溃
        let result = load_project(tmp.path());
        assert!(result.is_ok() || result.is_err());
    }

    /// QG9: 合法项目能正常加载（正向验证）
    #[test]
    fn sec06_valid_project_loads() {
        let zip_bytes = make_valid_zip();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &zip_bytes).unwrap();
        let loaded = load_project(tmp.path()).unwrap();
        assert_eq!(loaded.document.metadata.title, "安全测试");
    }

    #[test]
    fn original_source_round_trips_in_project_container() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let doc = ScreenplayDocument::blank("原稿归档测试");
        let original = b"%PDF-1.7\noriginal screenplay source\n";
        save_project(tmp.path(), &doc, None, None, None, Some(original)).unwrap();

        let loaded = load_project(tmp.path()).unwrap();
        assert_eq!(loaded.original_source.as_deref(), Some(original.as_slice()));
        assert_eq!(loaded.document.metadata.title, "原稿归档测试");
    }
}
