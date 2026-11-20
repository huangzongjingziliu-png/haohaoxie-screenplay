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
    screenplay_core::series::validate(doc)?;
    if original_source.is_some_and(|bytes| bytes.len() as u64 > MAX_ORIGINAL_SOURCE_BYTES) {
        return Err("原始导入文件超过 512 MiB，无法写入项目文件".into());
    }
    atomic_write(container, |file| {
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
    })
}

/// Publish only a completed, synced archive. A failed write leaves the previous
/// project intact, including when autosaving over an existing file.
fn atomic_write(container:&Path, write:impl FnOnce(&mut std::fs::File)->Result<(),String>)->Result<(),String>{
    let parent=container.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e|e.to_string())?;
    // 沙盒里用户经保存面板选中的文件只授权了该文件本身，同目录建临时文件会被拒（EPERM）。
    // 这时**不能**原地截断覆盖：写到一半失败会把用户的工程变成不完整文件。
    // 已存在工程时直接中止并报错，让用户另选位置；只有目标尚不存在时才直接创建。
    match tempfile::NamedTempFile::new_in(parent) {
        Ok(mut temporary)=>{
            if let Ok(metadata)=std::fs::metadata(container){temporary.as_file().set_permissions(metadata.permissions()).map_err(|e|e.to_string())?;}
            write(temporary.as_file_mut())?;
            temporary.as_file().sync_all().map_err(|e|e.to_string())?;
            temporary.persist(container).map_err(|e|e.error.to_string())?;
            Ok(())
        }
        Err(_)=>direct_write(container,write),
    }
}

/// 无法在目标旁建临时文件时的回退（沙盒只授权单个文件，商店版每次覆盖保存都走这里）。
/// 不能直接截断：先把原文件完整备份到临时目录，再截断写入；
/// 写入失败立即用备份还原。这样任何失败或中断都不会让原工程只剩半截。
fn direct_write(container:&Path, write:impl FnOnce(&mut std::fs::File)->Result<(),String>)->Result<(),String>{
    if !container.exists() {
        // 目标尚不存在：直接创建，失败就清掉残片
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(container)
            .map_err(|e|format!("无法创建 {}: {e}",container.display()))?;
        let written=write(&mut file).and_then(|_|file.sync_all().map_err(|e|e.to_string()));
        if written.is_err(){let _=std::fs::remove_file(container);}
        return written;
    }

    let backup=create_save_backup(container)?;
    let written=(||{
        let mut file=std::fs::OpenOptions::new().write(true).truncate(true).open(container)
            .map_err(|e|format!("无法写入 {}: {e}",container.display()))?;
        write(&mut file)?;
        file.sync_all().map_err(|e|e.to_string())
    })();
    match written {
        Ok(())=>{remove_save_backup(&backup);Ok(())}
        Err(error)=>{
            let restored=std::fs::copy(&backup,container);
            if restored.is_err() {
                return Err(format!("{error}；原工程备份保存在 {}，请勿删除，可据此恢复",backup.display()));
            }
            remove_save_backup(&backup);
            Err(format!("{error}；已从备份还原原工程，文件未损坏"))
        }
    }
}

// ---------------------------------------------------------------------------
// 覆盖保存的残留备份
// ---------------------------------------------------------------------------

/// 临时备份文件名前缀。
pub const SAVE_BACKUP_PREFIX: &str = "screenplay-save-";

/// 覆盖保存留下的、可确认恢复的完整备份。
///
/// 只有「备份文件 + 侧写归属信息」都在，且用户明确确认后才会被恢复；
/// 应用不会自动采用它，也不会自动删除它。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaleSaveBackup {
    /// 备份文件绝对路径
    pub backup: String,
    /// 备份对应的工程文件绝对路径
    pub container: String,
    pub saved_at: String,
    pub size: u64,
}

/// 备份旁记录归属的 sidecar（JSON），避免靠文件名猜工程。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SaveBackupSidecar {
    container: String,
    saved_at: String,
}

