import {tr} from '@/i18n';
import { stripAutoContinuation } from '@/utils/continuation';
import { fragmentContext, sourceBlockId, getFragmentDraft, rememberFragmentDraft } from '@/utils/pageFragments';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useEditorStore } from '@/stores/editor';
import { ipc } from '@/ipc/client';
import { ELEMENT_TYPES, readShortcuts, keyCombo, shortcutLabel } from '@/utils/elementShortcuts';
import { sceneFollowSuppressed } from '@/utils/sceneFollow';
import { defaultElementStyle } from '@/utils/appearance';
import { paperInkColor } from '@/utils/theme';
import { flushPendingEdits, registerPendingEditor } from '@/utils/pendingEdits';
import { orderedBlockRange, sceneBlockIds, blocksToText, rememberBlockClipboard, readBlockClipboard, floatingMenuTop, blockRangeAnnouncement } from './blockSelection';
import { writeClipboardText } from '@/utils/clipboard';
import type { BlockType, Page, PageBlock, Pagination } from '@/ipc/types';

// Share document indexes across rendered paragraphs; build once per block array.
const documentIndexes = new WeakMap<object, { blocks: Map<string, import('@/ipc/types').ScreenplayBlock>; scenes: Map<string, number> }>();
function indexDocument(blocks: import('@/ipc/types').ScreenplayBlock[]) {
  let index = documentIndexes.get(blocks);
  if (!index) {
    index = { blocks: new Map(), scenes: new Map() };
    let scene = 0;
    for (const block of blocks) { index.blocks.set(block.id, block); if (block.block_type === 'scene_heading') index.scenes.set(block.id, ++scene); }
    documentIndexes.set(blocks, index);
  }
  return index;
}

/**
 * 纸面编辑器（左侧）。
 *
 * 设计要点：
 * 1. **分页来自 Rust**：`ipc.getPagination()`，与 PDF 导出同一份计算结果。
 *    前端不再有自己的分页实现（那会导致纸面与导出不一致）。
 * 2. **中文 IME 安全**：
 *    - 组字期间不提交、不重排、不覆盖 DOM；
 *    - `compositionend` 后**不能**同步读 DOM —— 此时浏览器可能还没写入确认字，
 *      同步读会把拼音串提交到后端，导致中文永久丢失。必须等一帧再提交。
 * 3. **提交走 CommandService**：输入防抖后调用 `changeBlockText`。
 * 4. 元素类型切换、插入、删除都通过后端命令。
 * 5. **不得用过期前端内容覆盖后端**：若外部（CLI/MCP/Agent）在本块编辑期间改了
 *    同一块，且用户自己没有再输入，则退焦时**采纳**后端内容而不是回写旧 DOM。
 */

// Physical inches match the pagination profile, never em-based pseudo indents.
const INDENT_INCHES: Record<string, number> = { character: 2.2, parenthetical: 1.6, dialogue: 1, dual_dialogue: 2.2, transition: 4 };

const BLOCK_LABELS: Record<string, string> = {
  scene_heading: '场景标题',
  action: '动作',
  character: '人物',
  parenthetical: '括注',
  dialogue: '对白',
  dual_dialogue: '双人对白',
  transition: '转场',
  shot: '镜头',
  note: '注释',
  section: '章节',
  synopsis: '概要',
  general: '普通文本',
  page_break: '分页符',
};

const TYPE_ORDER = ELEMENT_TYPES;

/** 元素类型菜单：跟随光标 / 吸附顶部横排 / 吸附左侧竖排。默认跟随光标，已保存偏好者沿用。 */
const TYPE_MENU_KEY = 'hhx.elementMenuMode';
const TYPE_MENU_COLLAPSED_KEY = 'hhx.elementMenuCollapsed';
type TypeMenuMode = 'float' | 'top' | 'left';
const readTypeMenuMode = (): TypeMenuMode => {
  try {
    const v = localStorage.getItem(TYPE_MENU_KEY);
    return v === 'float' || v === 'top' || v === 'left' ? v : 'float';
  } catch {
    return 'float';
  }
};
/** 收起/展开状态；默认展开，选择过就记住。 */
const readTypeMenuCollapsed = (): boolean => {
  try {
    return localStorage.getItem(TYPE_MENU_COLLAPSED_KEY) === '1';
  } catch {
    return false;
  }
};

/** 剧本语义：回车后新块的元素类型 */
function nextTypeFor(t: BlockType): BlockType {
  switch (t) {
    case 'character':
      return 'parenthetical';
    case 'parenthetical':
      return 'dialogue';
    default:
      return 'action';
  }
}

/** 防抖间隔：太短会把每个按键变成一次全量 IPC，太长会丢字 */
const COMMIT_DEBOUNCE_MS = 220;

interface FocusRequest {
  blockId: string;
  caret: number;
}

