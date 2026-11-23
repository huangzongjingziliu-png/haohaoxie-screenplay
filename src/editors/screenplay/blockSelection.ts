import type { BlockType } from '@/ipc/types';

/** 块级多选与块剪贴板的纯逻辑（与 DOM 无关，便于测试）。 */

export interface BlockLike {
  id: string;
  block_type: BlockType;
  text: string;
}

export interface CopiedBlock {
  type: BlockType;
  text: string;
}

/** 按文档顺序返回 anchor → focus 的连续块 id；任一端点不存在时返回空数组。 */
export function orderedBlockRange(ids: readonly string[], anchor: string, focus: string): string[] {
  const a = ids.indexOf(anchor);
  const f = ids.indexOf(focus);
  if (a < 0 || f < 0) return [];
  const [lo, hi] = a <= f ? [a, f] : [f, a];
  return ids.slice(lo, hi + 1);
}

/** 当前块所在场（场景标题起，到下一个场景标题前）；不在任何场内返回 null。 */
export function sceneBlockIds(blocks: readonly BlockLike[], activeId: string | null): string[] | null {
  if (!blocks.length || !activeId) return null;
  const idx = blocks.findIndex((b) => b.id === activeId);
  if (idx < 0) return null;
  let start = -1;
  for (let i = idx; i >= 0; i -= 1) {
    if (blocks[i].block_type === 'scene_heading') {
      start = i;
      break;
    }
  }
  if (start < 0) return null;
  let end = blocks.length;
  for (let i = start + 1; i < blocks.length; i += 1) {
    if (blocks[i].block_type === 'scene_heading') {
      end = i;
      break;
    }
  }
  return blocks.slice(start, end).map((b) => b.id);
}

/** 块复制到系统剪贴板的纯文本：每块一行。 */
export function blocksToText(blocks: readonly { text: string }[]): string {
  return blocks.map((b) => b.text).join('\n');
}

interface InAppClipboard {
  text: string;
  blocks: CopiedBlock[];
}

let inApp: InAppClipboard | null = null;

/**
 * 记录应用内块剪贴板。系统剪贴板仍写入纯文本，
 * 粘贴时只有文本与记录完全一致才按块还原类型。
 */
export function rememberBlockClipboard(text: string, blocks: readonly CopiedBlock[]): void {
  inApp = blocks.length ? { text, blocks: blocks.map((b) => ({ ...b })) } : null;
}

export function readBlockClipboard(text: string): CopiedBlock[] | null {
  if (!inApp || inApp.text !== text) return null;
  return inApp.blocks.map((b) => ({ ...b }));
}

export function clearBlockClipboard(): void {
  inApp = null;
}

/**
 * 浮动元素菜单的纵向位置。
 *
 * 默认与当前段落顶部对齐；下方空间不足时向上翻转，贴着段落下沿展开，
 * 再不够就限制在容器内（配合 CSS max-height + overflow 滚动），
 * 避免被窗口底部裁掉。
 */
export function floatingMenuTop(input: {
  rowTop: number;
  rowBottom: number;
  containerHeight: number;
  menuHeight: number;
  margin?: number;
}): { top: number; maxHeight: number } {
  const margin = input.margin ?? 4;
  const limit = Math.max(margin, input.containerHeight - margin);
  if (input.menuHeight <= 0) return { top: Math.max(margin, input.rowTop), maxHeight: limit };
  if (input.rowTop + input.menuHeight <= limit) {
    return { top: Math.max(margin, input.rowTop), maxHeight: limit - Math.max(margin, input.rowTop) };
  }
  const flipped = input.rowBottom - input.menuHeight;
  const top = Math.max(margin, flipped);
  return { top, maxHeight: limit - top };
}

/** 块选区的无障碍播报文本。 */
export function blockRangeAnnouncement(count: number): string {
  if (count <= 0) return '';
  return `已选中 ${count} 个段落，按 Esc 取消选择`;
}

