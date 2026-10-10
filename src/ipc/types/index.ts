/**
 * IPC 类型统一出口。
 *
 * ⚠️ 本目录下所有文件均由 `ts-rs` 从 Rust 源码自动生成，**禁止手工编辑**。
 *
 * 重新生成：
 *   cargo test -p screenplay-core -p screenplay-store
 *
 * 生成规则来自 `crates/screenplay-core/src/ast.rs`、`pagination.rs`、
 * `crates/screenplay-store/src/command.rs`、`db.rs` 中的
 * `#[derive(TS)]` + `#[ts(export, export_to = "../../../src/ipc/types/")]`。
 *
 * 这样做的好处：Rust AST 是唯一权威，前端类型不可能与后端漂移。
 * 修改 Rust 结构体字段后重新生成即可，前端 TypeScript 编译器会立即报错。
 */

export type { BlockMetadata } from './BlockMetadata';
export type { BlockType } from './BlockType';
export type { CharacterEntity } from './CharacterEntity';
export type { DocumentStats } from './DocumentStats';
export type { ImportInfo } from './ImportInfo';
export type { IntExt } from './IntExt';
export type { LocationEntity } from './LocationEntity';
export type { Metadata } from './Metadata';
export type { PaperSettings } from './PaperSettings';
export type { ProjectRules } from './ProjectRules';
export type { Scene } from './Scene';
export type { SceneHeadingData } from './SceneHeadingData';
export type { ScreenplayBlock } from './ScreenplayBlock';
export type { ScreenplayDocument } from './ScreenplayDocument';
export type { ScreenplaySettings } from './ScreenplaySettings';
export type { SourceRange } from './SourceRange';
export type { SourceType } from './SourceType';
export type { StoryBible } from './StoryBible';
export type { TitlePage } from './TitlePage';

export type { Page } from './Page';
export type { PageBlock } from './PageBlock';
export type { Pagination } from './Pagination';

export type { AuditEntry } from './AuditEntry';
export type { Branch } from './Branch';
export type { CommandError } from './CommandError';
export type { CommandResult } from './CommandResult';
export type { HistoryStatus } from './HistoryStatus';
export type { PendingRecovery } from './PendingRecovery';
export type { AutosaveAction } from './AutosaveAction';
export type { AutosaveConfig } from './AutosaveConfig';
export type { Origin } from './Origin';
export type { VersionRecord } from './VersionRecord';
// ---------------------------------------------------------------------------
// GUI 自检报告（开发期）
// ---------------------------------------------------------------------------

export interface SelftestStep {
  name: string;
  ok: boolean;
  ms: number;
  detail: string;
}

export interface SelftestReport {
  startedAt: string;
  projectPath: string;
  totalSteps: number;
  passed: number;
  failed: number;
  steps: SelftestStep[];
}
