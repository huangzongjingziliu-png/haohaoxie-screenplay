import {effectiveLanguage} from '@/i18n';
/**
 * IPC 客户端 —— 前端访问 Rust 核心的唯一入口。
 *
 * 铁律：
 * 1. 本文件之外，**任何前端代码都不得直接 invoke Tauri 命令**。
 * 2. 所有修改类命令最终都走 Rust `CommandService`，前端不持有可写文档副本。
 * 3. 错误必须是结构化的（`IpcError`），不得吞掉或转成 console.error。
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AutosaveAction,
  AutosaveConfig,
  PendingRecovery,
  BlockType,
  CommandResult,
  DocumentStats,
  Pagination,
  ScreenplayDocument,
  PaperSettings,
  VersionRecord,
  AuditEntry,
  HistoryStatus,
  Branch,
} from './types';


/** 结构化错误（与 Rust IpcError 一一对应） */
export type IpcError =
  | { error: 'REVISION_CONFLICT'; expected: number; actual: number; message: string }
  | { error: 'NOT_FOUND'; what: string; id: string }
  | { error: 'INVALID_ARGUMENT'; message: string }
  | { error: 'MODEL_REQUIRED'; message: string }
  | { error: 'LOCKED'; id: string }
  | { error: 'INTERNAL'; message: string }
  | { error: 'APP_STATE'; message: string }
  | { error: 'UNKNOWN'; message: string };

export class ScreenplayError extends Error {
  readonly code: IpcError['error'];
  readonly detail: Record<string, unknown>;

  constructor(e: IpcError) {
    super(errorMessage(e));
    this.name = 'ScreenplayError';
    this.code = e.error;
    this.detail = e as unknown as Record<string, unknown>;
  }
}

function errorMessage(e: IpcError): string {
  switch (e.error) {
    case 'REVISION_CONFLICT':
      return `文档已被其他来源修改（你基于 rev ${e.expected}，当前是 rev ${e.actual}）。${e.message}`;
    case 'NOT_FOUND':
      return `找不到${e.what}：${e.id}`;
    case 'INVALID_ARGUMENT':
      return e.message;
    case 'LOCKED':
      return `内容已锁定，无法修改（${e.id}）`;
    case 'INTERNAL':
      return e.message;
    case 'APP_STATE':
      return e.message;
    default:
      return (e as { message?: string }).message ?? '未知错误';
  }
}

/** 把 Tauri 的各种异常形态规整为 IpcError */
function normalize(err: unknown): IpcError {
  if (err && typeof err === 'object' && 'error' in err) {
    return err as IpcError;
  }
  if (typeof err === 'string') {
    try {
      const parsed = JSON.parse(err);
      if (parsed && typeof parsed === 'object' && 'error' in parsed) {
        return parsed as IpcError;
      }
      return { error: 'APP_STATE', message: err };
    } catch {
      return { error: 'APP_STATE', message: err };
    }
  }
  return { error: 'UNKNOWN', message: String(err) };
}

/** 统一调用包装：所有 invoke 都经过这里 */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    const error=new ScreenplayError(normalize(err));
    if(error.code==='MODEL_REQUIRED'&&(cmd!=='agent_refresh_memory'||args?.force))window.dispatchEvent(new CustomEvent('model-required',{detail:error.message}));
    throw error;
  }
}

// ---------------------------------------------------------------------------
// DTO（Rust 侧定义）
// ---------------------------------------------------------------------------

export interface ProjectInfo {
  state: 'empty' | 'unsaved' | 'open';
  path: string | null;
  title: string;
  revision: number;
  dirty: boolean;
  blockCount: number;
}

export interface RecentEntry {
  path: string;
  title: string;
  openedAt: string;
}

export interface Bootstrap {
  project: ProjectInfo;
  document: ScreenplayDocument | null;
  sourceText: string;
  recent: RecentEntry[];
}

export interface SaveOutcome {
  path: string;
  savedRevision: number;
  savedAt: string;
}

export interface ChangeNotice {
  revision: number;
  origin: string;
  affected: string[];
  dirty: boolean;
  at: string;
}

export interface SearchHit {
  blockId: string;
  blockIndex: number;
  blockType: string;
  text: string;
  start: number;
  end: number;
  sceneId: string | null;
}

// ---------------------------------------------------------------------------
// 项目生命周期
// ---------------------------------------------------------------------------