export default function ScreenplayEditor() {
  const screenplay = useEditorStore((s) => s.document);
  const zoom = useEditorStore((s) => s.zoom);
  const activeBlockId = useEditorStore((s) => s.activeBlockId);
  const setActiveBlock = useEditorStore((s) => s.setActiveBlock);
  const changeBlockText = useEditorStore((s) => s.changeBlockText);
  const insertBlock = useEditorStore((s) => s.insertBlock);
  const deleteBlock = useEditorStore((s) => s.deleteBlock);
  const mergeBlock = useEditorStore((s) => s.mergeBlock);
  const pushToast = useEditorStore((s) => s.pushToast);

  const [pagination, setPagination] = useState<Pagination | null>(null);
  const workspaceRef = useRef<HTMLDivElement>(null);
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const [toolPosition, setToolPosition] = useState({ top: 0, left: 0, maxHeight: 0, visible: true });
  const [shortcuts, setShortcuts] = useState(readShortcuts);
  const [menuMode, setMenuMode] = useState<TypeMenuMode>(readTypeMenuMode);
  const [menuCollapsed, setMenuCollapsed] = useState(readTypeMenuCollapsed);
  const setMenuModePersisted = (mode: TypeMenuMode) => {
    setMenuMode(mode);
    try {
      localStorage.setItem(TYPE_MENU_KEY, mode);
    } catch {
      /* 隐私模式下忽略，仅当次生效 */
    }
  };
  const setMenuCollapsedPersisted = (collapsed: boolean) => {
    setMenuCollapsed(collapsed);
    try {
      localStorage.setItem(TYPE_MENU_COLLAPSED_KEY, collapsed ? '1' : '0');
    } catch {
      /* 隐私模式下忽略，仅当次生效 */
    }
  };
  /** 切到跟随光标：确保有锚点块并滚到可见处，点一下就在光标旁出现。 */
  const switchToFloat = () => {
    const state = useEditorStore.getState();
    const blocks = state.document?.blocks ?? [];
    if (!state.activeBlockId && blocks.length) state.setActiveBlock(blocks[0].id);
    setMenuModePersisted('float');
    requestAnimationFrame(() => {
      const id = useEditorStore.getState().activeBlockId;
      const row = id ? workspaceRef.current?.querySelector<HTMLElement>(`[data-block-id="${cssEscape(id)}"]`) : null;
      row?.scrollIntoView({ block: 'nearest' });
    });
  };
  const targetId = focusedId ? sourceBlockId(focusedId) : activeBlockId;
  const switchType = useCallback(async (id: string, type: BlockType, restoreFocus = false) => {
    const editable = workspaceRef.current?.querySelector<HTMLElement>(`[data-block-id="${cssEscape(focusedId && sourceBlockId(focusedId)===id ? focusedId : id)}"] .screenplay-block`);
    const caret = editable ? getCaretOffset(editable) ?? 0 : 0;
    await flushPendingEdits();
    setActiveBlock(id);
    await useEditorStore.getState().changeBlockType(id, type);
    if (restoreFocus) requestAnimationFrame(() => { editable?.focus(); if (editable) setCaretAt(editable, caret); });
  }, [setActiveBlock, focusedId]);
  useEffect(() => {
    const refresh = () => setShortcuts(readShortcuts());
    window.addEventListener('muben-shortcuts-changed', refresh);
    return () => window.removeEventListener('muben-shortcuts-changed', refresh);
  }, []);
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.isComposing || composingRef.current || !(e.target instanceof HTMLElement) || !e.target.closest('.screenplay-block')) return;
      const type = ELEMENT_TYPES.find(t => shortcuts[t] && shortcuts[t] === keyCombo(e));
      const rawId = e.target.closest<HTMLElement>('[data-block-id]')?.dataset.blockId;
      const id = rawId ? sourceBlockId(rawId) : undefined;
      if (!type || !id) return;
      e.preventDefault();
      void switchType(id, type, true).catch(error => pushToast('error', String(error)));
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [shortcuts, switchType, pushToast]);
  useEffect(() => {
    const update = () => {
      const workspace = workspaceRef.current;
      const id = focusedId ?? activeBlockId;
      const row = workspace?.querySelector<HTMLElement>(`[data-block-id="${cssEscape(id ?? '')}"]`);
      if (!workspace || !row) return;
      const bounds = workspace.getBoundingClientRect(), r = row.getBoundingClientRect();
      const menuHeight = workspace.querySelector<HTMLElement>('.float-menu')?.offsetHeight ?? 0;
      const placed = floatingMenuTop({
        rowTop: r.top - bounds.top,
        rowBottom: r.bottom - bounds.top,
        containerHeight: bounds.height,
        menuHeight,
      });
      setToolPosition({
        top: placed.top,
        maxHeight: placed.maxHeight,
        left: Math.max(4, r.left - bounds.left - (menuCollapsed ? 72 : 136)),
        visible: r.bottom >= bounds.top && r.top < bounds.bottom,
      });
    };
    update();
    window.addEventListener('resize', update);
    document.addEventListener('scroll', update, true);
    const observer = new ResizeObserver(update);
    if (workspaceRef.current) observer.observe(workspaceRef.current);
    return () => { window.removeEventListener('resize', update); document.removeEventListener('scroll', update, true); observer.disconnect(); };
  }, [focusedId, activeBlockId, pagination, zoom, menuCollapsed]);
  const layoutRef = useRef<HTMLDivElement>(null);
  const [availableWidth, setAvailableWidth] = useState(1000);
  // 滚轮浏览时跟随高亮：只更新 activeBlockId，绝不触发滚动；正在编辑或刚手动跳转时不动。
  useEffect(() => {
    const scroller = layoutRef.current;
    if (!scroller) return;
    let frame = 0;
    const pick = () => {
      frame = 0;
      if (sceneFollowSuppressed()) return;
      const view = scroller.getBoundingClientRect();
      // 正在编辑的段落在视野内时不动高亮，避免打字被滚动跟随抢走。
      const focusedEl = focusedId ? scroller.querySelector<HTMLElement>(`[data-block-id="${cssEscape(focusedId)}"]`) : null;
      if (focusedEl) {
        const r = focusedEl.getBoundingClientRect();
        if (r.bottom > view.top && r.top < view.bottom) return;
      }
      const top = view.top + 24;
      let id: string | null = null;
      for (const row of scroller.querySelectorAll<HTMLElement>('[data-block-id]')) {
        if (row.getBoundingClientRect().bottom > top) {
          id = row.dataset.blockId ?? null;
          break;
        }
      }
      if (!id || useEditorStore.getState().activeBlockId === id) return;
      useEditorStore.getState().setActiveBlock(id);
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(pick);
    };
    scroller.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      scroller.removeEventListener('scroll', onScroll);
      if (frame) cancelAnimationFrame(frame);
    };
  }, [focusedId, pagination]);

  useEffect(() => {
    const parent = layoutRef.current?.parentElement;
    if (!parent) return;
    const observer = new ResizeObserver(() => setAvailableWidth(parent.clientWidth));
    observer.observe(parent);
    setAvailableWidth(parent.clientWidth);
    return () => observer.disconnect();
  }, [pagination !== null]);
  const composingRef = useRef(false);
  const focusRequest = useRef<FocusRequest | null>(null);

  // 纸张尺寸（英寸）来自 Rust 侧 settings，用户改设置后立即生效
  const paper = screenplay?.settings;
  const pageW = paper ? `${paper.page_width_inch}in` : '8.5in';
  const pageH = paper ? `${paper.page_height_inch}in` : '11in';
  const padTop = paper ? `${paper.margin_top_inch}in` : '1in';
  const padBottom = paper ? `${paper.margin_bottom_inch}in` : '1in';
  const padLeft = paper ? `${paper.margin_left_inch}in` : '1.5in';
  const padRight = paper ? `${paper.margin_right_inch}in` : '1in';

  // 分页从 Rust 拉取，与导出共用
  useEffect(() => {
    let cancelled = false;
    if (!screenplay) {
      setPagination(null);
      return;
    }
    void ipc
      .getPagination()
      .then((p) => {
        if (!cancelled) setPagination(p);
      })
      .catch(() => {
        /* 分页失败不阻塞编辑 */
      });
    return () => {
      cancelled = true;
    };
  }, [screenplay]);

  // ---- IME 组字期间抑制所有提交 ----
  useEffect(() => {
    const onStart = () => {
      composingRef.current = true;
      useEditorStore.getState().setComposing(true);
    };
    const onEnd = () => {
      composingRef.current = false;
      useEditorStore.getState().setComposing(false);
    };
    window.addEventListener('compositionstart', onStart, true);
    window.addEventListener('compositionend', onEnd, true);
    return () => {
      window.removeEventListener('compositionstart', onStart, true);
      window.removeEventListener('compositionend', onEnd, true);
    };
  }, []);

  // 插入/合并后把光标放到目标块（分页重渲染会重建 DOM，所以放在渲染后执行）
  useEffect(() => {
    if (!pagination) return;
    const req = focusRequest.current;
    if (!req) return;
    const el = document.querySelector<HTMLElement>(
      `[data-block-id="${cssEscape(req.blockId)}"] .screenplay-block`
    );
    if (!el) return;
    // 只有当目标块当前**没有焦点**时才接管光标，
    // 否则会把用户正在输入的位置抢走。
    if (document.activeElement !== el) {
      el.focus();
      setCaretAt(el, req.caret);
    }
    focusRequest.current = null;
  }, [pagination, screenplay?.revision]);

  const requestFocus = useCallback((blockId: string, caret: number) => {
    focusRequest.current = { blockId, caret };
  }, []);

  // -----------------------------------------------------------------
  // 块级多选：Shift 点击设端点、拖过块边界、⌘A 选当前场/全文。
  // 每块仍是独立 contenteditable，块选区只做「块的身份」选择，
  // 复制/剪切/粘贴按块搬运并保留元素类型。
  // -----------------------------------------------------------------
  const [blockRange, setBlockRange] = useState<{ anchor: string; focus: string } | null>(null);
  const blockRangeRef = useRef(blockRange);
  blockRangeRef.current = blockRange;
  const dragAnchor = useRef<string | null>(null);
  const dragCrossed = useRef(false);
  const [dragSelecting, setDragSelecting] = useState(false);
  const blockIds = useMemo(() => (screenplay?.blocks ?? []).map((b) => b.id), [screenplay]);
  const selectedIds = useMemo(
    () => (blockRange ? orderedBlockRange(blockIds, blockRange.anchor, blockRange.focus) : []),
    [blockRange, blockIds]
  );
  const selectedIdsRef = useRef<string[]>([]);
  selectedIdsRef.current = selectedIds;
  const selectedSet = useMemo(() => new Set(selectedIds), [selectedIds]);

  const rowForBlock = useCallback((id: string): HTMLElement | null => {
    const rows = workspaceRef.current?.querySelectorAll<HTMLElement>('[data-block-id]') ?? [];
    for (const row of rows) if (sourceBlockId(row.dataset.blockId ?? '') === id) return row;
    return null;
  }, []);

  /** 块级粘贴：插到选中块末尾；没有块选区时插到当前块之后；都没有则追加到文末。 */
  const insertBlocksAfterSelection = useCallback(async (blocks: { type: BlockType; text: string }[]) => {
    const doc = useEditorStore.getState().document;
    if (!doc || !blocks.length) return;
    const ids = selectedIdsRef.current;
    let after: string | null = null;
    if (ids.length) after = ids[ids.length - 1];
    else {
      const active = useEditorStore.getState().activeBlockId;
      if (active && doc.blocks.some((b) => b.id === active)) after = active;
    }
    const r = await useEditorStore.getState().applyBlockBatch({
      insertAfter: after,
      insert: blocks.map((b) => ({ blockType: b.type, text: b.text })),
    });
    if (!r) return;
    const inserted = r.affected.filter((id) => !doc.blocks.some((b) => b.id === id));
    if (inserted.length) setBlockRange({ anchor: inserted[0], focus: inserted[inserted.length - 1] });
  }, []);

  const removeSelectedBlocks = useCallback(async () => {
    const ids = selectedIdsRef.current;
    if (!ids.length) return;
    const doc = useEditorStore.getState().document;
    const firstIdx = doc ? doc.blocks.findIndex((b) => b.id === ids[0]) : -1;
    // 删除后优先落到上一块；从文首删除时落到后面第一块，避免锚到已删块。
    const next = doc && firstIdx >= 0 ? (doc.blocks[firstIdx - 1] ?? doc.blocks[firstIdx + ids.length]) : undefined;
    setBlockRange(null);
    const r = await useEditorStore.getState().applyBlockBatch({ delete: ids });
    if (!r) return;
    if (next) {
      useEditorStore.getState().setActiveBlock(next.id);
      requestFocus(next.id, 0);
    }
  }, [requestFocus]);

  /** 原生选区跨了多个块：合并首块前缀与末块后缀，中间块删除，一步撤销。 */
  const deleteAcrossBlocks = useCallback(
    async (startId: string, endId: string, range: Range) => {
      await flushPendingEdits();
      const doc = useEditorStore.getState().document;
      if (!doc) return;
      const blocks = doc.blocks;
      const si = blocks.findIndex((b) => b.id === startId);
      const ei = blocks.findIndex((b) => b.id === endId);
      if (si < 0 || ei < si) return;
      const startRow = closestRow(range.startContainer) ?? rowForBlock(startId);
      const endRow = closestRow(range.endContainer) ?? rowForBlock(endId);
      const startEl = startRow?.querySelector<HTMLElement>('.screenplay-block');
      const endEl = endRow?.querySelector<HTMLElement>('.screenplay-block');
      const startBase = Number(startEl?.dataset.sourceStart ?? 0) || 0;
      const endBase = Number(endEl?.dataset.sourceStart ?? 0) || 0;
      const startOff = Math.max(0, startBase + (startEl ? offsetWithin(startEl, range.startContainer, range.startOffset, 0) : 0));
      const endLen = endEl?.textContent?.length ?? blocks[ei].text.length;
      const endOff = Math.max(0, endBase + (endEl ? offsetWithin(endEl, range.endContainer, range.endOffset, endLen) : blocks[ei].text.length));
      // 同一源块被分页拆成多段：按 source offset 在单块内删除，不走「不同块」分支。
      if (si === ei) {
        const from = Math.min(startOff, endOff);
        const to = Math.max(startOff, endOff);
        const text = blocks[si].text;
        const clippedTo = Math.min(to, text.length);
        if (clippedTo <= from) return;
        const replacement = text.slice(0, from) + text.slice(clippedTo);
        if (replacement === text) return;
        const same = await useEditorStore.getState().applyBlockBatch({ replace: [{ blockId: startId, text: replacement }] });
        if (same) requestFocus(startId, from);
        return;
      }
      const head = blocks[si].text.slice(0, Math.min(startOff, blocks[si].text.length));
      // 选区停在末块开头时只删首块的尾巴，不能整块吞掉末块
      const keepEnd = endOff === 0;
      const tail = keepEnd ? '' : blocks[ei].text.slice(Math.min(endOff, blocks[ei].text.length));
      const replacement = head + tail;
      const del = blocks.slice(si + 1, keepEnd ? ei : ei + 1).map((b) => b.id);
      const batch: { replace?: { blockId: string; text: string }[]; delete?: string[] } = {};
      if (replacement !== blocks[si].text) batch.replace = [{ blockId: startId, text: replacement }];
      if (del.length) batch.delete = del;
      if (!batch.replace && !batch.delete) return;
      const r = await useEditorStore.getState().applyBlockBatch(batch);
      if (r) requestFocus(startId, head.length);
    },
    [requestFocus, rowForBlock]
  );

  /** 焦点是否在正文可编辑块里：块选区、⌘C/⌘X、粘贴与 Esc 共用同一判断。 */
  const focusInText = useCallback((): boolean => {
    const active = document.activeElement as HTMLElement | null;
    if (!active || !workspaceRef.current?.contains(active)) return false;
    const block = active.closest<HTMLElement>('.screenplay-block');
    if (!block) return false;
    return block.getAttribute('contenteditable') !== 'false';
  }, []);

  /** 事务式复制/剪切：先落盘草稿、取稳定载荷、写剪贴板成功后才删除。 */
  const copySelectedBlocks = useCallback(
    async (cut: boolean): Promise<boolean> => {
      const ids = selectedIdsRef.current;
      if (!ids.length) return false;
      try {
        await flushPendingEdits();
      } catch (error) {
        useEditorStore.getState().pushToast('error', String(error));
        return false;
      }
      const doc = useEditorStore.getState().document;
      const picked = ids.flatMap((id) => doc?.blocks.find((b) => b.id === id) ?? []);
      if (!picked.length) return false;
      const text = blocksToText(picked);
      const ok = await writeClipboardText(text);
      if (!ok) {
        useEditorStore
          .getState()
          .pushToast('error', cut ? '复制到剪贴板失败，所选段落未删除' : '复制到剪贴板失败，请检查系统剪贴板权限');
        return false;
      }
      rememberBlockClipboard(text, picked.map((b) => ({ type: b.block_type, text: b.text })));
      if (cut) await removeSelectedBlocks();
      return true;
    },
    [removeSelectedBlocks]
  );

  useEffect(() => {
    /** 弹层/对话框自己处理 Esc，兜底层不抢。 */
    const inDialog = () => !!document.activeElement?.closest?.('[role="dialog"], [aria-modal="true"]');
    /**
     * 选区端点可能在不可编辑的括号装饰里（整段选中括注时），
     * 此时焦点常常不在正文 contenteditable 上，keydown 不会冒泡到块组件。
     * 因此窗口层也要能独立接管纸面选区。
     */
    const selectionInfo = () => {
      const s = window.getSelection();
      if (!s || s.isCollapsed || s.rangeCount === 0) return null;
      const r = s.getRangeAt(0);
      const sRow = closestRow(r.startContainer);
      const eRow = closestRow(r.endContainer);
      const ws = workspaceRef.current;
      if (!ws || (!sRow?.closest('.paper-workspace') && !eRow?.closest('.paper-workspace'))) return null;
      if (sRow && !ws.contains(sRow)) return null;
      if (eRow && !ws.contains(eRow)) return null;
      const sEl = sRow?.querySelector<HTMLElement>('.screenplay-block') ?? null;
      const eEl = eRow?.querySelector<HTMLElement>('.screenplay-block') ?? null;
      const insideOne = !!sRow && sRow === eRow && !!sEl && sEl.contains(r.startContainer) && sEl.contains(r.endContainer);
      return { r, sRow, eRow, sEl, eEl, insideOne };
    };
    const deleteInPaper = (info: NonNullable<ReturnType<typeof selectionInfo>>) => {
      const { r, sRow, eRow } = info;
      if (sRow && eRow && sRow !== eRow) {
        const sId = sourceBlockId(sRow.dataset.blockId ?? '');
        const eId = sourceBlockId(eRow.dataset.blockId ?? '');
        if (sId && eId) {
          void deleteAcrossBlocks(sId, eId, r).catch((error) => useEditorStore.getState().pushToast('error', String(error)));
        }
        return;
      }
      const el = info.sEl ?? info.eEl ?? (sRow ?? eRow)?.querySelector<HTMLElement>('.screenplay-block');
      if (!el) return;
      const current = el.textContent ?? '';
      const len = current.length;
      const from = Math.max(0, Math.min(offsetWithin(el, r.startContainer, r.startOffset, 0), len));
      const to = Math.max(from, Math.min(offsetWithin(el, r.endContainer, r.endOffset, len), len));
      el.textContent = current.slice(0, from) + current.slice(to);
      el.focus();
      setCaretAt(el, from);
      // 交给 BlockView 的 onInput 走防抖提交，保留分页片段上下文
      el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'deleteContentBackward' }));
    };
    const onKey = (e: KeyboardEvent) => {
      if (composingRef.current || e.isComposing) return;
      const mod = e.metaKey || e.ctrlKey;
      const key = e.key.toLowerCase();
      const range = blockRangeRef.current;
      // 焦点不在正文：不接管任何块操作，并清掉残留块选区（工具栏、弹层、场景板等）。
      if (!focusInText()) {
        if (range) setBlockRange(null);
        return;
      }
      const info = selectionInfo();
      const activeEditable = !!document.activeElement?.closest?.('.screenplay-block');
      // 撤销/重做会整体重建文档：块选区此时已失效，先清掉，
      // 避免「界面无高亮但下一次删除仍按块执行」。
      if (mod && !e.altKey && key === 'z') setBlockRange(null);
      if (e.key === 'Escape') {
        if (!range && !info) return;
        if (inDialog()) return;
        e.preventDefault();
        e.stopPropagation();
        setBlockRange(null);
        window.getSelection()?.removeAllRanges();
        return;
      }
      const ids = selectedIdsRef.current;
      // Shift+↑/↓：已有块选区时按块扩展/收缩；没有时从当前块起选（仅接管折叠的文字光标）。
      if (e.shiftKey && !mod && !e.altKey && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
        const doc = useEditorStore.getState().document;
        if (!doc?.blocks.length) return;
        let current = range;
        if (!current) {
          const sel = window.getSelection();
          if (sel && !sel.isCollapsed) return;
          const seed = focusedId ? sourceBlockId(focusedId) : useEditorStore.getState().activeBlockId;
          if (!seed) return;
          current = { anchor: seed, focus: seed };
        }
        const all = doc.blocks.map((b) => b.id);
        const from = all.indexOf(current.focus);
        if (from < 0) return;
        const to = Math.min(all.length - 1, Math.max(0, from + (e.key === 'ArrowDown' ? 1 : -1)));
        e.preventDefault();
        e.stopPropagation();
        setBlockRange({ anchor: current.anchor, focus: all[to] });
        window.getSelection()?.removeAllRanges();
        return;
      }
      if (mod && !e.shiftKey && !e.altKey && key === 'a') {
        e.preventDefault();
        e.stopPropagation();
        const doc = useEditorStore.getState().document;
        if (!doc?.blocks.length) return;
        const allIds = doc.blocks.map((b) => b.id);
        const scene = sceneBlockIds(doc.blocks, useEditorStore.getState().activeBlockId);
        const same = (a: string[], b: string[]) => a.length === b.length && a.every((v, i) => v === b[i]);
        if (range && same(ids, allIds)) return;
        if (range && scene && same(ids, scene)) {
          setBlockRange({ anchor: allIds[0], focus: allIds[allIds.length - 1] });
          return;
        }
        const target = scene ?? allIds;
        setBlockRange({ anchor: target[0], focus: target[target.length - 1] });
        window.getSelection()?.removeAllRanges();
        return;
      }
      if (mod && !e.shiftKey && !e.altKey && (key === 'c' || key === 'x')) {
        if (ids.length) {
          e.preventDefault();
          e.stopPropagation();
          void copySelectedBlocks(key === 'x');
          return;
        }
        // 原生文字选区：焦点不在正文（例如端点落在括注括号）时浏览器切不动。
        // 剪切同样是事务：写剪贴板成功后才删。
        if (info && (info.sRow !== info.eRow || !activeEditable)) {
          e.preventDefault();
          e.stopPropagation();
          const text = info.r.toString();
          void (async () => {
            const ok = await writeClipboardText(text);
            if (!ok) {
              useEditorStore
                .getState()
                .pushToast('error', key === 'x' ? '复制到剪贴板失败，选区未删除' : '复制到剪贴板失败');
              return;
            }
            if (key === 'x') deleteInPaper(info);
          })();
        }
        return;
      }
      if (!mod && !e.shiftKey && !e.altKey && (e.key === 'Backspace' || e.key === 'Delete')) {
        if (range) {
          e.preventDefault();
          e.stopPropagation();
          void removeSelectedBlocks();
          return;
        }
        if (info && (info.sRow !== info.eRow || !info.insideOne || !activeEditable)) {
          e.preventDefault();
          e.stopPropagation();
          deleteInPaper(info);
        }
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [removeSelectedBlocks, deleteAcrossBlocks, copySelectedBlocks, focusInText, focusedId]);

  // 块剪贴板粘贴：只在焦点位于正文时拦截，避免在搜索框/标题框里粘贴插进剧本。
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      if (composingRef.current) return;
      if (!focusInText()) return;
      const text = e.clipboardData?.getData('text/plain') ?? '';
      const payload = readBlockClipboard(text);
      if (!payload || !useEditorStore.getState().document) return;
      e.preventDefault();
      e.stopPropagation();
      void insertBlocksAfterSelection(payload);
    };
    window.addEventListener('paste', onPaste, true);
    return () => window.removeEventListener('paste', onPaste, true);
  }, [insertBlocksAfterSelection, focusInText]);

  // macOS 原生 Edit 菜单会先截走 ⌘C/⌘X，页面收不到 keydown；
  // 因此在 copy/cut 事件里兜底，并走同一条事务：写剪贴板成功后才删除。
  useEffect(() => {
    const onCopyCut = (e: ClipboardEvent) => {
      if (composingRef.current) return;
      if (!selectedIdsRef.current.length || !focusInText()) return;
      e.preventDefault();
      e.stopPropagation();
      void copySelectedBlocks(e.type === 'cut');
    };
    window.addEventListener('copy', onCopyCut, true);
    window.addEventListener('cut', onCopyCut, true);
    return () => {
      window.removeEventListener('copy', onCopyCut, true);
      window.removeEventListener('cut', onCopyCut, true);
    };
  }, [copySelectedBlocks, focusInText]);

  // Esc 兜底：焦点不在正文（工具栏、场景板等）时也能清块选区；
  // 但组字中、弹层内、外部可编辑区里的 Esc 交给它们自己处理。
  useEffect(() => {
    const onEsc = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || !blockRangeRef.current) return;
      if (composingRef.current || e.isComposing) return;
      const active = document.activeElement as HTMLElement | null;
      if (active?.closest?.('[role="dialog"], [aria-modal="true"]')) return;
      if (
        active &&
        !workspaceRef.current?.contains(active) &&
        (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.isContentEditable)
      )
        return;
      e.preventDefault();
      setBlockRange(null);
      window.getSelection()?.removeAllRanges();
    };
    document.addEventListener('keydown', onEsc, true);
    return () => document.removeEventListener('keydown', onEsc, true);
  }, []);

  // 撤销/文档变化后端点可能已不存在：清掉块选区，避免「无高亮但 Delete 被吞」。
  useEffect(() => {
    if (!blockRange) return;
    if (!blockIds.includes(blockRange.anchor) || !blockIds.includes(blockRange.focus)) setBlockRange(null);
  }, [blockRange, blockIds]);

  // 焦点离开正文（工具栏按钮、弹层、场景板、帮助面板）→ 立刻清块选区。
  useEffect(() => {
    const clearIfOutside = () => {
      if (blockRangeRef.current && !focusInText()) setBlockRange(null);
    };
    document.addEventListener('focusin', clearIfOutside, true);
    document.addEventListener('pointerdown', clearIfOutside, true);
    return () => {
      document.removeEventListener('focusin', clearIfOutside, true);
      document.removeEventListener('pointerdown', clearIfOutside, true);
    };
  }, [focusInText]);

  useEffect(() => {
    const onUp = () => {
      const hadAnchor = !!dragAnchor.current;
      dragAnchor.current = null;
      dragCrossed.current = false;
      setDragSelecting(false);
      // 拖选/Shift 点选后把焦点交回正文，保证 ⌘C/⌘X 与 Shift+↑↓ 立即可用。
      if (hadAnchor && blockRangeRef.current && !focusInText()) {
        rowForBlock(blockRangeRef.current.focus)
          ?.querySelector<HTMLElement>('.screenplay-block')
          ?.focus({ preventScroll: true });
      }
    };
    window.addEventListener('mouseup', onUp);
    return () => window.removeEventListener('mouseup', onUp);
  }, [focusInText, rowForBlock]);

  /** 把键盘焦点交回指定块正文（gutter/拖选后立即能复制、扩展选区）。 */
  const focusBlockText = useCallback(
    (id: string) => {
      rowForBlock(id)?.querySelector<HTMLElement>('.screenplay-block')?.focus({ preventScroll: true });
    },
    [rowForBlock]
  );

  const onPaperMouseDown = (e: React.MouseEvent) => {
    const row = (e.target as HTMLElement).closest<HTMLElement>('[data-block-id]');
    if (!row) return;
    const id = sourceBlockId(row.dataset.blockId ?? '');
    if (!id) return;
    if (e.shiftKey) {
      e.preventDefault();
      // 合并对白行没有块选区时，Shift 点行内块以它自己为锚点，便于单独选中。
      const mergedInline = row.classList.contains('block-inline') && !!row.closest('.merged-dialogue-row');
      const anchor =
        blockRangeRef.current?.anchor ??
        (mergedInline ? id : useEditorStore.getState().activeBlockId ?? id);
      setBlockRange({ anchor, focus: id });
      useEditorStore.getState().setActiveBlock(id);
      focusBlockText(id);
      window.getSelection()?.removeAllRanges();
      return;
    }
    // 三击括注整行：按整块选中，退格时连同装饰括号一起删除，而不是只清空文字。
    if (e.detail >= 3) {
      const type = useEditorStore.getState().document?.blocks.find((b) => b.id === id)?.block_type;
      if (type === 'parenthetical') {
        e.preventDefault();
        dragAnchor.current = id;
        dragCrossed.current = false;
        setBlockRange({ anchor: id, focus: id });
        useEditorStore.getState().setActiveBlock(id);
        focusBlockText(id);
        window.getSelection()?.removeAllRanges();
        return;
      }
    }
    if (blockRangeRef.current) setBlockRange(null);
    dragAnchor.current = id;
    dragCrossed.current = false;
  };

  const onPaperMouseMove = (e: React.MouseEvent) => {
    if (!dragAnchor.current || (e.buttons & 1) === 0) return;
    const hit = (e.target as HTMLElement).closest<HTMLElement>('[data-block-id], .block-gutter');
    if (!hit) return;
    const id = hit.classList.contains('block-gutter')
      ? hit.dataset.gutterFor ?? ''
      : sourceBlockId(hit.dataset.blockId ?? '');
    if (!id) return;
    // 拖过合并对白行时把整组算进范围（视觉上是一行）。
    const merged = hit.closest<HTMLElement>('.merged-dialogue-row');
    let focusId = id;
    if (merged) {
      const members = merged.querySelectorAll<HTMLElement>('.block-inline[data-block-id]');
      const last = members[members.length - 1];
      if (last) focusId = sourceBlockId(last.dataset.blockId ?? '') || id;
    }
    // 回拖到锚点要收缩回单块，而不是停在最远范围。
    if (focusId !== dragAnchor.current) dragCrossed.current = true;
    if (focusId === dragAnchor.current && !dragCrossed.current) return;
    if (!blockRangeRef.current) setDragSelecting(true);
    setBlockRange({ anchor: dragAnchor.current, focus: focusId });
    window.getSelection()?.removeAllRanges();
  };

  /** 左侧 gutter：单击选中整块、Shift 点击扩范围、按住拖动连选；不侵入正文点击区。 */
  const onGutterDown = (e: React.MouseEvent, id: string) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    // 合并对白行：gutter 选中整组（行内文字仍可 Shift 点选单个块）。
    let anchorId = id;
    let focusId = id;
    const mergedRow = (e.currentTarget as HTMLElement).parentElement;
    if (mergedRow?.classList.contains('merged-dialogue-row')) {
      const members = Array.from(mergedRow.querySelectorAll<HTMLElement>('.block-inline[data-block-id]'))
        .map((el) => sourceBlockId(el.dataset.blockId ?? ''))
        .filter(Boolean);
      if (members.length) {
        anchorId = members[0];
        focusId = members[members.length - 1];
      }
    }
    if (e.shiftKey) {
      const anchor = blockRangeRef.current?.anchor ?? useEditorStore.getState().activeBlockId ?? anchorId;
      setBlockRange({ anchor, focus: focusId });
      useEditorStore.getState().setActiveBlock(anchorId);
      focusBlockText(focusId);
    } else {
      dragAnchor.current = anchorId;
      dragCrossed.current = false;
      setBlockRange({ anchor: anchorId, focus: focusId });
      useEditorStore.getState().setActiveBlock(anchorId);
      focusBlockText(anchorId);
    }
    window.getSelection()?.removeAllRanges();
  };

  /** 键盘/读屏激活 gutter（无拖拽语义）：选中该块并把焦点交回正文。 */
  const selectGutterBlock = useCallback(
    (id: string) => {
      dragAnchor.current = id;
      dragCrossed.current = false;
      setBlockRange({ anchor: id, focus: id });
      useEditorStore.getState().setActiveBlock(id);
      focusBlockText(id);
      window.getSelection()?.removeAllRanges();
    },
    [focusBlockText]
  );

  const starting = useRef(false);
  const startWriting = async () => {
    const doc = useEditorStore.getState().document;
    if (!doc || doc.blocks.length || starting.current) return;
    starting.current = true;
    try {
      const result = await ipc.insertBlock(null, 'scene_heading', '', doc.revision);
      if (useEditorStore.getState().document?.id !== doc.id) return;
      if (result.affected[0]) requestFocus(result.affected[0], 0);
      await useEditorStore.getState().applyResult(result);
      if (result.affected[0]) setActiveBlock(result.affected[0]);
    } catch (error) { pushToast('error', String(error)); }
    finally { starting.current = false; }
  };

  // ---- 键盘：回车拆分、退格合并/删除 ----
  const onKeyDown = useCallback(
    (e: React.KeyboardEvent, blockId: string) => {
      // IME 组字中不拦截任何按键
      if (composingRef.current || (e.nativeEvent as any).isComposing) return;

      const el = e.currentTarget as HTMLElement;

      // 选区优先：括注括号在 contenteditable 之外，整段选中时端点落在
      // `.bracket-decoration` 里；若继续走「块首退格合并」会算错光标位置。
      if ((e.key === 'Backspace' || e.key === 'Delete') && !blockRangeRef.current) {
        const sel = window.getSelection();
        if (sel && !sel.isCollapsed && sel.rangeCount > 0) {
          const range = sel.getRangeAt(0);
          const touches = el.contains(range.startContainer) || el.contains(range.endContainer)
            || (typeof range.intersectsNode === 'function' && range.intersectsNode(el));
          if (touches) {
            const startRow = closestRow(range.startContainer);
            const endRow = closestRow(range.endContainer);
            const startId = startRow ? sourceBlockId(startRow.dataset.blockId ?? '') : blockId;
            const endId = endRow ? sourceBlockId(endRow.dataset.blockId ?? '') : blockId;
            if (startId && endId && startId !== endId) {
              e.preventDefault();
              void deleteAcrossBlocks(startId, endId, range).catch((error) => useEditorStore.getState().pushToast('error', String(error)));
            }
            // 同一块内的选区交给浏览器/BlockView；这里只兜住「不能误当成块首合并」
            return;
          }
        }
      }

      if (e.key === 'Enter' && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        const blocks = useEditorStore.getState().document?.blocks ?? [];
        const cur = blocks.find((b) => b.id === blockId);
        if (!cur) return;
        const local = stripAutoContinuation(el.textContent ?? '', el.dataset.autoContd === 'true');
        const start = el.dataset.sourceStart === undefined ? undefined : Number(el.dataset.sourceStart);
        const end = el.dataset.sourceEnd === undefined ? undefined : Number(el.dataset.sourceEnd);
        const context = getFragmentDraft(el) ?? fragmentContext(cur.text, start, end);
        const full = context.prefix + local + context.suffix;
        // 有选区时先把选区算进去：删除选中文字后按光标位置拆分，
        // 不能让被选中的文字继续留在原块里。
        const selection = getSelectionOffsets(el);
        const caretLocal = Math.min(selection ? selection.start : getCaretOffset(el) ?? local.length, local.length);
        const tailLocal = Math.min(selection ? selection.end : caretLocal, local.length);
        const caret = context.prefix.length + caretLocal;
        const head = full.slice(0, caret);
        const tail = full.slice(context.prefix.length + tailLocal);
        const nextType = nextTypeFor(cur.block_type);

        // 当前块保留光标前的文字，新块承载光标后的文字
        const apply = async () => {
          await flushPendingEdits();
          if (head !== full) {
            const ok = await useEditorStore.getState().changeBlockText(blockId, head);
            if (!ok) return;
          }
          await useEditorStore.getState().insertBlock(blockId, nextType, tail);
          const insertedId = useEditorStore.getState().activeBlockId;
          if (insertedId && insertedId !== blockId) {
            requestFocus(insertedId, 0);
            requestAnimationFrame(() => {
              const target = document.querySelector<HTMLElement>(`[data-block-id="${cssEscape(insertedId)}"] .screenplay-block`);
              if (target) { target.focus(); setCaretAt(target, 0); target.scrollIntoView?.({ block: "nearest" }); focusRequest.current = null; }
            });
          }
        };
        void apply().catch((error) => useEditorStore.getState().pushToast('error', String(error)));
        return;
      }

      if (e.key === 'Backspace') {
        // 有选区时交给浏览器删除选中文字：选区起点常在块首，
        // 若按「块首退格」处理会把整段文字并进上一块。
        const sel = window.getSelection();
        const hasSelection = !!sel && !sel.isCollapsed && sel.rangeCount > 0
          && (el.contains(sel.getRangeAt(0).startContainer)
            || el.contains(sel.getRangeAt(0).endContainer)
            || (typeof sel.getRangeAt(0).intersectsNode === 'function' && sel.getRangeAt(0).intersectsNode(el)));
        if (hasSelection) return;
        const full = stripAutoContinuation(el.textContent ?? '', el.dataset.autoContd === 'true');
        const caret = (Number(el.dataset.sourceStart) || 0) + (getCaretOffset(el) ?? 0);
        const blocks = useEditorStore.getState().document?.blocks ?? [];
        const idx = blocks.findIndex((b) => b.id === blockId);

        if (caret === 0 && full === '') {
          if (idx > 0) {
            e.preventDefault();
            const prev = blocks[idx - 1];
            const prevLen = prev.text.length;
            void deleteBlock(blockId);
            requestFocus(prev.id, prevLen);
          }
          return;
        }

        if (caret === 0 && idx > 0) {
          // 专业编辑器行为：块首退格 = 与上一块合并
          e.preventDefault();
          const prev = blocks[idx - 1];
          const prevLen = prev.text.length;
          // 先提交防抖中的草稿，否则刚输入还没上送的文字会在合并时丢失
          void (async () => {
            await flushPendingEdits();
            await mergeBlock(prev.id, blockId);
            requestFocus(prev.id, prevLen);
          })().catch((error) => useEditorStore.getState().pushToast('error', String(error)));
          return;
        }

        if (full === '' && idx === 0) {
          e.preventDefault();
          void deleteBlock(blockId);
        }
        return;
      }

      // Tab 切换元素类型（剧本写作习惯）
      if (e.key === 'Tab') {
        e.preventDefault();
        const blocks = useEditorStore.getState().document?.blocks ?? [];
        const cur = blocks.find((b) => b.id === blockId);
        if (!cur) return;
        const order: BlockType[] = [
          'scene_heading',
          'action',
          'character',
          'parenthetical',
          'dialogue',
          'transition',
        ];
        const at = order.indexOf(cur.block_type);
        // 不在序列中的类型（如 shot/note）从 action 之后接续，
        // 不能用 indexOf 的 -1 直接参与取模算错位置。
        const base = at >= 0 ? at : 1;
        const next = e.shiftKey
          ? order[(base - 1 + order.length) % order.length]
          : order[(base + 1) % order.length];
        void flushPendingEdits().then(() => useEditorStore.getState().changeBlockType(blockId, next)).catch((error) => useEditorStore.getState().pushToast('error', String(error)));
      }
    },
    [deleteBlock, mergeBlock, requestFocus, deleteAcrossBlocks]
  );

  const blocksById = useMemo(() => {
    const m = new Map<string, PageBlock>();
    for (const pg of pagination?.pages ?? []) {
      for (const pb of pg.blocks) m.set(pb.block_id, pb);
    }
    return m;
  }, [pagination]);

  if (!screenplay) {
    return <div className="screenplay-editor empty">{tr("未打开项目")}</div>;
  }

  if (!pagination) {
    return <div className="screenplay-editor empty">{tr("正在计算分页…")}</div>;
  }

  const selected = screenplay.blocks.find(b => b.id === targetId);
  const dockBlock = screenplay.blocks.find(b => b.id === (targetId ?? activeBlockId)) ?? null;
  return (
    <div className="paper-workspace" ref={workspaceRef} onMouseDown={onPaperMouseDown} onMouseMove={onPaperMouseMove} style={{ userSelect: dragSelecting ? 'none' : undefined }} onFocusCapture={e => {
      const element = e.target as HTMLElement;
      if (element.matches('.screenplay-block') && element.isContentEditable !== false) setFocusedId(element.closest<HTMLElement>('[data-block-id]')?.dataset.blockId ?? null);
    }} onBlurCapture={() => {
      requestAnimationFrame(() => {
        if (!workspaceRef.current?.contains(document.activeElement)) setFocusedId(null);
      });
    }}>
      <div className="sr-only" role="status" aria-live="polite">{blockRangeAnnouncement(selectedIds.length)}</div>
      {menuMode === 'float' && selected && (
        <div className={`row-element-tools float-menu ${menuCollapsed ? 'collapsed' : ''}`} aria-label={tr("当前元素工具")} style={{ top: toolPosition.top, left: toolPosition.left, maxHeight: toolPosition.maxHeight || undefined, visibility: toolPosition.visible ? 'visible' : 'hidden' }}>
          {menuCollapsed ? (
            <button type="button" className="float-menu-pill" title={tr("展开元素菜单")} onMouseDown={(e) => e.preventDefault()} onClick={() => setMenuCollapsedPersisted(false)}>
              <span>{tr("元素")}</span><span className="float-menu-caret" aria-hidden="true">▸</span>
            </button>
          ) : (
            <>
              <div className="float-menu-head">
                <span>{tr("元素")}</span>
                <button type="button" title={tr("收起元素菜单")} onMouseDown={(e) => e.preventDefault()} onClick={() => setMenuCollapsedPersisted(true)}>▾</button>
              </div>
              <div className="float-menu-list">
                {TYPE_ORDER.map((t) => (
                  <button key={t} type="button" className={selected.block_type === t ? 'on' : ''} title={`${tr(BLOCK_LABELS[t])} · ${shortcutLabel(shortcuts[t])}`}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => { void switchType(selected.id, t, true).catch(error => pushToast('error', String(error))); setMenuCollapsedPersisted(true); }}>
                    <span>{tr(BLOCK_LABELS[t])}</span><kbd className="element-dock-key" aria-hidden="true">{shortcutLabel(shortcuts[t])}</kbd>
                  </button>
                ))}
              </div>
              <div className="row-element-actions">
                <button type="button" title={tr("吸附到顶部横排，位置固定")} onMouseDown={(e) => e.preventDefault()} onClick={() => setMenuModePersisted('top')}>{tr("顶部")}</button>
                <button type="button" title={tr("吸附到左侧竖排，位置固定")} onMouseDown={(e) => e.preventDefault()} onClick={() => setMenuModePersisted('left')}>{tr("左侧")}</button>
              </div>
            </>
          )}
        </div>
      )}
      {menuMode !== 'float' && <div className={`element-dock ${menuMode} ${menuCollapsed ? 'collapsed' : ''}`} role="toolbar" aria-label={tr("元素类型")}>
        <button type="button" className="element-dock-toggle" aria-expanded={!menuCollapsed} title={menuCollapsed ? tr("展开元素菜单") : tr("收起元素菜单")} onMouseDown={(e) => e.preventDefault()} onClick={() => setMenuCollapsedPersisted(!menuCollapsed)}>
          <span className="element-dock-title">{tr("元素")}</span>
          <span className="element-dock-arrow" aria-hidden="true">{menuCollapsed ? (menuMode === 'left' ? '▸' : '▾') : (menuMode === 'left' ? '◂' : '▴')}</span>
        </button>
        {!menuCollapsed && <>
          {TYPE_ORDER.map((t) => <button key={t} type="button" disabled={!dockBlock} className={dockBlock?.block_type === t ? 'on' : ''} title={`${tr(BLOCK_LABELS[t])} · ${shortcutLabel(shortcuts[t])}`} onMouseDown={(e) => e.preventDefault()} onClick={() => { if (dockBlock) void switchType(dockBlock.id, t, true).catch(error => pushToast('error', String(error))); setMenuCollapsedPersisted(true); }}>{tr(BLOCK_LABELS[t])}<kbd className="element-dock-key" aria-hidden="true">{shortcutLabel(shortcuts[t])}</kbd></button>)}
          <button type="button" className="element-dock-float" title={tr("恢复跟随光标，位置随当前段落移动")} onMouseDown={(e) => e.preventDefault()} onClick={switchToFloat}>{tr("跟随光标")}</button>
        </>}
      </div>}
    <div className={`screenplay-editor typography-${paper?.typography ?? "hollywood"}`} ref={layoutRef} style={{ zoom: zoom === 0 ? Math.min(1, availableWidth / ((paper?.page_width_inch ?? 8.5) * 96 + 48)) : zoom, minWidth: `${(paper?.page_width_inch ?? 8.5) * 96 + 48}px`, fontFamily: paper?.font_family, fontSize: `${paper?.font_size ?? 12}pt`, lineHeight: `${Math.max(paper?.font_size ?? 12, ...Object.values(paper?.element_styles ?? {}).map(s => s?.font_size ?? 0)) * (paper?.line_height ?? 1)}pt` }}>
      <div className="pages-container" role="listbox" aria-multiselectable="true" aria-label={tr("剧本正文，可多选段落")}>
        {pagination.pages.map((page, i) => (
          <PageView
            key={i}
            page={page}
            blocksById={blocksById}
            pageW={pageW}
            pageH={pageH}
            padTop={padTop}
            padBottom={padBottom}
            padLeft={padLeft}
            padRight={padRight}
            activeBlockId={activeBlockId}
            selectedIds={selectedSet}
            onStartWriting={screenplay.blocks.length === 0 ? startWriting : undefined}
            onSelect={setActiveBlock}
            onGutterDown={onGutterDown}
            onGutterKey={selectGutterBlock}
            onChangeText={changeBlockText}
            onKeyDown={onKeyDown}
            onInsertAfter={insertBlock}
            onDelete={deleteBlock}
            onError={(m) => pushToast('error', m)}
          />
        ))}
      </div>
    </div>
    </div>
  );
}

