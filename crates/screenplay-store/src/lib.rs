//! screenplay-store — Command Service + Version Engine + SQLite + 项目容器。
//!
//! 本 crate 提供 `ScreenplayCommandService`，GUI / Agent / CLI / MCP 全部通过它修改剧本。
//!
//! # 跨进程共享（说明书第 82 条）
//!
//! GUI / CLI / MCP **必须**操作同一个权威项目状态，不能各自解压
//! `.screenplay` 得到独立副本。机制见 [`bridge`]：
//!
//! - GUI 监听 Unix socket 并发布描述文件
//! - CLI / MCP 优先连上 GUI 转发命令
//! - GUI 未运行时回退独立进程模式，并用文件锁互斥

pub mod bridge;
pub mod command;
pub mod db;
pub mod history;
pub mod project;
pub mod recovery;
pub mod workspace;

pub use bridge::{BridgeClient, BridgeDescriptor, BridgeError, BridgeRequest, BridgeResponse};
pub use command::{CommandError, CommandResult, CommandService, PendingOp, SearchHit};
pub use db::{AuditEntry, Branch, Origin, Store, VersionRecord};
pub use workspace::Workspace;

pub mod workbench_bridge;
