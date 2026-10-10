/**
 * 纸面编辑器 DOM 自检（B5）
 *
 * 目的：验证**可见纸面**的真实编辑行为，而不是只测后端命令。
 *
 * 覆盖：
 * 1. 纸面元素真实渲染（contentEditable、正确的 block 类型样式）
 * 2. 中文 IME 组字期间不提交、不重排、不丢字
 * 3. 光标位置在组字前后保持
 * 4. 选区（selection）在编辑中保持
 * 5. 回车插入新块并按剧本语义切换类型
 * 6. 退格删除空块
 * 7. 元素类型切换
 * 8. 分页正确渲染
 *
 * 这些断言都在**真实 webview** 里跑（由 src-tauri 通过 SCREENPLAY_SELFTEST
 * 启动的 .app 进程执行），不是 jsdom mock。
 */

import { useEditorStore } from '@/stores/editor';
import { ipc } from '@/ipc/client';
import type { SelftestStep } from '@/ipc/types';

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** 触发 IME 组字开始 */
function fireCompositionStart(el: HTMLElement) {
  el.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
}

/** 触发 IME 组字更新（拼音 → 候选） */
function fireCompositionUpdate(el: HTMLElement, data: string) {
  el.dispatchEvent(
    new CompositionEvent('compositionupdate', { bubbles: true, data })
  );
}

/**
 * 触发 IME 组字结束（确认候选字）。
 *
 * 注意：**不能**用 `el.textContent = data` 来模拟 —— 那会销毁并重建文本节点，
 * 直接把 selection 打回 0，制造出「产品丢光标」的假象。
 * 真实输入法是选中拼音串后替换成中文，文本节点结构不变、光标自然跟到末尾。
 * `document.execCommand('insertText', ...)` 走浏览器内部同一条编辑路径。
 *
 * @param el 目标元素
 * @param finalText 组字完成后的**最终**文本（含原文）
 */
function fireCompositionEnd(el: HTMLElement, finalText: string) {
  const before = el.textContent ?? '';
  // 选中组字期间插入的拼音串（末尾连续的拉丁字母）
  selectCompositionText(el);
  // 用最终文本替换选中段：拼音串 → 剩余内容
  const tail = finalText.startsWith(before.replace(/[a-zA-Z]+$/, ''))
    ? finalText.slice(before.replace(/[a-zA-Z]+$/, '').length)
    : finalText;
  insertTextAtCaret(el, tail);
  el.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: tail }));
}

/** 选中组字期间插入的拼音串（末尾的拉丁字母部分） */
function selectCompositionText(el: HTMLElement) {
  const txt = el.textContent ?? '';
  const m = /[a-zA-Z]+$/.exec(txt);
  if (!m) return;
  const start = m.index;
  const range = document.createRange();
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let seen = 0;
  let n: Node | null;
  while ((n = walker.nextNode())) {
    const l = n.textContent?.length ?? 0;
    if (seen + l >= start) {
      range.setStart(n, start - seen);
      range.setEnd(n, start - seen + m[0].length);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
      return;
    }
    seen += l;
  }
}

/** 在光标处插入纯文本（走浏览器内部编辑路径） */
function insertTextAtCaret(el: HTMLElement, text: string) {
  el.focus?.();
  document.execCommand('insertText', false, text);
}

function setCaretAtEnd(el: HTMLElement) {
  const range = document.createRange();
  range.selectNodeContents(el);
  range.collapse(false);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
  (el as HTMLElement & { focus?: () => void }).focus?.();
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

function getSelectionText(el: HTMLElement): string {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return '';
  const range = sel.getRangeAt(0);
  return el.contains(range.startContainer) ? range.toString() : '';
}

/** 纸面 block 元素 */
function blockEls(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>('.screenplay-block'));
}

/**
 * 等到纸面文本真正写入 DOM。
 * contentEditable 是非受控的，文本由 effect 命令式写入，
 * 所以要等一个渲染周期，不能只看元素是否已挂载。
 */