function PageView({
  onStartWriting,
  page,
  blocksById,
  pageW,
  pageH,
  padTop,
  padBottom,
  padLeft,
  padRight,
  activeBlockId,
  selectedIds,
  onSelect,
  onGutterDown,
  onGutterKey,
  onChangeText,
  onKeyDown,
  onInsertAfter,
  onDelete,
  onError,
}: {
  onStartWriting?: () => Promise<void>;
  page: Page;
  blocksById: Map<string, PageBlock>;
  pageW: string;
  pageH: string;
  padTop: string;
  padBottom: string;
  padLeft: string;
  padRight: string;
  activeBlockId: string | null;
  selectedIds: Set<string>;
  onSelect: (id: string) => void;
  onGutterDown: (e: React.MouseEvent, id: string) => void;
  onGutterKey: (id: string) => void;
  onChangeText: (id: string, text: string, expectedRevision?: number) => Promise<boolean>;
  onKeyDown: (e: React.KeyboardEvent, id: string) => void;
  onInsertAfter: (afterId: string | null, type: BlockType, text?: string) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onError: (m: string) => void;
}) {
  const settings = useEditorStore((s) => s.document?.settings);
  const contentHeight = settings ? settings.page_height_inch - settings.margin_top_inch - settings.margin_bottom_inch : 9;
  const gridSize = Math.max(settings?.font_size ?? 12, ...Object.values(settings?.element_styles ?? {}).map(s => s?.font_size ?? 0));
  const lineHeight = settings?.line_height ?? 1;
  // 合并预设下，人物/括注/对白三块共用一行：同一个 group 的连续块收进一个行容器。
  const rows: PageBlock[][] = [];
  for (const pb of page.blocks) {
    const prev = rows[rows.length - 1];
    if (pb.group && prev && prev[prev.length - 1].group === pb.group) prev.push(pb);
    else rows.push([pb]);
  }
  return (
    <div
      className="page"
      style={{
        width: pageW,
        minHeight: pageH,
        paddingTop: padTop,
        paddingBottom: padBottom,
        paddingLeft: padLeft,
        paddingRight: padRight,
      }}
    >
      {page.show_page_number && <div className="page-number">{page.number}.</div>}
      <div className="page-content" style={{ position: "relative", height: `${contentHeight}in` }}>
        {onStartWriting && page.number === 1 && <button
          type="button"
          aria-label={tr("开始书写剧本")}
          style={{ width: '100%', height: '100%', display: 'block', textAlign: 'left', verticalAlign: 'top', background: 'transparent', border: 0, color: 'inherit', cursor: 'text', padding: 0 }}
          onClick={() => void onStartWriting()}
        ><span style={{ display: 'block', height: '100%', opacity: 0.5 }}>{tr("点击这里，开始书写场景标题…")}</span></button>}
        {rows.map((row) => {
          const head = blocksById.get(row[0].block_id);
          if (!head) return null;
          if (!head.group) {
            return (
              <BlockView
                key={head.block_id}
                pb={head}
                active={activeBlockId === sourceBlockId(head.block_id)}
                selected={selectedIds.has(sourceBlockId(head.block_id))}
                onSelect={onSelect}
                onGutterDown={onGutterDown}
                onGutterKey={onGutterKey}
                onChangeText={onChangeText}
                onKeyDown={onKeyDown}
                onInsertAfter={onInsertAfter}
                onDelete={onDelete}
                onError={onError}
              />
            );
          }
          const top = Math.min(...row.map((p) => (blocksById.get(p.block_id) ?? p).start_line));
          const groupSelected = row.some((p) => selectedIds.has(sourceBlockId(p.block_id)));
          return (
            <div
              key={`group:${head.group}`}
              className={`block-row merged-dialogue-row${groupSelected ? ' block-selected' : ''}`}
              role="option"
              aria-selected={groupSelected}
              style={{ position: 'absolute', top: `${top * gridSize * lineHeight}pt`, width: '100%', margin: 0 }}
            >
              <div
                className="block-gutter"
                role="button"
                tabIndex={-1}
                data-gutter-for={sourceBlockId(head.block_id)}
                aria-label={tr('选中此对白组')}
                title={tr('选中此对白组')}
                onMouseDown={(e) => onGutterDown(e, sourceBlockId(head.block_id))}
                onClick={(e) => { if (e.detail === 0) onGutterKey(sourceBlockId(head.block_id)); }}
              />
              {row.map((p) => {
                const fresh = blocksById.get(p.block_id);
                if (!fresh) return null;
                return (
                  <BlockView
                    key={fresh.block_id}
                    pb={fresh}
                    inline
                    active={activeBlockId === sourceBlockId(fresh.block_id)}
                    selected={selectedIds.has(sourceBlockId(fresh.block_id))}
                    onSelect={onSelect}
                    onGutterDown={onGutterDown}
                    onGutterKey={onGutterKey}
                    onChangeText={onChangeText}
                    onKeyDown={onKeyDown}
                    onInsertAfter={onInsertAfter}
                    onDelete={onDelete}
                    onError={onError}
                  />
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function BlockView({
  pb,
  active,
  selected,
  inline,
  onSelect,
  onGutterDown,
  onGutterKey,
  onChangeText,
  onKeyDown,
  onInsertAfter,
  onDelete,
  onError,
}: {
  pb: PageBlock;
  active: boolean;
  /** 块级多选中的可视选中态 */
  selected?: boolean;
  /** 合并预设的行内片段：不参与绝对定位，由外层行容器排布。 */
  inline?: boolean;
  onSelect: (id: string) => void;
  onGutterDown: (e: React.MouseEvent, id: string) => void;
  onGutterKey: (id: string) => void;
  onChangeText: (id: string, text: string, expectedRevision?: number) => Promise<boolean>;
  onKeyDown: (e: React.KeyboardEvent, id: string) => void;
  onInsertAfter: (afterId: string | null, type: BlockType, text?: string) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onError: (m: string) => void;
}) {
  const sourceId = sourceBlockId(pb.block_id);
  const fragmentDraft = useRef<{prefix:string;suffix:string}|null>(null);
  const captureDraft = () => {if (!userEdited.current && !fragmentDraft.current) { const doc=useEditorStore.getState().document; const source=doc?.blocks.find(b=>b.id===sourceId)?.text ?? pb.text; fragmentDraft.current=fragmentContext(source,pb.source_start,pb.source_end); baseRevision.current=doc?.revision; if(ref.current)rememberFragmentDraft(ref.current,fragmentDraft.current); }};
  const ref = useRef<HTMLDivElement>(null);
  const composing = useRef(false);
  const [completionInput, setCompletionInput] = useState<string | null>(null);
  const [completionIndex, setCompletionIndex] = useState(0);
  const timer = useRef<number | null>(null);
  const inFlight = useRef<Promise<boolean> | null>(null);
  /** 后端最后一次已知的文本 */
  const backendText = useRef(pb.text);
  /** 用户是否真的输入过 */
  const userEdited = useRef(false);
  const baseRevision = useRef<number>();
  /** 编辑期间外部来源给的新文本（用户没再动过就采纳它，不回写） */
  const externalWhileEditing = useRef<string | null>(null);

  // 外部更新（非本块编辑导致）时同步 DOM，但**绝不在组字期间**
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (composing.current) return;
    if ((el.textContent ?? '') === pb.text) {
      backendText.current = pb.text;
      fragmentDraft.current=null;rememberFragmentDraft(el,null);
      externalWhileEditing.current = null;
      return;
    }
    if (userEdited.current) {
      // 正在编辑这块：记录外部新值，不打断用户
      externalWhileEditing.current = pb.text;
      return;
    }
    setBlockText(el, pb.text, true);
    backendText.current = pb.text;
    userEdited.current = false;
    fragmentDraft.current=null;rememberFragmentDraft(el,null);
    externalWhileEditing.current = null;
  }, [pb.text, pb.block_type]);

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    []
  );

  const flush = useCallback(async function flushDraft(): Promise<void> {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
    if (composing.current) throw new Error(tr("请先完成输入法组字"));
    const el = ref.current;
    if (!el) return;

    // 用户没输入过，但外部来源改了同一块 → 采纳后端内容，绝不回写旧 DOM
    if (!userEdited.current && externalWhileEditing.current !== null) {
      const incoming = externalWhileEditing.current;
      externalWhileEditing.current = null;
      setBlockText(el, incoming, true);
      backendText.current = incoming;
      fragmentDraft.current=null;rememberFragmentDraft(el,null);
      return;
    }

    if (inFlight.current) await inFlight.current;
    const text = el.textContent ?? '';
    if (text === backendText.current) return;
    const local = stripAutoContinuation(text, pb.auto_contd);
    const context = fragmentDraft.current ?? fragmentContext(useEditorStore.getState().document?.blocks.find(b=>b.id===sourceId)?.text ?? pb.text, pb.source_start, pb.source_end);
    const submitted = context.prefix + local + context.suffix;
    const task = onChangeText(sourceId, submitted, baseRevision.current);
    inFlight.current = task;
    try {
      if (!await task) throw new Error(tr("文字尚未提交成功，请重试后再保存"));
      backendText.current = text;
      userEdited.current = (el.textContent ?? '') !== text;
      if (useEditorStore.getState().document?.blocks.find(b=>b.id===sourceId)?.text === submitted) baseRevision.current = useEditorStore.getState().document?.revision;

    } finally { inFlight.current = null; }
    if (userEdited.current) await flushDraft();
  }, [onChangeText, onError, sourceId, pb.auto_contd, pb.source_start, pb.source_end]);

  useEffect(() => registerPendingEditor(flush), [flush]);

  const schedule = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void flush().catch((e) => onError(String(e))), COMMIT_DEBOUNCE_MS);
  }, [flush]);

  const screenplayDoc = useEditorStore((s) => s.document);
  const settings = screenplayDoc?.settings;
  const index = screenplayDoc ? indexDocument(screenplayDoc.blocks) : undefined;
  const sceneBlock = index?.blocks.get(sourceId);
  const sceneNumber = sceneBlock?.scene_heading?.scene_number ?? String(index?.scenes.get(sourceId) ?? 0);
  const chinese = settings?.typography === 'chinese-reading';
  const production = settings?.typography === 'chinese-production';
  const general = settings?.typography === 'chinese-general';
  // 合并预设的场号已并入场景头正文（`1-日-办公室-内` / `1、办公室，日，内`），
  // 不再额外渲染标签，避免编号重复。
  const sceneLabel = settings?.show_scene_numbers && pb.block_type === 'scene_heading' && !production && !general ? (settings.typography === 'chinese-reading' ? `第${sceneNumber}场` : `SCENE ${sceneNumber}`) : null;
  const cjkBrackets = chinese || production || general;
  const customStyle = settings?.element_styles?.[pb.block_type];
  const resolvedStyle = customStyle ?? defaultElementStyle(pb.block_type, settings?.typography ?? 'hollywood');
  const gridSize = Math.max(settings?.font_size ?? 12, ...Object.values(settings?.element_styles ?? {}).map(s => s?.font_size ?? 0));
  const customCss = customStyle ? { fontFamily: resolvedStyle.font_family, fontSize: `${resolvedStyle.font_size}pt`, fontWeight: pb.block_type !== 'parenthetical' && resolvedStyle.bold ? 700 : 400, fontStyle: resolvedStyle.italic ? 'italic' : 'normal', color: paperInkColor(resolvedStyle.color) } : {};
  const indent = (production || general) ? 0 : chinese ? (['dialogue', 'parenthetical'].includes(pb.block_type) ? 1 : 0) : (INDENT_INCHES[pb.block_type] ?? 0);
  // 中式剧本：人物（情态）后接冒号，动作行补 △。都是装饰，不写入正文。
  const docIndex = screenplayDoc ? screenplayDoc.blocks.findIndex(b => b.id === sourceId) : -1;
  const nextType = docIndex >= 0 ? screenplayDoc!.blocks.slice(docIndex + 1).find(b => b.text.trim())?.block_type : undefined;
  const cueColon = production && nextType === 'dialogue' && (pb.block_type === 'character' || pb.block_type === 'parenthetical');
  const deltaPrefix = production && pb.block_type === 'action' && pb.text.trim() !== '' && !pb.text.trimStart().startsWith('△');

  const locationPrefix = completionInput?.match(/^((?:内景|外景|内|外|INT\.?|EXT\.?)[.。．\s]*)([^—\-\n]*)$/i);
  const suggestions = completionInput === null || !active ? [] : pb.block_type === 'character'
    ? [...new Set((screenplayDoc?.characters ?? []).map((c) => c.name))].filter((name) => name.toLowerCase().startsWith(completionInput.toLowerCase()) && name !== completionInput).slice(0, 6)
    : pb.block_type === 'scene_heading' && locationPrefix
    ? [...new Set((screenplayDoc?.locations ?? []).map((l) => l.name))].filter((name) => name.startsWith(locationPrefix[2]) && name !== locationPrefix[2]).slice(0, 6).map((name) => locationPrefix[1] + name)
    : [];
  const acceptCompletion = (value: string) => {
    const el = ref.current;
    if (!el || composing.current) return;
    captureDraft();
    el.textContent = value;
    setCaretAt(el, value.length);
    userEdited.current = true;
    setCompletionInput(null);
    schedule();
  };

  /**
   * 括注括号是 contentEditable=false 的装饰 span，位于正文 div 之外。
   * 整段选中（含括号）时选区端点不在正文里，浏览器无法跨 editing host 删除；
   * 这里把选区夹到正文偏移并显式删除。返回 true 表示已经处理。
   */
  const handleBracketSelection = (e: React.KeyboardEvent): boolean => {
    const el = ref.current;
    if (!el) return false;
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) return false;
    const range = sel.getRangeAt(0);
    if (typeof range.intersectsNode === 'function' && !range.intersectsNode(el)) return false;
    const mod = e.metaKey || e.ctrlKey;
    const isCut = mod && e.key.toLowerCase() === 'x';
    if (e.key !== 'Backspace' && e.key !== 'Delete' && !isCut) return false;
    if (el.contains(range.startContainer) && el.contains(range.endContainer)) return false;
    // 选区跨到了别的块：交给上层按块处理
    const startRow = closestRow(range.startContainer);
    const endRow = closestRow(range.endContainer);
    if (startRow && endRow && startRow !== endRow) return false;
    const len = (el.textContent ?? '').length;
    const from = Math.max(0, Math.min(offsetWithin(el, range.startContainer, range.startOffset, 0), len));
    const to = Math.max(from, Math.min(offsetWithin(el, range.endContainer, range.endOffset, len), len));
    e.preventDefault();
    const applyDeletion = () => {
      captureDraft();
      const current = el.textContent ?? '';
      const next = current.slice(0, from) + current.slice(to);
      el.textContent = next;
      setCaretAt(el, from);
      userEdited.current = true;
      setCompletionInput(next);
      schedule();
    };
    if (isCut) {
      // 剪切是事务：写剪贴板成功后才删，失败保留正文并提示。
      const cutText = range.toString();
      void (async () => {
        const ok = await writeClipboardText(cutText);
        if (!ok) {
          onError(tr('复制到剪贴板失败，选区未删除'));
          return;
        }
        applyDeletion();
      })();
      return true;
    }
    applyDeletion();
    return true;
  };

  /**
   * 多行粘贴不能交给 contenteditable：浏览器会插入 <div>/<br>，
   * 提交时 textContent 读不到换行，标题和正文会被拼成同一段。
   * 按 Enter 的语义把每一行拆成独立块，空行保留为空块。
   */
  const pasteMultiline = async (text: string) => {
    const el = ref.current;
    if (!el) return;
    if (inFlight.current) await inFlight.current;
    const sel = window.getSelection();
    if (sel && sel.rangeCount && !sel.isCollapsed && el.contains(sel.anchorNode) && el.contains(sel.focusNode)) sel.deleteFromDocument();
    const lines = text.replace(/\r\n?/g, '\n').split('\n');
    if (lines.length < 2) return;
    const local = stripAutoContinuation(el.textContent ?? '', pb.auto_contd);
    const context = fragmentDraft.current ?? fragmentContext(
      useEditorStore.getState().document?.blocks.find((b) => b.id === sourceId)?.text ?? pb.text,
      pb.source_start,
      pb.source_end,
    );
    const caret = context.prefix.length + Math.min(getCaretOffset(el) ?? local.length, local.length);
    const full = context.prefix + local + context.suffix;
    const first = full.slice(0, caret) + lines[0];
    // 原段落光标后的文字属于「最后一行」，不能留在第一行里
    const tailRest = full.slice(caret);
    if (!await onChangeText(sourceId, first)) return;
    userEdited.current = false;
    setCompletionInput(null);
    let afterId = sourceId;
    let prevType: BlockType = pb.block_type;
    const rest = lines.slice(1);
    for (const [i, line] of rest.entries()) {
      const type = nextTypeFor(prevType);
      await onInsertAfter(afterId, type, i === rest.length - 1 ? line + tailRest : line);
      const inserted = useEditorStore.getState().activeBlockId;
      if (!inserted || inserted === afterId) return;
      afterId = inserted;
      prevType = type;
    }
    const caretAtEnd = lines[lines.length - 1].length;
    let attempts = 0;
    const focusLast = () => {
      const target = document.querySelector<HTMLElement>(`[data-block-id="${cssEscape(afterId)}"] .screenplay-block`);
      if (!target) { if (++attempts < 60) requestAnimationFrame(focusLast); return; }
      target.focus();
      setCaretAt(target, caretAtEnd);
      target.scrollIntoView?.({ block: 'nearest' });
    };
    requestAnimationFrame(focusLast);
  };

  return (
    <div className={`block-row ${inline ? 'block-inline' : ''} ${customStyle ? 'custom-element-style' : ''} ${active ? 'active' : ''} ${selected ? 'block-selected' : ''}`} data-block-id={pb.block_id}
      role={inline ? undefined : 'option'}
      aria-selected={inline ? undefined : !!selected}
      aria-label={inline ? undefined : `${tr(BLOCK_LABELS[pb.block_type] ?? '正文')}：${pb.text.slice(0, 40)}`}
      style={inline
        ? { position: 'static', display: 'inline', width: 'auto', margin: 0, ...customCss }
        : { position: 'absolute', top: `${pb.start_line * gridSize * (settings?.line_height ?? 1)}pt`, width: '100%', margin: 0, ...customCss }}>
      {!inline && (
        <div
          className="block-gutter"
          role="button"
          tabIndex={-1}
          data-gutter-for={sourceId}
          aria-label={tr('选中此段落')}
          title={tr('选中此段落')}
          onMouseDown={(e) => onGutterDown(e, sourceId)}
          onClick={(e) => { if (e.detail === 0) onGutterKey(sourceId); }}
        />
      )}
      {pb.contd && pb.block_type !== 'character' && <div className="contd-marker">(CONT'D)</div>}
      {pb.more && <div className="more-marker">(MORE)</div>}

      {sceneLabel && <div className="scene-number-label" style={customCss} contentEditable={false}>{sceneLabel}</div>}
      <div className={pb.block_type === 'parenthetical' ? 'parenthetical-shell' : 'block-shell'}
        style={pb.block_type === 'parenthetical' ? { marginLeft: `${indent}in`, marginRight: chinese ? `${indent}in` : undefined, fontSize: !customStyle && chinese ? '85%' : undefined, textAlign: chinese ? 'center' : undefined } : undefined}>
      {pb.block_type === 'parenthetical' && !/^[（(]/.test(pb.text.trim()) && <span className="bracket-decoration" contentEditable={false}>{cjkBrackets ? '（' : '('}</span>}
      <div
        ref={ref}
        className={`screenplay-block block-${pb.block_type}${cueColon ? ' cue-colon' : ''}${deltaPrefix ? ' prefix-delta' : ''}`}
        data-block-type={pb.block_type}
        data-source-start={pb.source_start}
        data-source-end={pb.source_end}
        data-auto-contd={pb.auto_contd ? 'true' : undefined}
        data-bracketed={pb.text.trim().startsWith('（') || pb.text.trim().startsWith('(')}
        style={{ ...customCss, marginLeft: pb.block_type === 'parenthetical' ? 0 : `${indent}in`, marginRight: chinese && pb.block_type !== 'parenthetical' ? `${indent}in` : undefined }}
        contentEditable={!pb.contd}
        suppressContentEditableWarning
        spellCheck={false}
        onFocus={() => { onSelect(sourceId); setCompletionInput(ref.current?.textContent ?? ""); }}
        onClick={() => onSelect(sourceId)}
        onInput={() => {
          if (composing.current) return;
          captureDraft();
          userEdited.current = true;
          setCompletionInput(ref.current?.textContent ?? "");
          setCompletionIndex(0);
          schedule();
        }}
        onCompositionStart={() => {
          composing.current = true;
          setCompletionInput(null);
          if (timer.current !== null) {
            window.clearTimeout(timer.current);
            timer.current = null;
          }
          useEditorStore.getState().setComposing(true);
        }}
        onCompositionEnd={() => {
          composing.current = false;
          useEditorStore.getState().setComposing(false);
          captureDraft();
          userEdited.current = true;
          // ⚠️ 关键：compositionend 触发时，浏览器**可能还没**把确认后的
          // 中文字写入 DOM。同步读取会拿到拼音串并提交，导致中文永久丢失。
          // 因此等一帧（compositionend 之后的 input 事件）再提交。
          requestAnimationFrame(() => {
            void flush().catch((e) => onError(String(e)));
          });
        }}
        onPaste={(e) => {
          // 粘贴一律走纯文本，避免带入外部 HTML/CSS 破坏纸面
          e.preventDefault();
          const txt = e.clipboardData.getData('text/plain');
          if (/[\r\n]/.test(txt)) {
            void pasteMultiline(txt).catch((error) => onError(String(error)));
            return;
          }
          document.execCommand('insertText', false, txt);
        }}
        onBlur={() => { setCompletionInput(null); void flush().then(()=>{const el=ref.current;if(!el||composing.current||userEdited.current||document.activeElement===el)return;const current=useEditorStore.getState().document?.blocks.find(b=>b.id===sourceId);if(current){const display=(['scene_heading','character','parenthetical'].includes(current.block_type)||pb.source_start!==undefined)&&current.text===sceneBlock?.text?pb.text:pb.source_start!==undefined?backendText.current:current.text;setBlockText(el,display,false);backendText.current=display;externalWhileEditing.current=null;}}).catch((e) => onError(String(e))); }}
        onKeyDown={(e) => {
          if (!composing.current && !e.nativeEvent.isComposing && suggestions.length) {
            if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
              e.preventDefault(); e.stopPropagation();
              setCompletionIndex((index) => (index + (e.key === 'ArrowDown' ? 1 : suggestions.length - 1)) % suggestions.length);
              return;
            }
            if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); setCompletionInput(null); return; }
            if (e.key === 'Enter' && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
              e.preventDefault(); e.stopPropagation(); acceptCompletion(suggestions[Math.min(completionIndex, suggestions.length - 1)]); return;
            }
          }
          if(!pb.contd && handleBracketSelection(e)) return;
          if(!pb.contd)onKeyDown(e, sourceId);
        }}
      />
      {pb.block_type === 'parenthetical' && !/^[（(]/.test(pb.text.trim()) && <span className="bracket-decoration" contentEditable={false}>{cjkBrackets ? '）' : ')'}</span>}
      </div>
      {suggestions.length > 0 && <div className="writing-completions" role="listbox" aria-label={tr("写作补全")}>
        {suggestions.map((value, index) => <button key={value} role="option" aria-selected={index === Math.min(completionIndex, suggestions.length - 1)}
          onMouseDown={(e) => e.preventDefault()} onClick={() => acceptCompletion(value)}>{value}</button>)}
        <small>{tr("↑↓选择 · 回车补全 · Esc关闭")}</small>
      </div>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 光标工具
// ---------------------------------------------------------------------------

/**
 * 把文本写入 contentEditable 元素，**不破坏光标**。
 *
 * 为什么不能直接 `el.textContent = s`：
 * 设置 textContent 会销毁并重建所有子文本节点。WebKit 在节点被替换后
 * 会把 selection 塌缩到元素开头，表现为「每改一句光标就跳回第一行」。
 *
 * 这里采用最小化写入：
 * - 内容相同 → 什么都不做
 * - 只有一个文本节点 → 改 nodeValue（部分平台仍会移动光标，之后补一次归位）
 * - 结构不符 → 才整体重建
 * 写完后若元素是当前焦点，把光标恢复到合理位置。
 */
function setBlockText(el: HTMLElement, s: string, keepCaret: boolean) {
  const cur = el.textContent ?? '';
  if (document.activeElement !== el) {
    const cue = el.dataset.blockType === 'character' ? s.match(/^(.*?)([（(](?:内心独白|画外音|画外|旁白|离画|电话音|V\.O\.?|O\.S\.?|OFF|CONT['’]?D)(?:[,，]\s*CONT['’]?D)?[）)])$/i) : null;
    const prefix = s.match(/^(镜头切换[。.]|蒙太奇(?=[。.]))/);
    if (cue || prefix) {
      const first = document.createElement('span');
      const second = document.createElement('span');
      first.textContent = cue ? cue[1] : prefix![1];
      second.textContent = cue ? cue[2] : s.slice(prefix![1].length);
      first.className = cue ? 'cue-name' : 'technical-prefix';
      second.className = cue ? 'cue-extension' : 'technical-body';
      el.replaceChildren(first, second);
      return;
    }
  }
  if (cur === s) { if (document.activeElement !== el && el.querySelector(".cue-extension, .technical-prefix")) el.textContent = s; return; }

  const onlyChild = el.childNodes.length === 1 ? el.firstChild : null;
  if (onlyChild && onlyChild.nodeType === Node.TEXT_NODE) {
    (onlyChild as Text).nodeValue = s;
  } else {
    // 没有文本节点（首次渲染）或结构异常：整体写入
    el.textContent = s;
  }

  if (keepCaret && document.activeElement === el) {
    // 文本节点被替换时 WebKit 会把光标放到 0，这里按比例归位到末尾附近。
    // 只有在光标确实丢失（offset === 0 且原内容非空）时才干预，
    // 避免打断正常光标移动。
    const sel = window.getSelection();
    if (sel && sel.rangeCount > 0) {
      const r = sel.getRangeAt(0);
      if (el.contains(r.startContainer)) {
        const off = getCaretOffset(el);
        if (off === 0 && s.length > 0) {
          setCaretAt(el, s.length);
        }
      }
    }
  }
}

function setCaretAt(el: HTMLElement, offset: number) {
  const len = (el.textContent ?? '').length;
  const target = Math.max(0, Math.min(offset, len));
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let seen = 0;
  let last: Text | null = null;
  let n: Node | null;
  while ((n = walker.nextNode())) {
    last = n as Text;
    const l = n.textContent?.length ?? 0;
    if (seen + l >= target) {
      const range = document.createRange();
      range.setStart(n, target - seen);
      range.collapse(true);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
      return;
    }
    seen += l;
  }
  const range = document.createRange();
  range.selectNodeContents(el);
  range.collapse(false);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
}

function getCaretOffset(el: HTMLElement): number | null {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  if (!el.contains(range.startContainer)) return null;
  const pre = range.cloneRange();
  pre.selectNodeContents(el);
  pre.setEnd(range.startContainer, range.startOffset);
  return pre.toString().length;
}

/** 非折叠选区在可编辑元素内的起止偏移；选区不在元素内时返回 null。 */
function getSelectionOffsets(el: HTMLElement): { start: number; end: number } | null {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  if (!el.contains(range.startContainer) || !el.contains(range.endContainer)) return null;
  const pre = range.cloneRange();
  pre.selectNodeContents(el);
  pre.setEnd(range.startContainer, range.startOffset);
  const start = pre.toString().length;
  const post = range.cloneRange();
  post.selectNodeContents(el);
  post.setStart(range.endContainer, range.endOffset);
  const end = (el.textContent ?? '').length - post.toString().length;
  return { start, end: Math.max(start, end) };
}

/** 选区端点所在的块行（`.block-row[data-block-id]`）。 */
function closestRow(node: Node | null): HTMLElement | null {
  if (!node) return null;
  const el = node instanceof Element ? node : node.parentElement;
  return el?.closest<HTMLElement>('[data-block-id]') ?? null;
}

/**
 * 端点相对正文元素的字符偏移。端点落在正文之外（括注括号、块外）时，
 * 按文档顺序夹到 0（在正文之前）或末尾（在正文之后）。
 */
function offsetWithin(el: HTMLElement, node: Node, offset: number, fallback: number): number {
  if (!el.contains(node)) {
    // 端点可能落在正文的祖先（括注 shell）或块外：用边界点与正文首尾比较
    try {
      const probe = document.createRange();
      probe.setStart(node, offset);
      probe.collapse(true);
      const head = document.createRange();
      head.selectNodeContents(el);
      head.collapse(true);
      const tail = document.createRange();
      tail.selectNodeContents(el);
      tail.collapse(false);
      if (probe.compareBoundaryPoints(Range.START_TO_START, head) <= 0) return 0;
      if (probe.compareBoundaryPoints(Range.START_TO_START, tail) >= 0) return (el.textContent ?? '').length;
      return fallback;
    } catch {
      return fallback;
    }
  }
  const pre = document.createRange();
  pre.selectNodeContents(el);
  pre.setEnd(node, offset);
  return pre.toString().length;
}

function cssEscape(s: string): string {
  return typeof CSS !== 'undefined' && typeof CSS.escape === 'function'
    ? CSS.escape(s)
    : s.replace(/["\\]/g, '\\$&');
}