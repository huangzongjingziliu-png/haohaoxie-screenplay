/** 四种排版预设的唯一显示名与顺序。改名只动这里。 */
export const TYPOGRAPHY_PRESETS = [
  { id: 'hollywood', label: '好莱坞美式剧本' },
  { id: 'chinese-reading', label: '好莱坞中文剧本' },
  { id: 'chinese-general', label: '中文通用剧本' },
  { id: 'chinese-production', label: '中式剧本（△ 标记）' },
] as const;

export type TypographyPresetId = (typeof TYPOGRAPHY_PRESETS)[number]['id'];

export const PRESET_IDS: readonly string[] = TYPOGRAPHY_PRESETS.map(p => p.id);

export function presetLabel(id: string | undefined): string {
  return TYPOGRAPHY_PRESETS.find(p => p.id === id)?.label ?? TYPOGRAPHY_PRESETS[0].label;
}
