//! Workspace —— 一个「已打开的项目」。
//!
//! 绑定：项目容器路径、SQLite 连接、Command Service、当前文档。
//! CLI、MCP、GUI 都通过 Workspace 访问项目。

use crate::command::{CommandService, CommandResult};
use crate::db::{open_db, Store};
use crate::project;
use screenplay_core::ast::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

/// 打开一个已保存的项目
pub struct Workspace {
    /// 容器路径（None = 未保存的新项目）
    /// 另存为后必须更新，否则项目身份与实际落盘路径不一致。
    container: Mutex<Option<PathBuf>>,
    /// SQLite 工作副本路径
    pub db_path: PathBuf,
    pub service: CommandService,
    pub dirty: std::sync::atomic::AtomicBool,
    /// 原始导入字节，随项目保存并在重开时恢复。
    original_source: Mutex<Option<Vec<u8>>>,
}

impl Workspace {
    /// 打开已有项目
    pub fn open(container: &Path) -> Result<Self, String> {
        let mut loaded = project::load_project(container)?;
        let repaired=screenplay_core::typography::normalize_parentheticals(&mut loaded.document);
        if repaired {
            loaded.document.revision+=1;loaded.document.updated_at=now_iso();loaded.document.rebuild_derived();
            let source=screenplay_core::serializer::to_compact_source(&loaded.document);
            crate::command::parser_fix_ranges(&mut loaded.document,&source);
        }
        let original_source = loaded.original_source;

        // SQLite 提取到临时工作副本
        let workdir = std::env::temp_dir().join(format!("screenplay-{}", new_id("ws")));
        std::fs::create_dir_all(&workdir).map_err(|e| e.to_string())?;
        let db_path = workdir.join("project.sqlite");
        if let Some(b) = loaded.sqlite_bytes {
            std::fs::write(&db_path, b).map_err(|e| e.to_string())?;
        }

        let conn = open_db(&db_path).map_err(|e| format!("打开项目数据库失败: {e}"))?;
        let store = Arc::new(Store::new(conn, loaded.document));
        Ok(Self {
            container: Mutex::new(Some(crate::bridge::canonical_container_path(container))),
            db_path,
            service: CommandService::new(store),
            dirty: std::sync::atomic::AtomicBool::new(repaired),
            original_source: Mutex::new(original_source),
        })
    }

    /// 新建空白项目
    pub fn blank(title: &str) -> Result<Self, String> {
        let workdir = std::env::temp_dir().join(format!("screenplay-{}", new_id("ws")));
        std::fs::create_dir_all(&workdir).map_err(|e| e.to_string())?;
        let db_path = workdir.join("project.sqlite");
        let conn = open_db(&db_path).map_err(|e| e.to_string())?;
        let store = Arc::new(Store::new(conn, ScreenplayDocument::blank(title)));
        Ok(Self {
            container: Mutex::new(None),
            db_path,
            service: CommandService::new(store),
            dirty: std::sync::atomic::AtomicBool::new(true),
            original_source: Mutex::new(None),
        })
    }

    pub fn document(&self) -> ScreenplayDocument {
        self.service.store.snapshot()
    }

    pub fn revision(&self) -> u64 {
        self.service.store.revision()
    }

