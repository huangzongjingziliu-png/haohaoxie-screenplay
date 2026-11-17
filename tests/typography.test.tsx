import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import ScreenplayEditor from '@/editors/screenplay/ScreenplayEditor';
import SettingsPanel from '@/components/SettingsPanel';
import { useEditorStore } from '@/stores/editor';
import { mockBackend } from './setup';
const settings = { typography: 'chinese-reading', font_family: 'Songti SC', font_size: 14, line_height: 1.6,
  page_width_inch: 8.27, page_height_inch: 11.69, margin_top_inch: 1, margin_bottom_inch: 1,
  margin_left_inch: 1.5, margin_right_inch: 1, language: 'zh', template: 'a4', ocr_languages: [] };
beforeEach(() => {
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  localStorage.clear();
  mockBackend.reset();
  useEditorStore.setState({ document: { id: 'qa', revision: 1, blocks: [], settings, metadata: { title: '虚构', author: '' } } as never,
    zoom: 1, activeBlockId: null, toasts: [] });
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
it('人物名和画外音分别设置字重，不把格式标签写入内容', async () => {
  mockBackend.on('get_pagination', () => ({ total_pages: 1, has_title_page: false, pages: [{ number: 1,
    show_page_number: false, used_lines: 3, blocks: [{ block_id: 'cue', block_type: 'character',
      text: '小林（画外音）', start_line: 1, line_count: 1, more: false, contd: false }] }] }));
  const change = vi.fn().mockResolvedValue(true);
  useEditorStore.setState({ changeBlockText: change });
  const { container } = render(<ScreenplayEditor />);
  await waitFor(() => expect(container.querySelector('.cue-name')?.textContent).toBe('小林'));
  expect(container.querySelector('.cue-extension')?.textContent).toBe('（画外音）');
  const block = container.querySelector('.screenplay-block') as HTMLElement;
  expect(block.textContent).toBe('小林（画外音）');
  expect(block.style.marginLeft).toBe('0in');
  expect(parseFloat((container.querySelector('.block-row') as HTMLElement).style.top)).toBeCloseTo(22.4);
  await act(async () => fireEvent.blur(block));
  expect(change).not.toHaveBeenCalled();
});
it('排版模板有独立标签，并通过后端设置命令持久化', async () => {
  mockBackend.on('autosave_config', () => ({ enabled: true, saveIntervalSecs: 30, versionIntervalSecs: 300 }));
  mockBackend.on('update_settings', () => ({}));
  mockBackend.on('configure_autosave', () => ({}));
  const { getByLabelText, getByRole } = render(<SettingsPanel onClose={() => undefined} />);
  fireEvent.click(getByLabelText('导出时显示场号'));
  fireEvent.change(getByLabelText('剧本排版模板'), { target: { value: 'hollywood' } });
  await act(async () => fireEvent.click(getByRole('button', { name: '保存设置' })));
  expect(mockBackend.callsTo('update_settings')[0]?.args?.patch).toMatchObject({ typography: 'hollywood', showSceneNumbers: true });
});

it('未手动改过场号时随预设取默认值，改过以后不再被预设覆盖', () => {
  mockBackend.on('autosave_config', () => ({ enabled: true, saveIntervalSecs: 30, versionIntervalSecs: 300 }));
  const { getByLabelText } = render(<SettingsPanel onClose={() => undefined} />);
  const preset = getByLabelText('剧本排版模板');
  const toggle = getByLabelText('导出时显示场号') as HTMLInputElement;
  expect(toggle.checked).toBe(false);
  fireEvent.change(preset, { target: { value: 'chinese-production' } });
  expect(toggle.checked).toBe(true);
  fireEvent.change(preset, { target: { value: 'chinese-general' } });
  expect(toggle.checked).toBe(false);
  fireEvent.change(preset, { target: { value: 'chinese-production' } });
  fireEvent.click(toggle);
  expect(toggle.checked).toBe(false);
  fireEvent.change(preset, { target: { value: 'hollywood' } });
  expect(toggle.checked).toBe(false);
  fireEvent.change(preset, { target: { value: 'chinese-production' } });
  expect(toggle.checked).toBe(false);
});

it('场号是独立显示层，不混入可编辑标题，编号设置参与后端保存', async () => {
  const heading = { id: 'scene', block_type: 'scene_heading', text: '外景。纽约——白天', scene_heading: { scene_number: '十二' } };
  useEditorStore.setState({ document: { id: 'qa', revision: 1, blocks: [heading], settings: { ...settings, show_scene_numbers: true }, metadata: { title: '虚构', author: '' } } as never });
  mockBackend.on('get_pagination', () => ({ total_pages: 1, has_title_page: false, pages: [{ number: 1, show_page_number: false, used_lines: 2,
    blocks: [{ block_id: 'scene', block_type: 'scene_heading', text: heading.text, start_line: 0, line_count: 2 }] }] }));
  const { container } = render(<ScreenplayEditor />);
  await waitFor(() => expect(container.querySelector('.scene-number-label')?.textContent).toBe('第十二场'));
  expect(container.querySelector('.screenplay-block')?.textContent).toBe(heading.text);
  expect(container.querySelector('.scene-number-label')?.getAttribute('contenteditable')).toBe('false');
});

it('角色补全通过当前剧本人物提供候选，回车确认仍保存纯文本', async () => {
  const block = { id: 'cue', block_type: 'character', text: '' };
  useEditorStore.setState({ document: { id: 'qa', revision: 1, blocks: [block], characters: [{ name: '小林' }, { name: '小李' }], settings } as never,
    activeBlockId: 'cue', setActiveBlock: vi.fn(), changeBlockText: vi.fn().mockResolvedValue(true) });
  mockBackend.on('get_pagination', () => ({ total_pages: 1, pages: [{ number: 1, blocks: [{ block_id: 'cue', block_type: 'character', text: '', start_line: 0, line_count: 1 }] }] }));
  const { container, getByRole } = render(<ScreenplayEditor />);
  await waitFor(() => expect(container.querySelector('.screenplay-block')).not.toBeNull());
  const field = container.querySelector('.screenplay-block') as HTMLElement;
  act(() => field.focus());
  expect(getByRole('option', { name: '小林' })).toBeTruthy();
  fireEvent.keyDown(field, { key: 'Enter' });
  expect(field.textContent).toBe('小林');
  expect(container.querySelector('.writing-completions')).toBeNull();
  fireEvent.compositionStart(field);
  fireEvent.input(field);
  expect(container.querySelector('.writing-completions')).toBeNull();
});

it('括注独立显示且旧自定义粗体不会污染字重', async () => {
  const blocks=[{id:'cue',block_type:'character',text:'小林'},{id:'note',block_type:'parenthetical',text:'轻声'}];
  useEditorStore.setState({document:{id:'qa',revision:1,blocks,settings:{...settings,element_styles:{parenthetical:{font_family:'Songti SC',font_size:12,bold:true,italic:false,color:'#000000'}}}} as never});
  mockBackend.on('get_pagination',()=>({total_pages:1,pages:[{number:1,blocks:blocks.map((b,i)=>({block_id:b.id,block_type:b.block_type,text:b.text,start_line:i,line_count:1}))}]}));
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('.block-parenthetical')?.textContent).toBe('轻声'));
  const note=container.querySelector('.block-parenthetical') as HTMLElement;
  expect(note.style.fontWeight).toBe('400');expect(container.querySelector('.block-character')?.textContent).toBe('小林');
  expect(container.querySelector('.parenthetical-shell')?.textContent).toBe('（轻声）');
});

