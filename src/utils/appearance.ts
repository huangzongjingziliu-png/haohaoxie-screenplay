import type { ElementStyle } from '@/ipc/types/ElementStyle';
export const ELEMENT_LABELS: Record<string, string> = { scene_heading: '场景标题 / 场号', action: '动作', character: '人物', parenthetical: '括注', dialogue: '对白', dual_dialogue: '双人对白', transition: '转场', shot: '镜头', note: '注释', section: '章节', synopsis: '概要', general: '普通文本' };
export type StylePreset = { name: string; typography: string; styles: Record<string, ElementStyle> };
export function validElementStyle(value: unknown): value is ElementStyle {
  if (!value || typeof value !== 'object') return false;
  const style = value as ElementStyle;
  return typeof style.font_family === 'string' && style.font_family.trim().length > 0 && style.font_family.length <= 200
    && /^[\p{L}\p{N} ,\-'"]+$/u.test(style.font_family) && Number.isFinite(style.font_size) && style.font_size >= 8 && style.font_size <= 36
    && typeof style.bold === 'boolean' && typeof style.italic === 'boolean' && /^#[0-9a-f]{6}$/i.test(style.color);
}
export function readPresets(): StylePreset[] {
  try { const value = JSON.parse(localStorage.getItem('muben.style-presets.v1') || '[]'); return Array.isArray(value) ? value.filter(p => p && typeof p.name === 'string' && p.styles && typeof p.styles === 'object' && !Array.isArray(p.styles) && Object.entries(p.styles).every(([key, style]) => key in ELEMENT_LABELS && validElementStyle(style)) && ['hollywood', 'chinese-reading'].includes(p.typography)) : []; } catch { return []; }
}
export function writePresets(presets: StylePreset[]) { localStorage.setItem('muben.style-presets.v1', JSON.stringify(presets)); }
export function readUiSize(): number { try { const n = Number(localStorage.getItem('muben.ui-size')); return n >= 13 && n <= 18 ? n : 14; } catch { return 14; } }
export function applyUiSize(size: number) { document.documentElement.style.setProperty('--ui-font-size', `${size}px`); }
export function saveUiSize(size: number) { localStorage.setItem('muben.ui-size', String(size)); applyUiSize(size); }
export function defaultElementStyle(type: string, typography: string): ElementStyle {
  const chinese = typography === 'chinese-reading';
  return { font_family: chinese ? (type === 'character' ? 'PingFang SC' : 'Songti SC') : 'Courier', font_size: chinese ? (type === 'parenthetical' ? 11.9 : 14) : 12, bold: ['scene_heading', 'character', 'transition'].includes(type), italic: false, color: '#000000' };
}
