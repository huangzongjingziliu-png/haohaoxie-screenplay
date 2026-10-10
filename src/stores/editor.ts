import {tr, fmt} from '@/i18n';
/**
 * 编辑器 Store
 *
 * ⚠️ 架构铁律（说明书第 82 条）：
 * - 本 store **不持有**可写文档副本作为权威状态。
 * - `document` 只是后端状态的**只读缓存**，用于渲染。
 * - 任何修改必须调用 `ipc.*` → Rust `CommandService`，
 *   然后用后端返回的 `CommandResult` 刷新缓存。
 * - 前端**禁止**使用 `JSON.stringify(document)` 之类方式保存文档，
 *   也**禁止**在前端解析剧本文本。
 */

import { create } from 'zustand';
import { flushPendingEdits } from '@/utils/pendingEdits';
import { ipc, ScreenplayError } from '@/ipc/client';
import type {
  BlockType,
  CommandResult,
  ScreenplayBlock,
  ScreenplayDocument,
} from '@/ipc/types';
import type { ProjectInfo, SaveOutcome } from '@/ipc/client';
import type { HistoryStatus } from '@/ipc/types';

export type ViewMode = 'split' | 'left' | 'right' | 'focus';

export interface Toast {
  id: number;
  kind: 'error' | 'info' | 'success';
  message: string;
  code?: string;
}

interface EditorState {
  // ---- 后端权威状态（只读缓存）----
  document: ScreenplayDocument | null;
  project: ProjectInfo;
  /** 后端返回的 source_text（右侧自由文本），不是前端拼的 */
  sourceText: string;

  // ---- UI 状态（前端独有，允许自由修改）----
  viewMode: ViewMode;
  previousViewMode: ViewMode;
  sourceFirst: boolean;
  leftSplitRatio: number;
  activeBlockId: string | null;
  zoom: number;
  searchQuery: string;
  showNavigator: boolean;
  navigatorTab: 'scenes' | 'characters' | 'locations' | 'outline';
  /** IME 组字中：期间不重排、不覆盖 DOM */
  composing: boolean;
  toasts: Toast[];

  // ---- 动作 ----
  bootstrap: () => Promise<void>;
  newProject: (title: string) => Promise<void>;
  openProject: (path: string) => Promise<void>;
  closeProject: () => Promise<void>;
  save: (path?: string) => Promise<SaveOutcome | null>;
  saveAs: () => Promise<SaveOutcome | null>;
  refreshDocument: () => Promise<void>;
  revert: () => Promise<void>;

  /** 左侧 block 文本修改 → CommandService.replace_block */
  changeBlockText: (blockId: string, text: string, expectedRevision?: number) => Promise<boolean>;
  /** 左侧 block 类型修改 */
  changeBlockType: (blockId: string, type: BlockType) => Promise<void>;
  /** 右侧文本整体修改 → CommandService.replace_source_text */
  changeSourceText: (text: string, expectedRevision?: number) => Promise<boolean>;
  insertBlock: (afterId: string | null, type: BlockType, text?: string) => Promise<void>;
  deleteBlock: (blockId: string) => Promise<void>;
  /** 块首退格：把 second 合并进 first */
  mergeBlock: (firstId: string, secondId: string) => Promise<void>;

  /** 撤销（Rust 侧栈，覆盖 GUI/Agent/CLI/MCP 全部修改） */
  undo: () => Promise<void>;
  /** 重做 */
  redo: () => Promise<void>;
  history: HistoryStatus | null;
  refreshHistory: () => Promise<void>;

  setViewMode: (m: ViewMode) => void;
  exitFocus: () => void;
  swapEditors: () => void;
  setLeftSplitRatio: (r: number) => void;
  setActiveBlock: (id: string | null) => void;
  setZoom: (z: number) => void;
  setSearchQuery: (q: string) => void;
  setShowNavigator: (s: boolean) => void;
  setNavigatorTab: (t: EditorState['navigatorTab']) => void;
  setComposing: (c: boolean) => void;

  pushToast: (kind: Toast['kind'], message: string, code?: string) => void;
  dismissToast: (id: number) => void;

  /** 供 CommandResult 统一刷新缓存（会等待后端 projectInfo 同步完成） */
  applyResult: (r: CommandResult) => Promise<void>;
  refreshProjectInfo: () => Promise<void>;
}

