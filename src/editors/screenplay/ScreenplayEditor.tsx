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

/** 元素类型菜单：跟随光标 / 吸附顶部横排 / 吸附左侧竖排。默认吸附左侧。 */
const TYPE_MENU_KEY = 'hhx.elementMenuMode';
type TypeMenuMode = 'float' | 'top' | 'left';
const readTypeMenuMode = (): TypeMenuMode => {
  try {
    const v = localStorage.getItem(TYPE_MENU_KEY);
    return v === 'float' || v === 'top' || v === 'left' ? v : 'left';
  } catch {
    return 'left';
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
  const [toolPosition, setToolPosition] = useState({ top: 0, left: 0, visible: true });
  const [shortcuts, setShortcuts] = useState(readShortcuts);
  const [menuMode, setMenuMode] = useState<TypeMenuMode>(readTypeMenuMode);
  const setMenuModePersisted = (mode: TypeMenuMode) => {
    setMenuMode(mode);
    try {
      localStorage.setItem(TYPE_MENU_KEY, mode);
    } catch {
      /* 隐私模式下忽略，仅当次生效 */
    }
  };
  const targetId = focusedId ? sourceBlockId(focusedId) : null;
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
      const row = workspace?.querySelector<HTMLElement>(`[data-block-id="${cssEscape(focusedId ?? '')}"]`);
      if (!workspace || !row) return;
      const bounds = workspace.getBoundingClientRect(), r = row.getBoundingClientRect();
      setToolPosition({ top: r.top - bounds.top, left: Math.max(4, r.left - bounds.left - 112), visible: r.bottom >= bounds.top && r.top < bounds.bottom });
    };
    update();
    window.addEventListener('resize', update);
    document.addEventListener('scroll', update, true);
    const observer = new ResizeObserver(update);
    if (workspaceRef.current) observer.observe(workspaceRef.current);
    return () => { window.removeEventListener('resize', update); document.removeEventListener('scroll', update, true); observer.disconnect(); };
  }, [focusedId, pagination, zoom]);
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
          && (el.contains(sel.getRangeAt(0).startContainer) || el.contains(sel.getRangeAt(0).endContainer));
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
    [deleteBlock, mergeBlock, requestFocus]
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
    <div className="paper-workspace" ref={workspaceRef} onFocusCapture={e => {
      const element = e.target as HTMLElement;
      if (element.matches('.screenplay-block') && element.isContentEditable !== false) setFocusedId(element.closest<HTMLElement>('[data-block-id]')?.dataset.blockId ?? null);
    }} onBlurCapture={() => {
      requestAnimationFrame(() => {
        if (!workspaceRef.current?.contains(document.activeElement)) setFocusedId(null);
      });
    }}>
      {menuMode === 'float' && selected && <div className="row-element-tools" aria-label={tr("当前元素工具")} style={{ top: toolPosition.top, left: toolPosition.left, visibility: toolPosition.visible ? 'visible' : 'hidden' }}>
        <div className="row-type-select"><span className="row-type-label" aria-hidden="true">{tr(BLOCK_LABELS[selected.block_type])}</span><svg className="row-type-arrow" aria-hidden="true" width="12" height="18" viewBox="0 0 12 18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d="M3 6 6 3 9 6 M3 12 6 15 9 12"/></svg><select aria-label={tr("当前元素类型")} title={tr("修改此段类型；Tab 也可依次切换。普通文本表示未指定剧本元素。")} value={selected.block_type} onChange={e => { const type = e.target.value as BlockType; void switchType(selected.id, type).catch(error => pushToast('error', String(error))); }}>{TYPE_ORDER.map((t) => <option key={t} value={t}>{tr(BLOCK_LABELS[t])} · {shortcutLabel(shortcuts[t])}</option>)}</select></div>
        <div className="row-element-actions">
          <button type="button" title={tr("吸附到顶部横排，位置固定")} onClick={() => setMenuModePersisted('top')}>{tr("顶部")}</button>
          <button type="button" title={tr("吸附到左侧竖排，位置固定")} onClick={() => setMenuModePersisted('left')}>{tr("左侧")}</button>
        </div>
      </div>}
      {menuMode !== 'float' && <div className={`element-dock ${menuMode}`} role="toolbar" aria-label={tr("元素类型")}>
        {menuMode === 'left' && <span className="element-dock-title">{tr("元素")}</span>}
        {TYPE_ORDER.map((t) => <button key={t} type="button" disabled={!dockBlock} className={dockBlock?.block_type === t ? 'on' : ''} title={`${tr(BLOCK_LABELS[t])} · ${shortcutLabel(shortcuts[t])}`} onMouseDown={(e) => e.preventDefault()} onClick={() => { if (dockBlock) void switchType(dockBlock.id, t).catch(error => pushToast('error', String(error))); }}>{tr(BLOCK_LABELS[t])}</button>)}
        <button type="button" className="element-dock-float" title={tr("恢复跟随光标，位置随当前段落移动")} onClick={() => setMenuModePersisted('float')}>{tr("跟随光标")}</button>
      </div>}
    <div className={`screenplay-editor typography-${paper?.typography ?? "hollywood"}`} ref={layoutRef} style={{ zoom: zoom === 0 ? Math.min(1, availableWidth / ((paper?.page_width_inch ?? 8.5) * 96 + 48)) : zoom, minWidth: `${(paper?.page_width_inch ?? 8.5) * 96 + 48}px`, fontFamily: paper?.font_family, fontSize: `${paper?.font_size ?? 12}pt`, lineHeight: `${Math.max(paper?.font_size ?? 12, ...Object.values(paper?.element_styles ?? {}).map(s => s?.font_size ?? 0)) * (paper?.line_height ?? 1)}pt` }}>
      <div className="pages-container">
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
            onStartWriting={screenplay.blocks.length === 0 ? startWriting : undefined}
            onSelect={setActiveBlock}
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
  onSelect,
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
  onSelect: (id: string) => void;
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
                onSelect={onSelect}
                onChangeText={onChangeText}
                onKeyDown={onKeyDown}
                onInsertAfter={onInsertAfter}
                onDelete={onDelete}
                onError={onError}
              />
            );
          }
          const top = Math.min(...row.map((p) => (blocksById.get(p.block_id) ?? p).start_line));
          return (
            <div
              key={`group:${head.group}`}
              className="block-row merged-dialogue-row"
              style={{ position: 'absolute', top: `${top * gridSize * lineHeight}pt`, width: '100%', margin: 0 }}
            >
              {row.map((p) => {
                const fresh = blocksById.get(p.block_id);
                if (!fresh) return null;
                return (
                  <BlockView
                    key={fresh.block_id}
                    pb={fresh}
                    inline
                    active={activeBlockId === sourceBlockId(fresh.block_id)}
                    onSelect={onSelect}
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
  inline,
  onSelect,
  onChangeText,
  onKeyDown,
  onInsertAfter,
  onDelete,
  onError,
}: {
  pb: PageBlock;
  active: boolean;
  /** 合并预设的行内片段：不参与绝对定位，由外层行容器排布。 */
  inline?: boolean;
  onSelect: (id: string) => void;
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
    <div className={`block-row ${inline ? 'block-inline' : ''} ${customStyle ? 'custom-element-style' : ''} ${active ? 'active' : ''}`} data-block-id={pb.block_id}
      style={inline
        ? { position: 'static', display: 'inline', width: 'auto', margin: 0, ...customCss }
        : { position: 'absolute', top: `${pb.start_line * gridSize * (settings?.line_height ?? 1)}pt`, width: '100%', margin: 0, ...customCss, zIndex: active ? 10 : undefined }}>
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

function cssEscape(s: string): string {
  return typeof CSS !== 'undefined' && typeof CSS.escape === 'function'
    ? CSS.escape(s)
    : s.replace(/["\\]/g, '\\$&');
}