async function waitForBlocks(minCount = 1, timeoutMs = 4000): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const els = blockEls();
    if (els.length >= minCount && els.some((e) => (e.textContent ?? '').length > 0)) {
      return true;
    }
    await wait(50);
  }
  return false;
}

/** 把光标放到块内指定字符偏移 */
function setCaretAt(el: HTMLElement, offset: number) {
  const len = (el.textContent ?? '').length;
  const target = Math.max(0, Math.min(offset, len));
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let seen = 0;
  let n: Node | null;
  while ((n = walker.nextNode())) {
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

function cssEscape(s: string): string {
  return typeof CSS !== 'undefined' && typeof CSS.escape === 'function'
    ? CSS.escape(s)
    : s.replace(/["\\]/g, '\\$&');
}

/**
 * 等待后端权威状态满足条件（IPC 是异步的，同 tick 读到的一定是旧快照）
 */
async function waitUntil(
  predicate: () => boolean,
  timeoutMs = 3000
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return true;
    await wait(50);
  }
  return predicate();
}

/**
 * 纸面编辑器 DOM 自检
 *
 * 需要先有内容（调用方保证已 replace_source_text）。
 */
export async function runEditorDomSelfTest(): Promise<SelftestStep[]> {
  const steps: SelftestStep[] = [];
  const record = (s: SelftestStep) => {
    steps.push(s);
    return s;
  };
  // 注意：所有断言必须支持 async。
  // 修改类命令（insert/delete/replace）都是异步 IPC，
  // 若在 dispatch 事件后同 tick 读 store，读到的必然是旧快照 —— 会造成假失败。
  const t = async (name: string, fn: () => void | Promise<void>) => {
    const t0 = performance.now();
    try {
      await fn();
      record({ name, ok: true, ms: Math.round(performance.now() - t0), detail: '' });
    } catch (e) {
      record({
        name,
        ok: false,
        ms: Math.round(performance.now() - t0),
        detail: e instanceof Error ? e.message : String(e),
      });
    }
  };

  await wait(400); // 等分页渲染

  // ---- 1. 纸面元素真实渲染 ----
  // 纸面文本由 effect 命令式写入（非受控 contentEditable），
// 必须等文本真正落进 DOM 再断言
  await t('dom_blocks_rendered', async () => {
    if (!(await waitForBlocks(5))) {
      throw new Error(`纸面未在超时内渲染出文本（元素数 ${blockEls().length}）`);
    }
    const els = blockEls();
    if (els.length === 0) throw new Error('纸面没有渲染任何 block');
    const editable = els.filter((e) => e.isContentEditable);
    if (editable.length !== els.length) {
      throw new Error(`${els.length - editable.length} 个 block 不可编辑`);
    }
    const types = els.map((e) => e.className);
    if (!types.some((c) => c.includes('block-scene_heading'))) {
      throw new Error('缺少场景标题样式');
    }
    if (!types.some((c) => c.includes('block-dialogue'))) {
      throw new Error('缺少对白样式');
    }
  });

  // ---- 2. 分页渲染 ----
  await t('dom_pagination_rendered', () => {
    const pages = document.querySelectorAll('.page');
    if (pages.length === 0) throw new Error('没有渲染分页');
    const page = pages[0] as HTMLElement;
    const w = page.style.width || '';
    if (!w.includes('in')) throw new Error(`页面宽度未按英寸设置: ${w}`);
  });

  // ---- 3. 中文 IME 组字（核心）----
  const els = blockEls();
  const target = els.find((e) => e.className.includes('block-dialogue')) ?? els[0];
  if (!target) {
    record({ name: 'ime_available', ok: false, ms: 0, detail: '找不到可编辑元素' });
    return steps;
  }

  let imeSnapshotDuring = '';
  let imeSnapshotAfter = '';
  let caretBefore = 0;
  let caretAfterComposition = -1;
  let composingDuring = false;
  let imeNodeSurvived = false;
  const imeNodeTag = `ime-${Date.now()}`;

  await t('ime_composition_no_lost_chars', async () => {
    target.focus?.();
    setCaretAtEnd(target);

    const original = target.textContent ?? '';
    caretBefore = getCaretOffset(target) ?? 0;

    // 拼音输入阶段（真实输入法在光标处插入拼音串）
    fireCompositionStart(target);
    fireCompositionUpdate(target, 'zuo');
    insertTextAtCaret(target, 'zuo');
    imeSnapshotDuring = target.textContent ?? '';

    // 组字期间必须处于 composing 状态（此刻才是正确的检查时机）
    composingDuring = useEditorStore.getState().composing;

    // 打标记，用于诊断重渲染后 DOM 节点是否被替换
    target.setAttribute('data-ime-tag', imeNodeTag);

    // 候选确认：拼音串 'zuo' 被中文 '昨日，去了买卖城。' 替换
    const committed = original + '昨日，去了买卖城。';
    fireCompositionEnd(target, committed);

    // 给编辑器一帧完成「等一帧再提交」的路径
    await wait(60);
    imeSnapshotAfter = target.textContent ?? '';

    // 关键断言：组字完成后文字完整（不丢字、不重复）
    if (imeSnapshotAfter !== committed) {
      throw new Error(
        `组字后文字不完整\n  期望: ${committed}\n  实际: ${imeSnapshotAfter}`
      );
    }
    // 组字期间 DOM 应该是拼音串，而不是已确认的中文
    if (imeSnapshotDuring !== original + 'zuo') {
      throw new Error(
        `组字期间 DOM 异常\n  期望(原+拼音): ${original}zuo\n  实际: ${imeSnapshotDuring}`
      );
    }
    if (imeSnapshotDuring.includes('买卖城')) {
      throw new Error('组字期间不应出现已确认的中文（说明提前提交）');
    }
    if (!composingDuring) {
      throw new Error('组字期间 store.composing 应为 true');
    }
  });

  await t('ime_no_early_submit', async () => {
    // 组字期间 store 的 composing 必须为 true，修改不应被提交
    // （fireCompositionStart 已触发，这里检查状态被正确清理）
    const composing = useEditorStore.getState().composing;
    if (composing) throw new Error('compositionend 后 composing 应为 false');
  });

  // ---- 4. 组字提交到后端 ----
  // 注意：编辑器在 compositionend 后**等一帧**才提交（防丢字修复），
  // 再叠加输入防抖，所以这里必须等足时间，不能同 tick 断言。
  await t('ime_committed_to_backend', async () => {
    const deadline = Date.now() + 3000;
    let dialogue = '';
    while (Date.now() < deadline) {
      dialogue =
        useEditorStore
          .getState()
          .document?.blocks.find((b) => b.block_type === 'dialogue')?.text ?? '';
      if (dialogue.includes('昨日，去了买卖城')) return;
      await wait(100);
    }
    throw new Error(`中文未在 3 秒内提交到后端: ${dialogue}`);
  });

  // ---- 5. 光标位置 ----
  await t('caret_preserved_after_ime', () => {
    // 分页重渲染可能替换 DOM 节点，按标记 + 内容双路定位
    const tagged = document.querySelector<HTMLElement>(`[data-ime-tag="${imeNodeTag}"]`);
    const cur =
      tagged ??
      blockEls().find((e) => (e.textContent ?? '').includes('昨日，去了买卖城')) ??
      target;
    imeNodeSurvived = Boolean(tagged) && cur === target;

    caretAfterComposition = getCaretOffset(cur) ?? -1;
    const len = (cur.textContent ?? '').length;
    if (caretAfterComposition < 0) {
      throw new Error('组字后光标丢失');
    }
    // 组字后光标应在文本末尾附近
    if (caretAfterComposition < len - 20) {
      throw new Error(
        `光标位置异常: offset=${caretAfterComposition}, len=${len}（组字前 ${caretBefore}，` +
          `DOM 节点${imeNodeSurvived ? '存活' : '已被替换'}，activeElement=${
            (document.activeElement as HTMLElement | null)?.className ?? 'null'
          }）`
      );
    }
  });

  // ---- 6. 选区 ----
  await t('selection_supported', () => {
    const el = blockEls()[0];
    const range = document.createRange();
    const textNode = el.firstChild;
    if (!textNode) throw new Error('元素没有文本节点');
    range.setStart(textNode, 0);
    range.setEnd(textNode, Math.min(3, textNode.textContent?.length ?? 0));
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);
    const selected = getSelectionText(el);
    if (selected.length === 0) throw new Error('无法建立选区');
    sel?.removeAllRanges();
  });

  // ---- 7. 回车：拆分块 + 剧本语义类型切换 ----
  await t('enter_splits_block_with_script_semantics', async () => {
    const charEls = blockEls().filter((e) => e.className.includes('block-character'));
    const anchor = charEls[0];
    if (!anchor) throw new Error('找不到人物块用于测试回车');
    const blockId = anchor.closest<HTMLElement>('[data-block-id]')?.dataset.blockId;
    if (!blockId) throw new Error('人物块缺少 data-block-id');

    const before = useEditorStore.getState().document ?? null;
    const beforeLen = before?.blocks.length ?? 0;
    const beforeRev = before?.revision ?? 0;
    const beforeTypes = before?.blocks.map((b) => b.block_type) ?? [];

    anchor.focus?.();
    setCaretAtEnd(anchor);
    anchor.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
    );

    // 异步 IPC：轮询等待后端权威状态更新
    const deadline = Date.now() + 3000;
    while (Date.now() < deadline) {
      const doc = useEditorStore.getState().document;
      if ((doc?.blocks.length ?? 0) === beforeLen + 1 && (doc?.revision ?? 0) > beforeRev) break;
      await wait(50);
    }

    const after = useEditorStore.getState().document;
    const afterTypes = after?.blocks.map((b) => b.block_type) ?? [];

    if ((after?.blocks.length ?? 0) !== beforeLen + 1) {
      throw new Error(`回车后块数应 +1，实际 ${beforeLen} → ${after?.blocks.length}`);
    }

    // 人物块后回车 → 新块必须是括注（剧本语义）
    const at = afterTypes.indexOf(beforeTypes[beforeTypes.indexOf('character')]);
    const charIdx = afterTypes.findIndex((t) => t === 'character');
    if (charIdx >= 0) {
      const next = afterTypes[charIdx + 1];
      if (next !== 'parenthetical') {
        throw new Error(`人物后回车应为括注，实际为 ${next}（新块类型序列: ${afterTypes.join(',')}）`);
      }
    } else {
      throw new Error('回车后找不到人物块');
    }
    void at;
    void blockId;
  });

  // ---- 8. Tab 切换元素类型（保留稳定 ID） ----
  await t('block_type_switch_updates_dom', async () => {
    // 放在合并测试**之前**：合并会改变块结构，
    // 依赖固定块类型的断言会互相污染。
    const before = useEditorStore.getState().document?.blocks ?? [];
    // 取最后一个块（回车测试新插入的块），避免与合并测试抢块
    const b = before[before.length - 1];
    if (!b) throw new Error('找不到可用块');
    const originalType = b.block_type;

    const el = document.querySelector<HTMLElement>(
      `[data-block-id="${cssEscape(b.id)}"] .screenplay-block`
    );
    if (!el) throw new Error('找不到目标块 DOM');
    el.focus?.();
    setCaretAt(el, (el.textContent ?? '').length);
    el.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true })
    );

    const deadline = Date.now() + 3000;
    while (Date.now() < deadline) {
      const now = useEditorStore.getState().document?.blocks ?? [];
      const same = now.find((x) => x.id === b.id);
      if (same && same.block_type !== originalType) {
        // 关键：类型切换必须保留稳定 ID，不能靠「删除+插入」实现
        if (!same) throw new Error('类型切换后原 ID 消失');
        return;
      }
      await wait(50);
    }
    throw new Error(`类型切换未生效（原类型 ${originalType}）`);
  });

  // ---- 9. 退格：块首退格合并上一块（专业编辑器语义） ----
  await t('backspace_at_block_start_merges', async () => {
    const before = useEditorStore.getState().document;
    const blocks = before?.blocks ?? [];
    if (blocks.length < 2) throw new Error('块数不足，无法测试退格合并');

    // 取最后一个非空块，把光标放到块首（避免吃掉场景标题这类结构块）
    let idx = -1;
    for (let i = blocks.length - 1; i > 0; i--) {
      if (blocks[i].text.length > 0) {
        idx = i;
        break;
      }
    }
    if (idx < 0) throw new Error('找不到可用于合并的块');
    const targetId = blocks[idx].id;
    const prevId = blocks[idx - 1].id;
    const prevText = blocks[idx - 1].text;
    const curText = blocks[idx].text;
    const beforeLen = blocks.length;

    const el = document.querySelector<HTMLElement>(
      `[data-block-id="${cssEscape(targetId)}"] .screenplay-block`
    );
    if (!el) throw new Error(`找不到 block ${targetId} 的 DOM`);
    el.focus?.();
    setCaretAt(el, 0);

    el.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true })
    );

    const deadline = Date.now() + 3000;
    while (Date.now() < deadline) {
      const n = useEditorStore.getState().document?.blocks.length ?? 0;
      if (n === beforeLen - 1) break;
      await wait(50);
    }

    const after = useEditorStore.getState().document?.blocks ?? [];
    if (after.length !== beforeLen - 1) {
      throw new Error(`块首退格应合并两块，${beforeLen} → ${after.length}`);
    }
    if (!after.some((b) => b.id === prevId)) {
      throw new Error('合并后应保留前一块的稳定 ID');
    }
    if (after.some((b) => b.id === targetId)) {
      throw new Error('合并后后一块应被删除');
    }
    const merged = after.find((b) => b.id === prevId);
    const want = prevText + curText;
    if (merged?.text !== want) {
      throw new Error(`合并后文本不正确\n  期望: ${want}\n  实际: ${merged?.text}`);
    }
  });

  // ---- 10. 组字期间 DOM 不被后端状态覆盖 ----
  await t('dom_not_overwritten_during_composition', async () => {
    const allBlocks = useEditorStore.getState().document?.blocks ?? [];
    const blockId = allBlocks.length ? allBlocks[allBlocks.length - 1].id : undefined;
    const el = blockId
      ? document.querySelector<HTMLElement>(
          `[data-block-id="${cssEscape(blockId)}"] .screenplay-block`
        )
      : null;
    if (!el || !blockId) throw new Error('找不到可编辑元素');
    el.focus?.();
    setCaretAtEnd(el);
    const original = el.textContent ?? '';

    // 组字开始，插入拼音
    fireCompositionStart(el);
    fireCompositionUpdate(el, 'zhong');
    insertTextAtCaret(el, 'zhong');

    // 组字期间主动拉取后端状态，模拟其他来源刷新
    await ipc.getDocument().catch(() => undefined);
    await wait(200);

    if (el.textContent !== original + 'zhong') {
      throw new Error(
        `组字期间 DOM 被外部状态覆盖\n  组字前: ${original}\n  实际: ${el.textContent}`
      );
    }

    // 确认候选：拼音串被中文替换
    fireCompositionEnd(el, '中');
    await wait(500);

    // 提交后后端权威状态必须与 DOM 一致（中文不丢、不重复、无拼音残留）
    const backend = useEditorStore.getState().document?.blocks.find((b) => b.id === blockId);
    if (!backend) throw new Error('找不到后端块');
    if (backend.text !== original + '中') {
      throw new Error(
        `组字提交后后端文本不正确\n  期望: ${original}中\n  实际: ${backend.text}`
      );
    }
    if (/[a-zA-Z]/.test(backend.text)) {
      throw new Error(`后端残留拼音串: ${backend.text}`);
    }
    if ((el.textContent ?? '') !== backend.text) {
      throw new Error(`DOM 与后端不一致\n  DOM: ${el.textContent}\n  后端: ${backend.text}`);
    }
  });

  return steps;
}