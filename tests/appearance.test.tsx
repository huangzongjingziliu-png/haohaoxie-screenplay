import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { render, fireEvent, waitFor, cleanup } from '@testing-library/react';
import SettingsPanel from '@/components/SettingsPanel';
import Toolbar from '@/components/Toolbar';
import { useEditorStore } from '@/stores/editor';
import { mockBackend } from './setup';
import { readPresets, readUiSize } from '@/utils/appearance';
beforeEach(() => {
  localStorage.clear(); mockBackend.reset();
  useEditorStore.setState({ document: { id: 'qa', metadata: { title: '测试', author: '' }, settings: { typography: 'hollywood', element_styles: {}, style_preset_name: '', font_size: 12, template: 'a4', margin_top_inch: 1, margin_bottom_inch: 1, margin_left_inch: 1.5, margin_right_inch: 1 } } as never });
  mockBackend.on('autosave_config', () => ({ enabled: true, saveIntervalSecs: 30, versionIntervalSecs: 300 }));
  mockBackend.on('update_settings', () => ({})); mockBackend.on('configure_autosave', () => ({}));
});
afterEach(cleanup);
it('更多菜单收纳设置并能以 Escape 关闭', () => {
  const settings = vi.fn(); const view = render(<Toolbar onImport={() => undefined} onSettings={settings}/>);
  expect(view.queryByText('设置…')).toBeNull();
  fireEvent.click(view.getByText('更多 ▾')); fireEvent.click(view.getByText('设置…')); expect(settings).toHaveBeenCalledOnce();
  fireEvent.click(view.getByText('更多 ▾')); fireEvent.keyDown(document, { key: 'Escape' }); expect(view.queryByText('设置…')).toBeNull();
});
it('自定义对白样式和命名预设可以跨面板复用，界面字号独立保存', async () => {
  let view = render(<SettingsPanel onClose={() => undefined}/>);
  fireEvent.change(view.getByLabelText('样式元素'), { target: { value: 'dialogue' } });
  fireEvent.change(view.getByLabelText('元素字号'), { target: { value: '18' } });
  fireEvent.change(view.getByLabelText('元素字体'), { target: { value: 'PingFang SC' } });
  fireEvent.click(view.getByText('斜体')); fireEvent.click(view.getByLabelText('文字颜色：蓝色'));
  fireEvent.change(view.getByLabelText('预设名称'), { target: { value: '我的模板' } }); fireEvent.click(view.getByText('保存为命名预设'));
  expect(readPresets()[0].styles.dialogue).toMatchObject({ font_size: 18, italic: true, color: '#2355aa' });
  view.unmount(); view = render(<SettingsPanel onClose={() => undefined}/>);
  fireEvent.change(view.getByLabelText('样式预设'), { target: { value: '我的模板' } });
  fireEvent.change(view.getByLabelText('界面字号（标签、按钮，不影响纸面和导出）'), { target: { value: '16' } });
  fireEvent.click(view.getByText('保存设置'));
  await waitFor(() => expect(mockBackend.callsTo('update_settings')).toHaveLength(1));
  expect(mockBackend.callsTo('update_settings')[0].args?.patch).toMatchObject({ stylePresetName: '我的模板', elementStyles: { dialogue: { font_size: 18, italic: true } } });
  await waitFor(() => expect(readUiSize()).toBe(16));
});

it('损坏预设记录被跳过，不影响设置界面打开', () => {
  localStorage.setItem('muben.style-presets.v1', JSON.stringify([null, { name: '损坏', typography: 'hollywood', styles: { action: { font_size: 300 } } }]));
  expect(readPresets()).toEqual([]);
  expect(() => render(<SettingsPanel onClose={() => undefined}/>)).not.toThrow();
});
