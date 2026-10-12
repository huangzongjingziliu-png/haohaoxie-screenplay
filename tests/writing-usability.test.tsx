import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Toolbar from '@/components/Toolbar';
import { registerPendingEditor } from '@/utils/pendingEdits';
import ScreenplayEditor from '@/editors/screenplay/ScreenplayEditor';
import { useEditorStore } from '@/stores/editor';
import { mockBackend } from './setup';
const settings = { typography: 'chinese-reading', font_family: 'Songti SC', font_size: 14, line_height: 1.6, page_width_inch: 8.27, page_height_inch: 11.69 };
beforeEach(() => {
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  mockBackend.reset();
  useEditorStore.setState({ viewMode: 'split', previousViewMode: 'split', sourceFirst: true, zoom: 1, composing: false, activeBlockId: null });
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
it('专注退出恢复原视图，互换不改变两种编辑器的语义', () => {
  const s = useEditorStore.getState();
  s.setViewMode('right'); s.setViewMode('focus'); s.exitFocus();
  expect(useEditorStore.getState().viewMode).toBe('right');
  s.swapEditors(); expect(useEditorStore.getState().sourceFirst).toBe(false);
  expect(useEditorStore.getState().viewMode).toBe('right');
});
it('括注显示真实括号，编辑文本不包含装饰且不重复已有括号', async () => {
  useEditorStore.setState({ document: { id: 'qa', revision: 1, blocks: [], settings } as never });
  mockBackend.on('get_pagination', () => ({ total_pages: 1, pages: [{ number: 1, blocks: [
    { block_id: 'a', block_type: 'parenthetical', text: '轻声', start_line: 0, line_count: 1 },
    { block_id: 'b', block_type: 'parenthetical', text: '（停顿）', start_line: 2, line_count: 1 },
  ] }] }));
  const { container } = render(<ScreenplayEditor />);
  await waitFor(() => expect(container.querySelectorAll('.parenthetical-shell')).toHaveLength(2));
  await waitFor(()=>expect(container.querySelectorAll('.parenthetical-shell')[0].textContent).toBe('（轻声）'));
  expect(container.querySelector('.block-parenthetical')?.textContent).toBe('轻声');
  expect(container.querySelectorAll('.parenthetical-shell')[1].textContent).toBe('（停顿）');
});
it('纸面回车插入下一元素并将光标移至新段落', async () => {
  let blocks = [{ id: 'a', block_type: 'action', text: '风从荒原吹来。' }];
  useEditorStore.setState({ document: { id: 'qa', revision: 1, blocks, settings } as never,
    changeBlockText: vi.fn().mockResolvedValue(true),
    insertBlock: vi.fn(async (_id, type, text) => {
      blocks = [...blocks, { id: 'b', block_type: type, text: text ?? '' }];
      useEditorStore.setState({ document: { id: 'qa', revision: 2, blocks, settings } as never, activeBlockId: 'b' });
    }) });
  mockBackend.on('get_pagination', () => ({ total_pages: 1, pages: [{ number: 1, blocks: blocks.map((b, i) => ({ ...b, block_id: b.id, start_line: i * 2, line_count: 1 })) }] }));
  const { container } = render(<ScreenplayEditor />);
  await waitFor(() => expect(container.querySelector('.screenplay-block')).not.toBeNull());
  const field = container.querySelector('.screenplay-block') as HTMLElement;
  act(() => { field.focus(); const r = document.createRange(); r.selectNodeContents(field); r.collapse(false); window.getSelection()?.removeAllRanges(); window.getSelection()?.addRange(r); });
  fireEvent.keyDown(field, { key: 'Enter' });
  await waitFor(() => expect(container.querySelector('[data-block-id="b"] .screenplay-block')).toBe(document.activeElement));
  expect(useEditorStore.getState().insertBlock).toHaveBeenCalledWith('a', 'action', '');
});

it('切换视图先等待草稿提交，避免卸载编辑器丢失最后输入', async () => {
  let finish!: () => void;
  const pending = new Promise<void>((resolve) => { finish = resolve; });
  const unregister = registerPendingEditor(() => pending);
  try {
    const { getByTitle } = render(<Toolbar onImport={() => undefined} />);
    fireEvent.change(getByTitle('视图模式'), { target: { value: 'focus' } });
    expect(useEditorStore.getState().viewMode).toBe('split');
    await act(async () => finish());
    await waitFor(() => expect(useEditorStore.getState().viewMode).toBe('focus'));
  } finally { unregister(); }
});
it('元素工具独立于纸面缩放，不再在纸面中渲染缩小标签', async () => {
  useEditorStore.setState({ document: { id: 'qa', revision: 1, blocks: [{ id: 'a', block_type: 'action', text: '动作' }], settings } as never, activeBlockId: 'a', zoom: 0.5 });
  mockBackend.on('get_pagination', () => ({ pages: [{ number: 1, blocks: [{ block_id: 'a', block_type: 'action', text: '动作', start_line: 0, line_count: 1 }] }] }));
  const { container, getByLabelText } = render(<ScreenplayEditor/>);
  await waitFor(() => expect(container.querySelector('.screenplay-block')).toBeTruthy());
  fireEvent.focus(container.querySelector('.screenplay-block')!);
  await waitFor(() => expect(getByLabelText('当前元素类型')).toBeTruthy());
  expect(container.querySelector('.screenplay-editor')?.contains(getByLabelText('当前元素类型'))).toBe(false);
  expect(container.querySelector('.block-controls')).toBeNull();
});
it('鼠标经过不显示或移动标签，光标焦点决定标签目标；快捷键修改焦点段', async () => {
  const change = vi.fn().mockResolvedValue(undefined);
  const blocks = [{ id: 'a', block_type: 'action', text: '动作' }, { id: 'b', block_type: 'character', text: '剑客' }];
  useEditorStore.setState({ document: { id: 'qa', revision: 1, blocks, settings } as never, activeBlockId: 'a', changeBlockType: change });
  mockBackend.on('get_pagination', () => ({ pages: [{ number: 1, blocks: blocks.map((b,i) => ({ ...b, block_id: b.id, start_line: i*2, line_count: 1 })) }] }));
  const { container, getByLabelText } = render(<ScreenplayEditor/>);
  await waitFor(() => expect(container.querySelector('[data-block-id="b"]')).toBeTruthy());
  expect(container.querySelector('.row-element-tools')).toBeNull();
  fireEvent.focus(container.querySelector('[data-block-id="a"] .screenplay-block')!);
  fireEvent.mouseOver(container.querySelector('[data-block-id="b"]')!);
  expect((getByLabelText('当前元素类型') as HTMLSelectElement).value).toBe('action');
  fireEvent.focus(container.querySelector('[data-block-id="b"] .screenplay-block')!);
  expect((getByLabelText('当前元素类型') as HTMLSelectElement).value).toBe('character');
  fireEvent.change(getByLabelText('当前元素类型'), { target: { value: 'action' } });
  await waitFor(() => expect(change).toHaveBeenCalledWith('b', 'action'));
  fireEvent.keyDown(container.querySelector('[data-block-id="a"] .screenplay-block')!, {key:'3',code:'Digit3',metaKey:true});
  await waitFor(() => expect(change).toHaveBeenCalledWith('a', 'character'));
  change.mockClear();
  fireEvent.keyDown(container.querySelector('[data-block-id="a"] .screenplay-block')!, {key:'2',code:'Digit2',metaKey:true,isComposing:true});
  expect(change).not.toHaveBeenCalled();
});

it('标准剧本视图隐藏左右互换，只在分栏显示', () => {
 expect(useEditorStore.getInitialState().viewMode).toBe('left');
 useEditorStore.setState({viewMode:'left'});
 const view=render(<Toolbar onImport={()=>{}}/>);
 expect(view.queryByRole('button',{name:'左右互换 ⇄'})).toBeNull();
 act(()=>useEditorStore.getState().setViewMode('split'));
 expect(view.getByRole('button',{name:'左右互换 ⇄'})).toBeTruthy();
 act(()=>useEditorStore.getState().setViewMode('right'));
 expect(view.queryByRole('button',{name:'左右互换 ⇄'})).toBeNull();
});

it('纸面采用分页规范标题，退焦不会还原成工程中的旧写法',async()=>{
 useEditorStore.setState({document:{id:'qa',revision:1,blocks:[{id:'heading',block_type:'scene_heading',text:'外．城堡外，日'}],settings} as never,activeBlockId:null,changeBlockText:vi.fn().mockResolvedValue(true)});
 mockBackend.on('get_pagination',()=>({total_pages:1,pages:[{number:1,blocks:[{block_id:'heading',block_type:'scene_heading',text:'外景。城堡外——日',start_line:0,line_count:1}]}]}));
 const {container}=render(<ScreenplayEditor/>);
 await waitFor(()=>expect(container.querySelector('.block-scene_heading')?.textContent).toBe('外景。城堡外——日'));
 const el=container.querySelector('.block-scene_heading') as HTMLElement;
 el.focus();el.blur();
 await waitFor(()=>expect(el.textContent).toBe('外景。城堡外——日'));
});