it('蒙太奇只加粗三个字，句号与正文同段且不加粗',async()=>{
 const text='蒙太奇。画面上是各种报纸的头版标题。';
 useEditorStore.setState({document:{id:'qa',revision:1,blocks:[{id:'m',block_type:'action',text}],settings} as never});
 mockBackend.on('get_pagination',()=>({total_pages:1,pages:[{number:1,blocks:[{block_id:'m',block_type:'action',text,start_line:0,line_count:1}]}]}));
 const view=render(<ScreenplayEditor/>);
 await waitFor(()=>expect(view.container.querySelector('.technical-prefix')?.textContent).toBe('蒙太奇'));
 expect(view.container.querySelector('.technical-body')?.textContent).toBe('。画面上是各种报纸的头版标题。');
 expect(view.container.querySelectorAll('.screenplay-block')).toHaveLength(1);
});

it('中式剧本预设可选，△ 与冒号只作装饰不写入正文', async () => {
  const blocks = [
    { id: 'a', block_type: 'action', text: '李明擦汗后退，撞到墙。' },
    { id: 'c', block_type: 'character', text: '李明' },
    { id: 'p', block_type: 'parenthetical', text: '低声' },
    { id: 'd', block_type: 'dialogue', text: '报告……' },
  ];
  useEditorStore.setState({ document: { id: 'qa', revision: 1, blocks, settings: { ...settings, typography: 'chinese-production', font_family: 'Courier New, Songti SC', font_size: 12, line_height: 1.5 }, metadata: { title: '虚构', author: '' } } as never });
  mockBackend.on('get_pagination', () => ({ total_pages: 1, pages: [{ number: 1, blocks: blocks.map((b, i) => ({ block_id: b.id, block_type: b.block_type, text: b.text, start_line: i, line_count: 1 })) }] }));
  const { container } = render(<ScreenplayEditor />);
  await waitFor(() => expect(container.querySelector('.prefix-delta')).not.toBeNull());
  expect(container.querySelector('.prefix-delta')?.textContent).toBe('李明擦汗后退，撞到墙。');
  expect(container.querySelector('.block-parenthetical.cue-colon')?.textContent).toBe('低声');
  expect(container.querySelector('.block-character.cue-colon')).toBeNull();
  expect(container.textContent).not.toContain('△');
  expect(container.querySelectorAll('.cue-colon')).toHaveLength(1);
});

