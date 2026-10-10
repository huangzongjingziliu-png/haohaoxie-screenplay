//! screenplay-core — 剧本核心：AST、Parser、Serializer、Pagination、Diff、Validate。
//!
//! 本 crate 是整个软件唯一权威实现。GUI / Agent / CLI / MCP 全部通过
//! `ScreenplayCommandService`（位于 screenplay-store）修改文档。

pub mod ast;
pub mod denoise;
pub mod dict;
pub mod diff;
pub mod pagination;
pub mod parser;
pub mod serializer;
pub mod typography;

pub use ast::*;
pub use parser::{parse, ParseOptions, ParseOutcome};

mod markdown;