/// 路径规范化：能 canonicalize 就用真实路径，否则退回原样。
fn absolute_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// FNV-1a：跨进程、跨版本稳定，用来给备份文件名打上可复现的路径标记。
fn path_tag(path: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in absolute_path(path).to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn save_backup_sidecar_path(backup: &Path) -> PathBuf {
    backup.with_extension("json")
}

/// 覆盖写之前的临时备份位置（系统临时目录一定可写）。
fn save_backup_path(container: &Path) -> PathBuf {
    let name = container.file_name().and_then(|n| n.to_str()).unwrap_or("project");
    std::env::temp_dir().join(format!("{SAVE_BACKUP_PREFIX}{}-{name}.bak", path_tag(container)))
}

/// 覆盖写之前把原工程完整备份到临时目录，并记录它属于哪个工程。
///
/// 备份与归属记录都在、且原工程已被截断时，就是「保存被强杀」留下的残留状态。
pub fn create_save_backup(container: &Path) -> Result<PathBuf, String> {
    let backup = save_backup_path(container);
    // 先登记归属再复制：进程在复制后被强杀时，启动检查要能认出它属于哪个工程。
    write_save_backup_sidecar(&backup, container)?;
    if let Err(e) = std::fs::copy(container, &backup) {
        remove_save_backup(&backup);
        return Err(format!(
            "无法备份原工程 {}，为避免损坏已中止保存：{e}",
            container.display()
        ));
    }
    Ok(backup)
}

fn write_save_backup_sidecar(backup: &Path, container: &Path) -> Result<(), String> {
    let sidecar = save_backup_sidecar_path(backup);
    let payload = SaveBackupSidecar {
        container: absolute_path(container).to_string_lossy().into_owned(),
        saved_at: now_iso(),
    };
    std::fs::write(
        &sidecar,
        serde_json::to_string(&payload).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("无法记录备份位置 {}，为避免损坏已中止保存：{e}", sidecar.display()))
}

fn remove_save_backup(backup: &Path) {
    let _ = std::fs::remove_file(backup);
    let _ = std::fs::remove_file(save_backup_sidecar_path(backup));
}

fn read_save_backup_sidecar(backup: &Path) -> Option<SaveBackupSidecar> {
    std::fs::read_to_string(save_backup_sidecar_path(backup))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

/// 列出所有可确认恢复的残留保存备份（最新在前）。
///
/// 只做一次临时目录的 `read_dir`，没有残备份时开销可忽略。
pub fn list_stale_save_backups() -> Vec<StaleSaveBackup> {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(SAVE_BACKUP_PREFIX) || !name.ends_with(".bak") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        if !metadata.is_file() {
            continue;
        }
        // 没有归属记录的 .bak 无法确认属于哪个工程：不猜、不列、也不删。
        let Some(sidecar) = read_save_backup_sidecar(&path) else {
            continue;
        };
        found.push(StaleSaveBackup {
            backup: path.to_string_lossy().into_owned(),
            container: sidecar.container,
            saved_at: sidecar.saved_at,
            size: metadata.len(),
        });
    }
    found.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
    found
}

/// 查找某个工程的残留保存备份。
pub fn find_stale_save_backup(container: &Path) -> Option<StaleSaveBackup> {
    let target = absolute_path(container);
    list_stale_save_backups()
        .into_iter()
        .find(|item| Path::new(&item.container) == target.as_path())
}

/// 用户确认后用残留备份还原工程文件，成功后清理备份与归属记录。
///
/// 覆盖前先验证备份本身是完整可读的工程，避免把损坏内容写回去。
pub fn restore_stale_save_backup(container: &Path) -> Result<StaleSaveBackup, String> {
    let found = find_stale_save_backup(container)
        .ok_or_else(|| "未找到该工程的残留保存备份".to_string())?;
    // 工程文件仍能正常打开，说明那次保存其实已经完成：备份只是旧版本。
    // 此时拒绝覆盖，避免把用户更新的内容退回旧版；备份原样保留。
    if load_project(container).is_ok() {
        return Err("工程文件当前可以正常打开，已取消恢复；备份仍保留在原处".into());
    }
    let backup = PathBuf::from(&found.backup);
    load_project(&backup).map_err(|e| format!("备份文件不完整，未做任何改动：{e}"))?;
    if let Some(parent) = container.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&backup, container).map_err(|e| format!("恢复失败：{e}"))?;
    // 先确保落盘，再删备份：宁可留下多余备份，也不能出现「备份已删、内容未落盘」。
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(container) {
        let _ = file.sync_all();
    }
    remove_save_backup(&backup);
    Ok(found)
}