export const ipc = {
  openHelpLink: (url:string) => call<void>('open_help_link', {url}),
  // 生命周期
  bootstrap: () => call<Bootstrap>('bootstrap'),
  newProject: (title: string) => call<ProjectInfo>('new_project', { title }),
  takeFileOpenRequests: () => call<string[]>('take_file_open_requests'),
  openProject: (path: string) => call<Bootstrap>('open_project', { path }),
  closeProject: (discardChanges = false) => call<ProjectInfo>('close_project', { discardChanges }),
  saveProject: () => call<SaveOutcome>('save_project'),
  saveProjectAs: (path: string) => call<SaveOutcome>('save_project_as', { path }),
  revertProject: () => call<Bootstrap>('revert_project'),
  projectInfo: () => call<ProjectInfo>('project_info_cmd'),

  // 读取
  getDocument: () => call<ScreenplayDocument>('get_document'),
  getSourceText: () => call<string>('get_source_text'),
  search: (query: string, caseSensitive = false) =>
    call<SearchHit[]>('search', { query, caseSensitive }),

  reorderScenes: (sceneIds: string[], expectedRevision: number) => call<CommandResult>('reorder_scenes', {sceneIds, expectedRevision}),
  createBoardScene: (title: string, summary: string, expectedRevision: number) => call<CommandResult>('create_board_scene', {title, summary, expectedRevision}),
  updateSceneBoard: (board: import('./types/SceneBoard').SceneBoard, expectedRevision: number,sceneIds?:string[]) => call<CommandResult>('update_scene_board', {board, expectedRevision,sceneIds}),
  summarizeScene: (sceneId: string) => call<{summary: string; source: string; projectId: string}>('agent_summarize_scene', {sceneId}),
  // 修改（全部经 CommandService）
  replaceBlock: (blockId: string, text: string, expectedRevision?: number) =>
    call<CommandResult>('replace_block', { blockId, text, expectedRevision }),
  insertBlock: (
    afterId: string | null,
    blockType: BlockType,
    text: string,
    expectedRevision?: number
  ) =>
    call<CommandResult>('insert_block', {
      afterId,
      blockType,
      text,
      expectedRevision,
    }),
  deleteBlock: (blockId: string, expectedRevision?: number) =>
    call<CommandResult>('delete_block', { blockId, expectedRevision }),
  setBlockType: (
    blockId: string,
    blockType: BlockType,
    expectedRevision?: number
  ) => call<CommandResult>('set_block_type', { blockId, blockType, expectedRevision }),
  mergeBlocks: (firstId: string, secondId: string, expectedRevision?: number) =>
    call<CommandResult>('merge_blocks', {
      firstId,
      secondId,
      expectedRevision,
    }),
  replaceSourceText: (text: string, expectedRevision?: number) =>
    call<CommandResult>('replace_source_text', { text, expectedRevision }),

  // 撤销重做
  undo: () => call<CommandResult>('undo'),
  redo: () => call<CommandResult>('redo'),
  historyStatus: () => call<HistoryStatus>('history_status'),

  // 版本
  listVersions: () => call<VersionRecord[]>('list_versions'),
  createVersion: (name: string, note: string) =>
    call<CommandResult>('create_version', { name, note }),
  restoreVersion: (versionId: string) =>
    call<CommandResult>('restore_version', { versionId }),
  auditLog: (limit?: number) => call<AuditEntry[]>('audit_log', { limit }),
  compareVersions: (versionA: string, versionB?: string) =>
    call<{ rendered?: string; changed_blocks?: number; added_blocks?: number; removed_blocks?: number }>(
      'compare_versions',
      { versionA, versionB }
    ),
  listBranches: () => call<Branch[]>('list_branches'),
  currentBranch: () => call<string>('current_branch'),
  switchBranch: (name: string) => call<string>('switch_branch', { name }),

  // 分页与序列化（与导出共用 Rust 实现）
  getPagination: () => call<Pagination>('get_pagination'),
  documentStats: () => call<DocumentStats>('document_stats'),
  toFountain: () => call<string>('to_fountain'),
  updateSettings: (patch: {
    sceneHeadingTemplate?: string;
    sceneHeadingLanguage?: string;
    sceneHeadingAuto?: boolean;
    elementStyles?: Record<string, import("./types/ElementStyle").ElementStyle>;
    projectMemory?: import("./types/ProjectMemory").ProjectMemory;
    stylePresetName?: string;
    ocrLanguages?: string[];
    typography?: string;
    showSceneNumbers?: boolean;
    title?: string;
    author?: string;
    language?: string;
    paper?: PaperSettings;
  }) => call<CommandResult>('update_settings', { patch }),

  // 导入导出
  agentTransformText: (text:string,instruction:string,importMode=false,requestId?:string) => call<string>('agent_transform_text',{text,instruction,import:importMode,requestId}),
  agentApplyText: (text:string,expectedRevision:number,blockId?:string) => call<CommandResult>('agent_apply_text',{text,expectedRevision,blockId}),
  agentApplySelection: (blockIds:string[],text:string,expectedRevision:number) => call<CommandResult>('agent_apply_selection',{blockIds,text,expectedRevision}),
  agentAttachmentText: (path:string) => call<string>('agent_attachment_text',{path}),
  importFile: (path: string, requestId?: string) => call<CommandResult>('import_file', { path, requestId }),
  cancelImport: (requestId: string) => call<boolean>('cancel_import', { requestId }),
  exportFile: (path: string, format?: string) =>
    call<string>('export_file', { path, format }),

  agentConnectionInfo: () => call<{ cli: string; mcp: string; available: boolean; installDir: string }>('agent_connection_info'),
  checkAgentConnection: () => call<string>('check_agent_connection'),
  installAgentCli: () => call<string>('install_agent_cli'),
  // 自动保存
  autosaveConfig: () => call<AutosaveConfig>('autosave_config'),
  configureAutosave: (options: {
    enabled: boolean;
    saveIntervalSecs: number;
    versionIntervalSecs: number;
  }) => call<AutosaveConfig>('configure_autosave', { options }),
  autosaveTick: () => call<AutosaveAction | null>('autosave_tick'),
  hasUnsavedChanges: () => call<boolean>('has_unsaved_changes'),
  confirmClose: (discardChanges = false) => call<void>('confirm_close', { discardChanges }),

  // 崩溃恢复
  pendingRecovery: () => call<PendingRecovery | null>('pending_recovery'),
  adoptRecovery: () => call<Bootstrap>('adopt_recovery'),
  discardRecovery: () => call<void>('discard_recovery'),

  // 最近文件
  listRecent: () => call<RecentEntry[]>('list_recent'),
  removeRecent: (path: string) => call<void>('remove_recent', { path }),

  // Agent（智能编剧助理）
  agentChat: (projectId:string,instruction:string,history:{role:string;content:string}[],skillId?:string,blockId?:string) => call<string>('agent_chat',{projectId,instruction,history,skillId,blockId,uiLanguage:effectiveLanguage()}),
  agentShareProject: (path:string,includeMemory:boolean)=>call<void>('agent_share_project',{path,includeMemory}),
  agentRefreshMemory: (projectId:string,discussion?:string,force=false)=>call<void>('agent_refresh_memory',{projectId,discussion,force}),
  agentMemory: (scope:string,projectId:string) => call<string>('agent_memory',{scope,projectId}),
  agentSaveMemory: (scope:string,projectId:string,text:string) => call<void>('agent_save_memory',{scope,projectId,text}),
  agentListSkills: () =>
    call<{ id: string; name: string; description: string; builtin: boolean }[]>('agent_list_skills'),
  agentApplySkill: (skillId: string, blockId: string, userInput: string) =>
    call<CommandResult>('agent_apply_skill', { skillId, blockId, userInput }),
  agentFreeformEdit: (instruction: string) =>
    call<CommandResult>('agent_freeform_edit', { instruction }),
  agentListModels: () => call<ModelProfiles>('agent_list_models'),
  agentSaveModel: (profile: ModelProfile & { apiKey?: string }) => call<ModelProfiles>('agent_save_model', { profile }),
  agentSelectModel: (id: string) => call<ModelProfiles>('agent_select_model', { id }),
  agentDeleteModel: (id: string) => call<ModelProfiles>('agent_delete_model', { id }),
  agentTestModel: (profile: ModelProfile & {apiKey?:string}) => call<{success:boolean;model:string;latencyMs:number}>('agent_test_model',{profile}),
  agentGetConfig: () =>
    call<{ cloudAiEnabled: boolean; providerKind: string; model: string; apiKeySet: boolean; baseUrl: string }>(
      'agent_get_config'
    ),
  agentSetConfig: (opts: {
    cloudAiEnabled?: boolean;
    providerKind?: string;
    apiKey?: string;
    rememberKey?: boolean;
    model?: string;
    baseUrl?: string;
  }) =>
    call<{ cloudAiEnabled: boolean; providerKind: string; model: string; apiKeySet: boolean; baseUrl: string }>(
      'agent_set_config',
      opts
    ),
  agentGenerateSkill: (instruction:string)=>call<string>("agent_generate_skill",{instruction}),
  agentCreateSkill: (markdown:string)=>call<string>("agent_create_skill",{markdown}),
  agentInstallSkill: (sourcePath: string) => call<string>('agent_install_skill', { sourcePath }),
  agentUnloadSkill: (skillId: string) => call<boolean>('agent_unload_skill', { skillId }),
};

