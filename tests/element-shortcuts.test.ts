import { expect, it } from 'vitest';
import { DEFAULT_SHORTCUTS, readShortcuts, saveShortcuts, shortcutError, keyCombo } from '@/utils/elementShortcuts';
it('自定义快捷键保存并恢复，拒绝重复及系统截图组合', () => {
  localStorage.removeItem('muben.element-shortcuts.v1');
  expect(readShortcuts()).toEqual(DEFAULT_SHORTCUTS);
  const custom = {...DEFAULT_SHORTCUTS, action:'Mod+Alt+2'};
  saveShortcuts(custom); expect(readShortcuts()).toEqual(custom);
  expect(shortcutError({...custom,character:'Mod+Alt+2'})).toContain('重复');
  expect(shortcutError({...custom,character:'Mod+Shift+3'})).toContain('截图');
  expect(shortcutError({...custom,character:'Mod+s'})).toContain('数字');
  localStorage.removeItem('muben.element-shortcuts.v1');
});
it('Option 数字通过物理键识别，普通输入不成为快捷键', () => {
  expect(keyCombo({key:'™',code:'Digit2',metaKey:true,ctrlKey:false,altKey:true,shiftKey:false})).toBe('Mod+Alt+2');
  expect(keyCombo({key:'2',code:'Digit2',metaKey:false,ctrlKey:false,altKey:false,shiftKey:false})).toBe('');
});