    /// 标记已修改
    pub fn touch(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// 保存到容器
    pub fn save(&self, container: Option<&Path>) -> Result<PathBuf, String> {
        self.save_with_revision(container).map(|(path, _)| path)
    }

    /// Return the revision of the exact serialized snapshot, not a later live revision.
    pub fn save_with_revision(&self, container: Option<&Path>) -> Result<(PathBuf, u64), String> {
        let target = match container {
            Some(p) => p.to_path_buf(),
            None => self
                .container_path()
                .ok_or("尚未指定保存路径，请使用 save-as")?,
        };
        let target=crate::bridge::canonical_container_path(&target);
        let doc = self.document();
        // 导出 SQLite 为字节（先 checkpoint WAL，否则最新写入在 -wal 文件中）
        {
            let conn = self.service.store.conn.lock().unwrap();
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
        let db_bytes = std::fs::read(&self.db_path).ok();
        let bible = if doc.story_bible.character_bios.is_empty()
            && doc.story_bible.world.is_empty()
        {
            None
        } else {
            Some(render_story_bible(&doc.story_bible))
        };
        let rules = if doc.project_rules.enabled && !doc.project_rules.text.is_empty() {
            Some(doc.project_rules.text.clone())
        } else {
            None
        };
        let original_source = self.original_source.lock().map_err(|_| "原始文件归档状态不可用")?;
        project::save_project(
            &target,
            &doc,
            db_bytes.as_deref(),
            bible.as_deref(),
            rules.as_deref(),
            original_source.as_deref(),
        )?;
        drop(original_source);
        self.dirty.store(false, Ordering::SeqCst);
        Ok((target, doc.revision))
    }

    /// 替换项目关联的原始导入文件。
    pub fn set_original_source(&self, bytes: Vec<u8>) -> Result<(), String> {
        if bytes.len() as u64 > project::MAX_ORIGINAL_SOURCE_BYTES {
            return Err("原始导入文件超过 512 MiB，无法写入项目文件".into());
        }
        *self.original_source.lock().map_err(|_| "原始文件归档状态不可用")? = Some(bytes);
        Ok(())
    }

    /// 获取原始导入文件副本；未从文件导入或打开旧项目时为空。
    pub fn original_source(&self) -> Option<Vec<u8>> {
        self.original_source.lock().ok()?.clone()
    }

    /// 临时借用原始导入字节，避免每次崩溃恢复写入时复制大型附件。
    pub fn with_original_source<T>(
        &self,
        f: impl FnOnce(Option<&[u8]>) -> T,
    ) -> Result<T, String> {
        let source = self
            .original_source
            .lock()
            .map_err(|_| "原始文件归档状态不可用")?;
        Ok(f(source.as_deref()))
    }

    /// 便捷：执行命令并标记 dirty
    pub fn run(
        &self,
        f: impl FnOnce(&CommandService) -> Result<CommandResult, crate::CommandError>,
    ) -> Result<CommandResult, crate::CommandError> {
        let r = f(&self.service)?;
        self.touch();
        Ok(r)
    }
}

pub fn render_story_bible(b: &StoryBible) -> String {
    let mut s = String::new();
    s.push_str("# Story Bible\n\n");
    if !b.world.is_empty() {
        s.push_str("## 世界观\n\n");
        s.push_str(&b.world);
        s.push_str("\n\n");
    }
    if !b.era.is_empty() {
        s.push_str("## 时代背景\n\n");
        s.push_str(&b.era);
        s.push_str("\n\n");
    }
    if !b.character_bios.is_empty() {
        s.push_str("## 人物小传\n\n");
        for (k, v) in &b.character_bios {
            s.push_str(&format!("### {k}\n\n{v}\n\n"));
        }
    }
    if !b.relationships.is_empty() {
        s.push_str("## 人物关系\n\n");
        s.push_str(&b.relationships);
        s.push_str("\n\n");
    }
    if !b.principles.is_empty() {
        s.push_str("## 创作原则\n\n");
        s.push_str(&b.principles);
        s.push_str("\n\n");
    }
    if !b.references.is_empty() {
        s.push_str("## 参考资料\n\n");
        s.push_str(&b.references);
        s.push('\n');
    }
    s
}

/// 解析 Story Bible Markdown 回结构体
pub fn parse_story_bible(md: &str) -> StoryBible {
    let mut b = StoryBible::default();
    let mut section = String::new();
    let mut cur_char: Option<String> = None;
    for line in md.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            if let Some(name) = cur_char.take() {
                b.character_bios.insert(name, String::new());
            }
            cur_char = None;
            match h.trim() {
                "世界观" => section = "world".into(),
                "时代背景" => section = "era".into(),
                "人物关系" => section = "relationships".into(),
                "创作原则" => section = "principles".into(),
                "参考资料" => section = "references".into(),
                "人物小传" => section = "chars".into(),
                _ => section = "other".into(),
            }
            continue;
        }
        if let Some(h) = line.strip_prefix("### ") {
            cur_char = Some(h.trim().to_string());
            b.character_bios.insert(h.trim().to_string(), String::new());
            section = "char".into();
            continue;
        }
        let target: &mut String = match section.as_str() {
            "world" => &mut b.world,
            "era" => &mut b.era,
            "relationships" => &mut b.relationships,
            "principles" => &mut b.principles,
            "references" => &mut b.references,
            "char" => match &cur_char {
                Some(name) => b.character_bios.entry(name.clone()).or_default(),
                None => continue,
            },
            _ => continue,
        };
        if !target.is_empty() {
            target.push('\n');
        }
        target.push_str(line);
    }
    b
}

