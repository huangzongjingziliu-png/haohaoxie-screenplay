//! SQLite 持久化：操作日志、版本历史、分支。

use rusqlite::{params, Connection};
use screenplay_core::ast::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use std::path::Path;
use std::sync::Mutex;

/// 操作来源 —— 审计日志必须记录
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub enum Origin {
    GuiLeft,
    GuiRight,
    Agent,
    Cli,
    Mcp,
    Import,
    Parser,
    System,
}

impl Origin {
    pub fn as_str(&self) -> &'static str {
        match self {
            Origin::GuiLeft => "gui-left",
            Origin::GuiRight => "gui-right",
            Origin::Agent => "agent",
            Origin::Cli => "cli",
            Origin::Mcp => "mcp",
            Origin::Import => "import",
            Origin::Parser => "parser",
            Origin::System => "system",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "gui-left" => Origin::GuiLeft,
            "gui-right" => Origin::GuiRight,
            "agent" => Origin::Agent,
            "cli" => Origin::Cli,
            "mcp" => Origin::Mcp,
            "import" => Origin::Import,
            "parser" => Origin::Parser,
            _ => Origin::System,
        }
    }
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 审计日志条目（说明书第 83 条）
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct AuditEntry {
    #[ts(type = "number")]
    pub id: i64,
    pub at: String,
    pub origin: String,
    pub command: String,
    pub target: String,
    pub old_value: String,
    pub new_value: String,
    #[ts(type = "number")]
    pub revision: u64,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct VersionRecord {
    pub id: String,
    pub name: String,
    pub note: String,
    pub branch: String,
    pub parent_version: Option<String>,
    pub created_at: String,
    #[ts(type = "number")]
    pub revision: u64,
    pub origin: String,
    /// 触发该版本的命令
    pub trigger: String,
    pub auto: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct Branch {
    pub name: String,
    pub created_at: String,
    pub head_version: Option<String>,
    pub parent_branch: Option<String>,
}

/// 打开（或创建）项目数据库
pub fn open_db(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS versions (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            note            TEXT NOT NULL DEFAULT '',
            branch          TEXT NOT NULL DEFAULT 'main',
            parent_version  TEXT,
            created_at      TEXT NOT NULL,
            revision        INTEGER NOT NULL,
            origin          TEXT NOT NULL,
            trigger_cmd     TEXT NOT NULL DEFAULT '',
            auto            INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS operations (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            at         TEXT NOT NULL,
            origin     TEXT NOT NULL,
            command    TEXT NOT NULL,
            target     TEXT NOT NULL,
            old_value  TEXT NOT NULL DEFAULT '',
            new_value  TEXT NOT NULL DEFAULT '',
            revision   INTEGER NOT NULL,
            version_id TEXT,
            FOREIGN KEY (version_id) REFERENCES versions(id)
        );

        CREATE TABLE IF NOT EXISTS branches (
            name           TEXT PRIMARY KEY,
            created_at     TEXT NOT NULL,
            head_version   TEXT,
            parent_branch  TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_ops_revision ON operations(revision);
        CREATE INDEX IF NOT EXISTS idx_versions_branch ON versions(branch);

        CREATE TABLE IF NOT EXISTS snapshots (
            id         TEXT PRIMARY KEY,
            version_id TEXT NOT NULL,
            created_at TEXT NOT NULL,
            payload    TEXT NOT NULL,
            FOREIGN KEY (version_id) REFERENCES versions(id)
        );
        "#,
    )?;
    // 默认分支
    conn.execute(
        "INSERT OR IGNORE INTO branches (name, created_at) VALUES ('main', ?1)",
        params![now_iso()],
    )?;
    Ok(conn)
}

pub struct Store {
    pub conn: Mutex<Connection>,
    /// 当前文档（唯一权威状态）
    pub doc: Mutex<ScreenplayDocument>,
    pub current_version: Mutex<Option<String>>,
    pub branch: Mutex<String>,
    /// 撤销/重做栈（GUI/Agent/CLI/MCP 共用）
    pub history: Mutex<crate::history::HistoryStack>,
}

impl Store {
    pub fn new(conn: Connection, doc: ScreenplayDocument) -> Self {
        Self {
            conn: Mutex::new(conn),
            doc: Mutex::new(doc),
            current_version: Mutex::new(None),
            history: Mutex::new(crate::history::HistoryStack::new()),
            branch: Mutex::new("main".to_string()),
        }
    }

    pub fn revision(&self) -> u64 {
        self.doc.lock().unwrap().revision
    }

    pub fn snapshot(&self) -> ScreenplayDocument {
        self.doc.lock().unwrap().clone()
    }

    // ---------------- 审计 ----------------

    pub fn log_operation(
        &self,
        origin: Origin,
        command: &str,
        target: &str,
        old_value: &str,
        new_value: &str,
        version_id: Option<&str>,
    ) {
        let conn = self.conn.lock().unwrap();
        let rev = self.doc.lock().unwrap().revision;
        let _ = conn.execute(
            "INSERT INTO operations (at, origin, command, target, old_value, new_value, revision, version_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                now_iso(),
                origin.as_str(),
                command,
                target,
                old_value,
                new_value,
                rev as i64,
                version_id
            ],
        );
    }

    pub fn audit_log(&self, limit: usize) -> Vec<AuditEntry> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, at, origin, command, target, old_value, new_value, revision, version_id
                      FROM operations ORDER BY id DESC LIMIT ?1")
            .unwrap();
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(AuditEntry {
                    id: r.get(0)?,
                    at: r.get(1)?,
                    origin: r.get(2)?,
                    command: r.get(3)?,
                    target: r.get(4)?,
                    old_value: r.get(5)?,
                    new_value: r.get(6)?,
                    revision: r.get::<_, i64>(7)? as u64,
                    version: r.get(8)?,
                })
            })
            .unwrap();
        rows.filter_map(Result::ok).collect()
    }

    // ---------------- 版本 ----------------

    /// 列出全部分支
    pub fn list_branches(&self) -> Vec<Branch> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn
            .prepare("SELECT name, created_at, head_version, parent_branch FROM branches ORDER BY created_at")
            .map_err(|e| e.to_string());
        let Ok(st) = st.as_mut() else {
            return Vec::new();
        };
        let rows = st.query_map([], |r| {
            Ok(Branch {
                name: r.get(0)?,
                created_at: r.get(1)?,
                head_version: r.get(2)?,
                parent_branch: r.get(3)?,
            })
        });
        match rows {
            Ok(it) => it.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 切换到已有分支，或创建并切换
    pub fn switch_branch(&self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("分支名不能为空".into());
        }
        let conn = self.conn.lock().unwrap();
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM branches WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if exists == 0 {
            let parent = self.branch.lock().unwrap().clone();
            conn.execute(
                "INSERT INTO branches (name, created_at, parent_branch) VALUES (?1, ?2, ?3)",
                params![name, now_iso(), parent],
            )
            .map_err(|e| e.to_string())?;
        }
        *self.branch.lock().unwrap() = name.to_string();
        Ok(())
    }

    pub fn create_version(
        &self,
        name: &str,
        note: &str,
        origin: Origin,
        trigger: &str,
        auto: bool,
    ) -> Result<VersionRecord, String> {
        let doc = self.doc.lock().unwrap();
        let id = new_id("v");
        let rec = VersionRecord {
            id: id.clone(),
            name: name.to_string(),
            note: note.to_string(),
            branch: self.branch.lock().unwrap().clone(),
            parent_version: self.current_version.lock().unwrap().clone(),
            created_at: now_iso(),
            revision: doc.revision,
            origin: origin.as_str().to_string(),
            trigger: trigger.to_string(),
            auto,
        };
        let payload = serde_json::to_string(&*doc).map_err(|e| e.to_string())?;
        drop(doc);

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO versions (id, name, note, branch, parent_version, created_at, revision, origin, trigger_cmd, auto)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                rec.id, rec.name, rec.note, rec.branch, rec.parent_version, rec.created_at,
                rec.revision as i64, rec.origin, rec.trigger, rec.auto as i64
            ],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO snapshots (id, version_id, created_at, payload) VALUES (?1,?2,?3,?4)",
            params![new_id("snap"), rec.id, rec.created_at, payload],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE branches SET head_version = ?1 WHERE name = ?2",
            params![rec.id, rec.branch],
        )
        .map_err(|e| e.to_string())?;
        drop(conn);

        *self.current_version.lock().unwrap() = Some(id.clone());
        Ok(rec)
    }

    pub fn list_versions(&self) -> Vec<VersionRecord> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, note, branch, parent_version, created_at, revision, origin, trigger_cmd, auto
                      FROM versions ORDER BY revision DESC, created_at DESC")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok(VersionRecord {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    note: r.get(2)?,
                    branch: r.get(3)?,
                    parent_version: r.get(4)?,
                    created_at: r.get(5)?,
                    revision: r.get::<_, i64>(6)? as u64,
                    origin: r.get(7)?,
                    trigger: r.get(8)?,
                    auto: r.get::<_, i64>(9)? != 0,
                })
            })
            .unwrap();
        rows.filter_map(Result::ok).collect()
    }

    pub fn version(&self, id: &str) -> Option<VersionRecord> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, note, branch, parent_version, created_at, revision, origin, trigger_cmd, auto
             FROM versions WHERE id = ?1",
            params![id],
            |r| {
                Ok(VersionRecord {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    note: r.get(2)?,
                    branch: r.get(3)?,
                    parent_version: r.get(4)?,
                    created_at: r.get(5)?,
                    revision: r.get::<_, i64>(6)? as u64,
                    origin: r.get(7)?,
                    trigger: r.get(8)?,
                    auto: r.get::<_, i64>(9)? != 0,
                })
            },
        )
        .ok()
    }

    pub fn version_document(&self, id: &str) -> Option<ScreenplayDocument> {
        let conn = self.conn.lock().unwrap();
        let payload: Option<String> = conn
            .query_row(
                "SELECT payload FROM snapshots WHERE version_id = ?1 ORDER BY created_at DESC LIMIT 1",
                params![id],
                |r| r.get(0),
            )
            .ok();
        drop(conn);
        payload.and_then(|p| serde_json::from_str(&p).ok())
    }

    // ---------------- 分支 ----------------

    pub fn create_branch(&self, name: &str, from_version: Option<&str>) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO branches (name, created_at, head_version, parent_branch) VALUES (?1, ?2, ?3, ?4)",
            params![
                name,
                now_iso(),
                from_version,
                self.branch.lock().unwrap().clone()
            ],
        )
        .map_err(|e| e.to_string())?;
        drop(conn);
        *self.branch.lock().unwrap() = name.to_string();
        Ok(())
    }

}