it('四个排版预设的名称集中在同一处并按顺序展示', () => {
  mockBackend.on('autosave_config', () => ({ enabled: true, saveIntervalSecs: 30, versionIntervalSecs: 300 }));
  const { getByLabelText } = render(<SettingsPanel onClose={() => undefined} />);
  const select = getByLabelText('剧本排版模板') as HTMLSelectElement;
  expect([...select.options].map(o => o.textContent)).toEqual(['好莱坞美式剧本', '好莱坞中文剧本', '中文通用剧本', '中式剧本（△ 标记）']);
});

it('中文通用剧本不添加 △ 或冒号装饰', async () => {
  const blocks = [
    { id: 'a', block_type: 'action', text: '李明擦汗后退，撞到墙。' },
    { id: 'c', block_type: 'character', text: '李明' },
    { id: 'p', block_type: 'parenthetical', text: '低声' },
    { id: 'd', block_type: 'dialogue', text: '报告……' },
  ];
  useEditorStore.setState({ document: { id: 'qa', revision: 1, blocks, settings: { ...settings, typography: 'chinese-general', font_family: 'Songti SC, SimSun', font_size: 12, line_height: 1.5 }, metadata: { title: '虚构', author: '' } } as never });
  mockBackend.on('get_pagination', () => ({ total_pages: 1, pages: [{ number: 1, blocks: blocks.map((b, i) => ({ block_id: b.id, block_type: b.block_type, text: b.text, start_line: i, line_count: 1 })) }] }));
  const { container } = render(<ScreenplayEditor />);
  await waitFor(() => expect(container.querySelector('.typography-chinese-general')).not.toBeNull());
  expect(container.querySelectorAll('.prefix-delta')).toHaveLength(0);
  expect(container.querySelectorAll('.cue-colon')).toHaveLength(0);
  expect(container.querySelector('.block-action')?.textContent).toBe('李明擦汗后退，撞到墙。');
});