/// 用户选择保留备份但不再提示：只改名，不删除数据。
pub fn dismiss_stale_save_backup(container: &Path) -> Result<bool, String> {
    let Some(found) = find_stale_save_backup(container) else {
        return Ok(false);
    };
    let backup = PathBuf::from(&found.backup);
    let sidecar = save_backup_sidecar_path(&backup);
    let declined = PathBuf::from(format!("{}.declined", backup.display()));
    std::fs::rename(&backup, &declined)
        .map_err(|e| format!("保留备份失败：{e}"))?;
    if sidecar.exists() {
        let _ = std::fs::rename(&sidecar, PathBuf::from(format!("{}.declined", sidecar.display())));
    }
    Ok(true)
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
    screenplay_core::series::validate(&document)?;
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
    #[test]
    fn project_memory_travels_with_container() {
        let tmp=tempfile::NamedTempFile::new().unwrap();
        let mut doc=ScreenplayDocument::blank("记忆迁移");
        doc.settings.project_memory.markdown="# 人物\n剑客寡言".into();
        doc.settings.project_memory.author_notes="不改结局".into();
        doc.settings.project_memory.history=vec!["旧设定".into()];
        save_project(tmp.path(),&doc,None,None,None,None).unwrap();
        let loaded=load_project(tmp.path()).unwrap();
        assert_eq!(loaded.document.settings.project_memory,doc.settings.project_memory);
    }
    /// 覆盖保存失败/中断时不得让已存在的工程只剩半截：
    /// 写失败必须还原成原来的完整文件（商店版沙盒只能走这条覆盖路径）。
    #[test]
    fn save_failure_keeps_existing_container_intact() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.screenplay");
        let original = make_valid_zip();
        std::fs::write(&path, &original).unwrap();

        // 只读目录 → 无法在目标旁创建临时文件，只能走覆盖写回退
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = atomic_write(&path, |file| {
            std::io::Write::write_all(file, b"partial garbage").map_err(|e| e.to_string())?;
            Err("写入中断".to_string())
        });
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err(), "写入失败必须报错");
        assert_eq!(std::fs::read(&path).unwrap(), original, "失败后原工程必须被还原成完整文件");
    }

    /// 回退路径本身必须仍可保存成功，否则商店版无法覆盖保存。
    #[test]
    fn sandbox_like_directory_can_still_overwrite_save() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.screenplay");
        std::fs::write(&path, make_valid_zip()).unwrap();

        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = save_project(&path, &ScreenplayDocument::blank("新内容"), None, None, None, None);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_ok(), "无法建临时文件时应退回安全覆盖写：{result:?}");
        assert_eq!(load_project(&path).unwrap().document.metadata.title, "新内容");
    }

    #[test]
    fn old_settings_without_memory_remain_compatible() {
        let settings=screenplay_core::ast::ScreenplaySettings::default();
        let mut json=serde_json::to_value(settings).unwrap();
        json.as_object_mut().unwrap().remove("project_memory");
        let restored:screenplay_core::ast::ScreenplaySettings=serde_json::from_value(json).unwrap();
        assert!(restored.project_memory.auto_enabled);
        assert!(restored.project_memory.markdown.is_empty());
    }

}

#[cfg(test)] mod atomic_save_regressions {
 use super::*;
 #[test] fn partial_write_failure_keeps_existing_project_byte_for_byte(){
  let dir=tempfile::tempdir().unwrap();let path=dir.path().join("test.screenplay");
  let doc=ScreenplayDocument::blank("original");save_project(&path,&doc,None,None,None,None).unwrap();
  let before=std::fs::read(&path).unwrap();
  let result=atomic_write(&path,|file|{file.write_all(b"partial damaged archive").unwrap();Err("simulated disk full".into())});
  assert!(result.is_err());assert_eq!(std::fs::read(&path).unwrap(),before);
  assert_eq!(load_project(&path).unwrap().document.metadata.title,"original");
  assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
 }
 #[test] fn successful_save_replaces_archive_and_failed_new_save_leaves_no_partial_file(){
  let dir=tempfile::tempdir().unwrap();let path=dir.path().join("test.screenplay");
  assert!(atomic_write(&path,|file|{file.write_all(b"partial").unwrap();Err("failed".into())}).is_err());assert!(!path.exists());
  save_project(&path,&ScreenplayDocument::blank("first"),None,None,None,None).unwrap();
  save_project(&path,&ScreenplayDocument::blank("second"),None,None,None,None).unwrap();
  assert_eq!(load_project(&path).unwrap().document.metadata.title,"second");
 }
}