impl Workspace {
    /// 当前绑定的容器路径（未保存的新项目为 None）
    pub fn container_path(&self) -> Option<PathBuf> {
        self.container.lock().unwrap().clone()
    }

    /// 绑定容器路径（另存为后调用），使后续 save() 无需再传路径
    pub fn bind_container(&self, path: &Path) {
        *self.container.lock().unwrap() = Some(crate::bridge::canonical_container_path(path));
    }
}

#[cfg(test)]
mod source_archive_tests {
    use super::*;

    #[test] fn legacy_note_repair_is_dirty_until_saved_and_keeps_original_file(){
        let path=std::env::temp_dir().join(format!("legacy-note-{}.screenplay",new_id("qa")));
        let mut doc=ScreenplayDocument::blank("旧括注验收");doc.revision=7;
        doc.blocks=vec![ScreenplayBlock::new(BlockType::Character,"小林（轻声）"),ScreenplayBlock::new(BlockType::Dialogue,"等等。")];
        let id=doc.blocks[0].id.clone();project::save_project(&path,&doc,None,None,None,None).unwrap();
        let before=std::fs::read(&path).unwrap();let ws=Workspace::open(&path).unwrap();
        assert!(ws.dirty.load(Ordering::SeqCst));assert_eq!(ws.document().revision,8);assert_eq!(ws.document().blocks[0].id,id);
        assert_eq!(ws.document().blocks[1].block_type,BlockType::Parenthetical);assert_eq!(std::fs::read(&path).unwrap(),before);
        ws.save(None).unwrap();let again=Workspace::open(&path).unwrap();assert!(!again.dirty.load(Ordering::SeqCst));assert_eq!(again.document().blocks.len(),3);assert_eq!(again.document().revision,8);
        let _=std::fs::remove_file(path);
    }
    #[test]
    fn original_source_survives_workspace_save_and_reopen() {
        let path = std::env::temp_dir().join(format!(
            "screenplay-source-workspace-{}.screenplay",
            new_id("test")
        ));
        let original = b"original document bytes";
        let workspace = Workspace::blank("source archive roundtrip").unwrap();
        workspace.set_original_source(original.to_vec()).unwrap();
        workspace.save(Some(&path)).unwrap();

        let reopened = Workspace::open(&path).unwrap();
        assert_eq!(reopened.original_source().as_deref(), Some(original.as_slice()));
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]mod draft_persistence_tests{use super::*;
 #[test]fn draft_code_and_version_snapshots_survive_save_and_reopen(){let directory=tempfile::tempdir().unwrap();let path=directory.path().join("draft.screenplay");let workspace=Workspace::blank("剧集").unwrap();workspace.service.replace_source_text(crate::db::Origin::GuiRight,"INT. ROOM - DAY\n\n保留正文",None).unwrap();let version=workspace.service.create_version(crate::db::Origin::GuiLeft,"V2.1","交稿").unwrap();let id=version.version_id.unwrap();workspace.save(Some(&path)).unwrap();let reopened=Workspace::open(&path).unwrap();assert_eq!(reopened.document().draft_label.as_deref(),Some("V2.1"));assert_eq!(reopened.service.store.version_document(&id).unwrap().draft_label.as_deref(),Some("V2.1"));assert!(reopened.service.store.list_versions().iter().any(|v|v.name=="V2.1"));}
}

#[cfg(test)]mod symlink_save_tests{use super::*;
 #[test]fn saving_an_opened_symlink_updates_the_target_and_keeps_the_link(){let dir=tempfile::tempdir().unwrap();let actual=dir.path().join("actual.screenplay");let link=dir.path().join("link.screenplay");let first=Workspace::blank("原稿").unwrap();first.save(Some(&actual)).unwrap();std::os::unix::fs::symlink(&actual,&link).unwrap();let opened=Workspace::open(&link).unwrap();opened.service.insert_block(crate::db::Origin::GuiLeft,None,screenplay_core::ast::BlockType::Action,"新内容",None).unwrap();opened.save(None).unwrap();assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());assert!(Workspace::open(&actual).unwrap().document().blocks.iter().any(|b|b.text=="新内容"));}
}