const EMPTY_PROJECT: ProjectInfo = {
  state: 'empty',
  path: null,
  title: '',
  revision: 0,
  dirty: false,
  blockCount: 0,
};

let toastSeq = 1;

export const useEditorStore = create<EditorState>((set, get) => ({
  document: null,
  project: EMPTY_PROJECT,
  sourceText: '',

  viewMode: 'left',
  previousViewMode: 'left',
  sourceFirst: true,
  leftSplitRatio: 0.55,
  activeBlockId: null,
  zoom: 0,
  searchQuery: '',
  showNavigator: true,
  navigatorTab: 'scenes',
  composing: false,
  toasts: [],
  history: null,

  // -----------------------------------------------------------------
  // 撤销重做
  // -----------------------------------------------------------------

  undo: async () => {
    try {
      const r = await ipc.undo();
      await get().applyResult(r);
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
    }
  },

  redo: async () => {
    try {
      const r = await ipc.redo();
      await get().applyResult(r);
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
    }
  },

  refreshHistory: async () => {
    try {
      set({ history: await ipc.historyStatus() });
    } catch {
      /* 未打开项目时忽略 */
    }
  },

  // -----------------------------------------------------------------
  // 生命周期
  // -----------------------------------------------------------------

  bootstrap: async () => {
    try {
      const b = await ipc.bootstrap();
      set({
        document: b.document,
        project: b.project,
        sourceText: b.sourceText,
        activeBlockId: null,
      });
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
    }
  },

  newProject: async (title) => {
    try {
      const info = await ipc.newProject(title);
      const doc = await ipc.getDocument();
      set({
        document: doc,
        project: info,
        sourceText: await ipc.getSourceText(),
        activeBlockId: null,
      });
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
    }
  },

  openProject: async (path) => {
    try {
      const b = await ipc.openProject(path);
      set({
        document: b.document,
        project: b.project,
        sourceText: b.sourceText,
        activeBlockId: null,
      });
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
    }
  },

  closeProject: async () => {
    try {
      await flushPendingEdits();
      const info = await ipc.closeProject();
      set({ document: null, project: info, sourceText: '', activeBlockId: null });
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
    }
  },

  save: async (path) => {
    try {
      await flushPendingEdits();
      if (!path && get().project.state === 'unsaved' && !get().project.path) return await get().saveAs();
      const outcome = path ? await ipc.saveProjectAs(path) : await ipc.saveProject();
      // 关键：保存后**主动拉取**后端权威状态，
      // 而不是信任「保存成功 = 我这份数据是对的」。
      const info = await ipc.projectInfo();
      set({ project: info });
      get().pushToast('success', fmt('已保存到 {0}', outcome.path));
      return outcome;
    } catch (e) {
      // 保存失败**绝不能**标记为已保存
      get().pushToast('error', describe(e), codeOf(e));
      return null;
    }
  },

  saveAs: async () => {
    try {
      const { save: dialog } = await import('@tauri-apps/plugin-dialog');
      const name = (get().project.title || tr('剧本')).replace(/[/\\]/g, '-');
      const path = await dialog({ defaultPath: `${name}.screenplay`, filters: [{ name: tr('好好写剧本项目'), extensions: ['screenplay'] }] });
      return path ? await get().save(path) : null;
    } catch (e) { get().pushToast('error', describe(e), codeOf(e)); return null; }
  },

  refreshDocument: async () => {
    try {
      const b = await ipc.bootstrap();
      const current = get().document;
      // 忽略晚到的同项目旧快照，且不重置用户的选择。
      if (current && b.document?.id === current.id && b.document.revision < current.revision) return;
      set({ document: b.document, project: b.project, sourceText: b.sourceText });
      await get().refreshHistory();
    } catch (e) { get().pushToast('error', describe(e), codeOf(e)); }
  },

  revert: async () => {
    try {
      const b = await ipc.revertProject();
      set({ document: b.document, project: b.project, sourceText: b.sourceText });
      get().pushToast('info', tr('已放弃未保存的修改'));
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
    }
  },

  // -----------------------------------------------------------------
  // 修改（全部经 CommandService）
  // -----------------------------------------------------------------

  changeBlockText: async (blockId, text, expectedRevision) => {
    // IME 组字期间不提交，避免打断输入法
    if (get().composing) return false;
    const cur = expectedRevision ?? get().document?.revision;
    try {
      const r = await ipc.replaceBlock(blockId, text, cur);
      await get().applyResult(r);
      return true;
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
      // 冲突时重新拉取权威状态，避免前端停留在错误版本
      if (e instanceof ScreenplayError && e.code === 'REVISION_CONFLICT') {
        await get().bootstrap();
      }
      return false;
    }
  },

  changeBlockType: async (blockId, type) => {
    const cur = get().document?.revision;
    try {
      // 必须走后端 set_block_type：它保留稳定 ID、源文本范围与撤销单元。
      // 早期实现是「删除 + 插入」，会换掉 block.id 并丢失 undo 语义。
      const r = await ipc.setBlockType(blockId, type, cur);
      await get().applyResult(r);
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
      if (e instanceof ScreenplayError && e.code === 'REVISION_CONFLICT') {
        await get().bootstrap();
      }
    }
  },

  /** 块首退格：把 second 合并进 first（专业编辑器语义） */
  mergeBlock: async (firstId, secondId) => {
    const cur = get().document?.revision;
    try {
      const r = await ipc.mergeBlocks(firstId, secondId, cur);
      await get().applyResult(r);
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
      if (e instanceof ScreenplayError && e.code === 'REVISION_CONFLICT') {
        await get().bootstrap();
      }
    }
  },

  changeSourceText: async (text, expectedRevision) => {
    if (get().composing) return false;
    const cur = expectedRevision ?? get().document?.revision;
    try {
      const r = await ipc.replaceSourceText(text, cur);
      await get().applyResult(r);
      return true;
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
      if (e instanceof ScreenplayError && e.code === 'REVISION_CONFLICT') {
        await get().bootstrap();
      }
      return false;
    }
  },

  insertBlock: async (afterId, type, text = '') => {
    const cur = get().document?.revision;
    try {
      const r = await ipc.insertBlock(afterId, type, text, cur);
      await get().applyResult(r);
      if (r.affected.length > 0) set({ activeBlockId: r.affected[0] });
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
    }
  },

  deleteBlock: async (blockId) => {
    const cur = get().document?.revision;
    try {
      const r = await ipc.deleteBlock(blockId, cur);
      await get().applyResult(r);
      if (get().activeBlockId === blockId) set({ activeBlockId: null });
    } catch (e) {
      get().pushToast('error', describe(e), codeOf(e));
    }
  },

  // -----------------------------------------------------------------
  // 内部工具
  // -----------------------------------------------------------------

  applyResult: async (r) => {
    set({
      document: r.document,
      sourceText: r.source_text,
    });
    // dirty 必须以**后端**判定为准。
    // 这里必须 await：调用方 await insertBlock() 返回时，
    // 期望 document / sourceText / project 三者都已一致。
    await get().refreshProjectInfo();
    // 撤销栈状态也来自后端（GUI/Agent/CLI/MCP 共用同一个栈）
    await get().refreshHistory();
  },

  refreshProjectInfo: async () => {
    try {
      const info = await ipc.projectInfo();
      set({ project: info });
    } catch {
      /* 轮询失败不打扰用户 */
    }
  },

  setViewMode: (viewMode) => set((s) => ({ viewMode, previousViewMode: viewMode === "focus" && s.viewMode !== "focus" ? s.viewMode : s.previousViewMode })),
  exitFocus: () => set((s) => ({ viewMode: s.previousViewMode === "focus" ? "left" : s.previousViewMode })),
  swapEditors: () => set((s) => ({ sourceFirst: !s.sourceFirst })),
  setLeftSplitRatio: (leftSplitRatio) =>
    set({ leftSplitRatio: Math.max(0.2, Math.min(0.8, leftSplitRatio)) }),
  setActiveBlock: (activeBlockId) => set({ activeBlockId }),
  setZoom: (zoom) => set({ zoom }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setShowNavigator: (showNavigator) => set({ showNavigator }),
  setNavigatorTab: (navigatorTab) => set({ navigatorTab }),
  setComposing: (composing) => set({ composing }),

  pushToast: (kind, message, code) =>
    set((s) => ({
      toasts: [...s.toasts, { id: toastSeq++, kind, message, code }].slice(-5),
    })),
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

// ---------------------------------------------------------------------------

function describe(e: unknown): string {
  if (e instanceof ScreenplayError) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}

function codeOf(e: unknown): string | undefined {
  if (e instanceof ScreenplayError) return e.code;
  return undefined;
}

/** 便捷选择器 */
export const selectBlocks = (s: EditorState): ScreenplayBlock[] => s.document?.blocks ?? [];