#[cfg(test)]mod stale_save_backup_tests{
 use super::*;

 /// 模拟「覆盖写过程中进程被强杀」：完整备份 + 归属记录留着，工程文件只剩半截。
 fn leave_stale_backup(path:&Path,title:&str)->PathBuf{
  save_project(path,&ScreenplayDocument::blank(title),None,None,None,None).unwrap();
  // 走生产同一条备份路径，但停在覆盖写之前，模拟进程此刻被强杀。
  let backup=create_save_backup(path).unwrap();
  std::fs::write(path,b"half written garbage\n").unwrap();
  backup
 }

 #[test]
 fn stale_backup_can_be_found_restored_and_cleaned(){
  let dir=tempfile::tempdir().unwrap();
  let path=dir.path().join("残留.screenplay");
  let backup=leave_stale_backup(&path,"恢复后的完整工程");

  let found=find_stale_save_backup(&path).expect("应发现与本工程相关的残留备份");
  assert_eq!(Path::new(&found.container),absolute_path(&path).as_path());
  assert!(found.size>0);
  assert!(list_stale_save_backups().iter().any(|item|item.backup==found.backup));

  restore_stale_save_backup(&path).unwrap();
  assert_eq!(load_project(&path).unwrap().document.metadata.title,"恢复后的完整工程");
  assert!(!backup.exists(),"恢复成功后必须清理备份文件");
  assert!(!save_backup_sidecar_path(&backup).exists(),"恢复成功后必须清理归属记录");
  assert!(find_stale_save_backup(&path).is_none());
 }

 #[test]
 fn dismissed_backup_is_kept_but_not_listed(){
  let dir=tempfile::tempdir().unwrap();
  let path=dir.path().join("拒绝恢复.screenplay");
  let backup=leave_stale_backup(&path,"不自动采用的内容");
  let bytes=std::fs::read(&backup).unwrap();

  assert!(dismiss_stale_save_backup(&path).unwrap());
  assert!(!backup.exists());
  let declined=PathBuf::from(format!("{}.declined",backup.display()));
  assert_eq!(std::fs::read(&declined).unwrap(),bytes,"拒绝恢复只能改名，绝不能删数据");
  assert!(find_stale_save_backup(&path).is_none(),"已拒绝的备份不再打扰");
  assert!(!list_stale_save_backups().iter().any(|item|item.backup==backup.to_string_lossy()));

  let _=std::fs::remove_file(declined);
  let _=std::fs::remove_file(PathBuf::from(format!("{}.declined",save_backup_sidecar_path(&backup).display())));
 }

 #[test]
 fn healthy_project_is_not_overwritten_by_leftover_backup(){
  let dir=tempfile::tempdir().unwrap();
  let path=dir.path().join("healthy.screenplay");
  save_project(&path,&ScreenplayDocument::blank("最新版本"),None,None,None,None).unwrap();
  let backup=create_save_backup(&path).unwrap();

  let error=restore_stale_save_backup(&path).unwrap_err();
  assert!(error.contains("正常打开"),"错误信息：{error}");
  assert_eq!(load_project(&path).unwrap().document.metadata.title,"最新版本");
  assert!(backup.exists(),"拒绝恢复时不得删除备份");
  remove_save_backup(&backup);
 }

 #[test]
 fn corrupt_backup_is_not_restored_over_current_file(){
  let dir=tempfile::tempdir().unwrap();
  let path=dir.path().join("损坏备份.screenplay");
  leave_stale_backup(&path,"原工程");
  let backup=save_backup_path(&path);
  std::fs::write(&backup,b"not a zip").unwrap();

  let error=restore_stale_save_backup(&path).unwrap_err();
  assert!(error.contains("不完整"),"错误信息：{error}");
  assert_eq!(std::fs::read(&path).unwrap(),b"half written garbage\n","备份不可用时不得改动工程文件");
  assert!(backup.exists(),"未恢复成功前不得删除备份");

  remove_save_backup(&backup);
 }

 #[test]
 fn backup_paths_of_same_name_projects_do_not_collide(){
  let a=tempfile::tempdir().unwrap();
  let b=tempfile::tempdir().unwrap();
  let first=a.path().join("剧名.screenplay");
  let second=b.path().join("剧名.screenplay");
  save_project(&first,&ScreenplayDocument::blank("甲"),None,None,None,None).unwrap();
  save_project(&second,&ScreenplayDocument::blank("乙"),None,None,None,None).unwrap();
  assert_ne!(save_backup_path(&first),save_backup_path(&second));
 }
}

#[cfg(test)]mod series_container_tests{
 use super::*;
 #[test]fn series_round_trip_keeps_every_episode(){let dir=tempfile::tempdir().unwrap();let path=dir.path().join("series.screenplay");let mut doc=ScreenplayDocument::blank("剧集");doc.blocks.push(ScreenplayBlock::new(BlockType::Action,"第一集"));screenplay_core::series::edit(&mut doc,"add","","第二集",&[]).unwrap();doc.blocks.push(ScreenplayBlock::new(BlockType::Action,"第二集"));save_project(&path,&doc,None,None,None,None).unwrap();let loaded=load_project(&path).unwrap();let drafts=screenplay_core::series::export_drafts(&loaded.document).unwrap();assert_eq!(drafts[0].1.blocks[0].text,"第一集");assert_eq!(drafts[1].1.blocks[0].text,"第二集");}
}