// ---------------------------------------------------------------------------
// 变更事件订阅
// ---------------------------------------------------------------------------

export function onDocumentChanged(cb: (n: ChangeNotice) => void): Promise<UnlistenFn> {
  return listen<ChangeNotice>('document-changed', (e) => cb(e.payload));
}

export function onProjectChanged(cb: (p: ProjectInfo) => void): Promise<UnlistenFn> {
  return listen<ProjectInfo>('project-changed', (e) => cb(e.payload));
}

export function onProjectSaved(cb: (s: SaveOutcome) => void): Promise<UnlistenFn> {
  return listen<SaveOutcome>('project-saved', (e) => cb(e.payload));
}

export type { AutosaveAction, AutosaveConfig, PendingRecovery };

export function onCloseRequested(cb: () => void): Promise<UnlistenFn> {
  return listen('close-requested-with-unsaved', cb);
}

export interface ImportProgress {
  requestId: string;
  stage: 'credential' | 'agent' | 'reading' | 'parsing' | 'pdf_pages' | 'ocr' | 'ready';
  completed: number;
  total: number;
}
export function onImportProgress(cb: (progress: ImportProgress) => void): Promise<UnlistenFn> {
  return listen<ImportProgress>('import-progress', (e) => cb(e.payload));
}

export type ModelProfile = { id: string; name: string; providerKind: string; model: string; baseUrl: string };
export type ModelProfiles = { activeId: string; profiles: ModelProfile[] };
