import type { BlockType } from '@/ipc/types';
export const ELEMENT_TYPES: BlockType[] = ['scene_heading','action','character','parenthetical','dialogue','transition','shot','note','section','synopsis','general'];
export type Shortcuts = Record<string, string>;
export const DEFAULT_SHORTCUTS: Shortcuts = Object.fromEntries(ELEMENT_TYPES.map((t,i) => [t, i < 10 ? `Mod+${(i+1)%10}` : 'Mod+Alt+1']));
const KEY = 'muben.element-shortcuts.v1';
export function keyCombo(e: Pick<KeyboardEvent,'key'|'code'|'metaKey'|'ctrlKey'|'altKey'|'shiftKey'>): string {
  if (!e.metaKey && !e.ctrlKey) return '';
  const key = /^Digit\d$/.test(e.code) ? e.code.slice(5) : e.key.toLowerCase();
  if (['meta','control','alt','shift'].includes(key)) return '';
  return ['Mod', ...(e.altKey ? ['Alt'] : []), ...(e.shiftKey ? ['Shift'] : []), key].join('+');
}
export function shortcutError(values: Shortcuts): string {
  const assigned = Object.values(values).filter(Boolean);
  if (assigned.some(k => !/^Mod\+(Alt\+)?(Shift\+)?[0-9]$/.test(k))) return '请使用 Command/Ctrl + 数字，可加 Option/Alt 或 Shift。';
  if (new Set(assigned).size !== assigned.length) return '快捷键重复，请为不同元素选择不同组合。';
  if (assigned.some(k => /^Mod\+Shift\+[345]$/.test(k))) return '此组合是 macOS 截图快捷键，请换一个。';
  return '';
}
export function readShortcuts(): Shortcuts {
  try { const v = JSON.parse(localStorage.getItem(KEY) ?? 'null'); const next = Object.fromEntries(ELEMENT_TYPES.map(t => [t, typeof v?.[t] === 'string' ? v[t] : DEFAULT_SHORTCUTS[t]])); return shortcutError(next) ? {...DEFAULT_SHORTCUTS} : next; } catch { return {...DEFAULT_SHORTCUTS}; }
}
export function saveShortcuts(v: Shortcuts) { localStorage.setItem(KEY, JSON.stringify(v)); window.dispatchEvent(new Event('muben-shortcuts-changed')); }
export function shortcutLabel(v: string) { return v.replace('Mod+', navigator.platform.includes('Mac') ? '⌘' : 'Ctrl+').replace('Alt+', '⌥').replace('Shift+', '⇧') || '未设置'; }
