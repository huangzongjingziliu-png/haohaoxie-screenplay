import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {act,cleanup,fireEvent,render,waitFor} from '@testing-library/react';
import ScreenplayEditor from '@/editors/screenplay/ScreenplayEditor';
import {useEditorStore} from '@/stores/editor';
import {mockBackend} from './setup';
import {
  orderedBlockRange,
  sceneBlockIds,
  blocksToText,
  rememberBlockClipboard,
  readBlockClipboard,
  clearBlockClipboard,
  floatingMenuTop,
  blockRangeAnnouncement,
} from '@/editors/screenplay/blockSelection';
import type {BlockLike} from '@/editors/screenplay/blockSelection';

const settings={typography:'chinese-reading',font_family:'Songti SC',font_size:14,line_height:1.6,page_width_inch:8.27,page_height_inch:11.69};
const blocks:BlockLike[]=[
  {id:'sc1',block_type:'scene_heading',text:'1 内景 办公室 日'},
  {id:'act1',block_type:'action',text:'他推门进来。'},
  {id:'ch1',block_type:'character',text:'张三'},
  {id:'dlg1',block_type:'dialogue',text:'我来了。'},
  {id:'sc2',block_type:'scene_heading',text:'2 外景 街道 夜'},
  {id:'act2',block_type:'action',text:'雨很大。'},
];

const pageOf=(list:BlockLike[])=>({pages:[{number:1,blocks:list.map((b,i)=>({block_id:b.id,block_type:b.block_type,text:b.text,start_line:i,line_count:1}))}]});

beforeEach(()=>{
  mockBackend.reset();
  vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});
  clearBlockClipboard();
  window.localStorage.clear();
  useEditorStore.setState({document:{id:'d',revision:1,blocks,settings} as never,composing:false,zoom:1,activeBlockId:'act1'});
  mockBackend.on('get_pagination',()=>pageOf(blocks));
});
afterEach(()=>{cleanup();vi.unstubAllGlobals();window.getSelection()?.removeAllRanges();document.body.innerHTML='';});

const row=(container:HTMLElement,id:string)=>container.querySelector(`[data-block-id="${id}"]`) as HTMLElement;
const editable=(container:HTMLElement,id:string)=>row(container,id).querySelector('.screenplay-block') as HTMLElement;
const gutter=(container:HTMLElement,id:string)=>row(container,id).querySelector('.block-gutter') as HTMLElement;
const selected=(container:HTMLElement)=>Array.from(container.querySelectorAll('.block-row.block-selected[data-block-id]')).map(r=>(r as HTMLElement).dataset.blockId);
const focusBlock=(container:HTMLElement,id:string)=>{const el=editable(container,id);el.focus();return el;};

const mockClipboard=(impl:(text:string)=>Promise<void>)=>{
  const writeText=vi.fn(impl);
  Object.defineProperty(navigator,'clipboard',{value:{writeText},configurable:true});
  return writeText;
};

it('纯逻辑：区间、场范围、块剪贴板与浮动菜单翻转',()=>{
  const ids=blocks.map(b=>b.id);
  expect(orderedBlockRange(ids,'act1','dlg1')).toEqual(['act1','ch1','dlg1']);
  expect(orderedBlockRange(ids,'dlg1','act1')).toEqual(['act1','ch1','dlg1']);
  expect(orderedBlockRange(ids,'missing','dlg1')).toEqual([]);
  expect(sceneBlockIds(blocks,'ch1')).toEqual(['sc1','act1','ch1','dlg1']);
  expect(sceneBlockIds(blocks,'act2')).toEqual(['sc2','act2']);
  expect(blocksToText(blocks.slice(1,3))).toBe('他推门进来。\n张三');
  rememberBlockClipboard('甲\n乙',[{type:'action',text:'甲'},{type:'dialogue',text:'乙'}]);
  expect(readBlockClipboard('别的文字')).toBeNull();
  expect(readBlockClipboard('甲\n乙')).toEqual([{type:'action',text:'甲'},{type:'dialogue',text:'乙'}]);
  // 下方空间够：贴段落顶部展开
  expect(floatingMenuTop({rowTop:100,rowBottom:120,containerHeight:800,menuHeight:300})).toEqual({top:100,maxHeight:696});
  // 下方空间不足：向上翻转到段落下沿之上
  expect(floatingMenuTop({rowTop:600,rowBottom:620,containerHeight:800,menuHeight:300})).toEqual({top:320,maxHeight:476});
  // 菜单比容器还高：限高并从顶部开始
  expect(floatingMenuTop({rowTop:100,rowBottom:120,containerHeight:200,menuHeight:500}).top).toBe(4);
  expect(blockRangeAnnouncement(0)).toBe('');
  expect(blockRangeAnnouncement(3)).toContain('3 个段落');
});

it('Shift 点击设端点并显示块选中态，Esc 取消',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'ch1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'ch1'),{shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1']);
  fireEvent.keyDown(document.body,{key:'Escape'});
  expect(selected(container)).toEqual([]);
});

it('拖过块边界形成块选区；回拖到锚点收缩为单块',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'ch1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{buttons:1});
  fireEvent.mouseMove(row(container,'dlg1'),{buttons:1});
  expect(selected(container)).toEqual(['act1','ch1','dlg1']);
  fireEvent.mouseMove(row(container,'act1'),{buttons:1});
  expect(selected(container)).toEqual(['act1']);
  fireEvent.mouseUp(window);
});

it('pointermove 拖动也能扩范围并回拖收缩（WKWebView 兜底路径）',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'ch1')).toBeTruthy());
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  fireEvent(row(container,'dlg1'),new Event('pointermove',{bubbles:true}));
  expect(selected(container)).toEqual(['act1','ch1','dlg1']);
  fireEvent(row(container,'act1'),new Event('pointermove',{bubbles:true}));
  expect(selected(container)).toEqual(['act1']);
  fireEvent.pointerUp(window);
});

it('⌘A 先选当前场，再按一次选全文',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const el=focusBlock(container,'act1');
  fireEvent.keyDown(el,{key:'a',metaKey:true});
  expect(selected(container)).toEqual(['sc1','act1','ch1','dlg1']);
  fireEvent.keyDown(document.body,{key:'a',metaKey:true});
  expect(selected(container)).toEqual(blocks.map(b=>b.id));
});

it('Shift+↓/↑ 从当前块起选并按块扩展',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const el=focusBlock(container,'act1');
  fireEvent.keyDown(el,{key:'ArrowDown',shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1']);
  fireEvent.keyDown(document.body,{key:'ArrowDown',shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1','dlg1']);
  fireEvent.keyDown(document.body,{key:'ArrowUp',shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1']);
});

it('⌘C 按块复制到系统剪贴板（保留顺序）',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'dlg1'),{shiftKey:true});
  const writeText=mockClipboard(()=>Promise.resolve());
  fireEvent.keyDown(document.body,{key:'c',metaKey:true});
  await waitFor(()=>expect(writeText).toHaveBeenCalledWith('他推门进来。\n张三\n我来了。'));
});

it('⌘X 剪切：写剪贴板成功后才按块删除',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'ch1'),{shiftKey:true});
  const writeText=mockClipboard(()=>Promise.resolve());
  const applyBlockBatch=vi.fn(async()=>({affected:['act1','ch1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  fireEvent.keyDown(document.body,{key:'x',metaKey:true});
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['act1','ch1']},expect.any(Number)));
  expect(writeText).toHaveBeenCalledWith('他推门进来。\n张三');
});

it('⌘X 写剪贴板失败时保留正文',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'ch1'),{shiftKey:true});
  mockClipboard(()=>Promise.reject(new Error('denied')));
  const applyBlockBatch=vi.fn(async()=>({affected:[]}));
  useEditorStore.setState({applyBlockBatch} as never);
  fireEvent.keyDown(document.body,{key:'x',metaKey:true});
  await act(async()=>{await Promise.resolve();await Promise.resolve();});
  expect(applyBlockBatch).not.toHaveBeenCalled();
});

it('块选区按退格/删除删除整块（锚点沿用当前块）',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const el=focusBlock(container,'act1');
  fireEvent.mouseDown(row(container,'ch1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'dlg1'),{shiftKey:true});
  const applyBlockBatch=vi.fn(async()=>({affected:['act1','ch1','dlg1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  fireEvent.keyDown(el,{key:'Backspace'});
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['act1','ch1','dlg1']}));
});

it('左侧 gutter：单击选整块并把焦点交回正文，Shift 点扩展，退格整块删除',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  expect(selected(container)).toEqual(['act1']);
  // 焦点必须在正文里，⌘C/⌘X 与 Shift+↑↓ 才能立刻用
  expect(document.activeElement).toBe(editable(container,'act1'));
  fireEvent.keyDown(document.activeElement as HTMLElement,{key:'ArrowDown',shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1']);
  fireEvent.mouseDown(gutter(container,'dlg1'),{button:0,shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1','dlg1']);
  const applyBlockBatch=vi.fn(async()=>({affected:['act1','ch1','dlg1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  fireEvent.keyDown(document.activeElement as HTMLElement,{key:'Backspace'});
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['act1','ch1','dlg1']}));
});

it('gutter 支持键盘/读屏激活（click detail 0）',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'dlg1')).toBeTruthy());
  const g=gutter(container,'dlg1');
  expect(g.getAttribute('aria-hidden')).toBeNull();
  expect(g.getAttribute('role')).toBe('button');
  fireEvent.click(g,{detail:0});
  expect(selected(container)).toEqual(['dlg1']);
  expect(document.activeElement).toBe(editable(container,'dlg1'));
});

it('copy 事件兜底：原生菜单截走 ⌘C 时仍按块写入剪贴板',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  fireEvent.mouseDown(gutter(container,'ch1'),{button:0,shiftKey:true});
  const writeText=mockClipboard(()=>Promise.resolve());
  const ev=new Event('copy',{bubbles:true,cancelable:true});
  window.dispatchEvent(ev);
  expect(ev.defaultPrevented).toBe(true);
  await waitFor(()=>expect(writeText).toHaveBeenCalledWith('他推门进来。\n张三'));
  expect(readBlockClipboard('他推门进来。\n张三')).toEqual([{type:'action',text:'他推门进来。'},{type:'character',text:'张三'}]);
});

it('cut 事件兜底：复制成功后才删除，失败则保留',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  fireEvent.mouseDown(gutter(container,'ch1'),{button:0,shiftKey:true});
  const applyBlockBatch=vi.fn(async()=>({affected:['act1','ch1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  mockClipboard(()=>Promise.reject(new Error('denied')));
  window.dispatchEvent(new Event('cut',{bubbles:true,cancelable:true}));
  await act(async()=>{await Promise.resolve();await Promise.resolve();});
  expect(applyBlockBatch).not.toHaveBeenCalled();

  mockClipboard(()=>Promise.resolve());
  window.dispatchEvent(new Event('cut',{bubbles:true,cancelable:true}));
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['act1','ch1']},expect.any(Number)));
});

it('剪切按操作开始时的块集合与 revision 删除，不受剪贴板写入期间选区变化影响',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  fireEvent.mouseDown(gutter(container,'ch1'),{button:0,shiftKey:true});
  let finish!:()=>void;
  const writeText=mockClipboard(()=>new Promise<void>(resolve=>{finish=resolve;}));
  const applyBlockBatch=vi.fn(async()=>({affected:['act1','ch1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  window.dispatchEvent(new Event('cut',{bubbles:true,cancelable:true}));
  await waitFor(()=>expect(writeText).toHaveBeenCalled());
  // 剪贴板还没写完，用户改了选区
  fireEvent.mouseDown(gutter(container,'dlg1'),{button:0});
  expect(selected(container)).toEqual(['dlg1']);
  finish();
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['act1','ch1']},expect.any(Number)));
  expect(selected(container)).toEqual([]);
});

it('无块选区时不拦截原生复制/剪切',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const ev=new Event('copy',{bubbles:true,cancelable:true});
  window.dispatchEvent(ev);
  expect(ev.defaultPrevented).toBe(false);
});

it('焦点离开正文（工具栏/场景板）立刻清块选区，Delete 不误删',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  expect(selected(container)).toEqual(['act1']);
  const toolbar=document.createElement('button');
  document.body.appendChild(toolbar);
  toolbar.focus();
  await waitFor(()=>expect(selected(container)).toEqual([]));
  const applyBlockBatch=vi.fn(async()=>({affected:['act1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  fireEvent.keyDown(toolbar,{key:'Delete'});
  await act(async()=>{await Promise.resolve();});
  expect(applyBlockBatch).not.toHaveBeenCalled();
});

it('文档变化后清理失效的块选区端点',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  fireEvent.mouseDown(gutter(container,'ch1'),{button:0,shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1']);
  const next=blocks.filter(b=>b.id!=='ch1');
  act(()=>{
    useEditorStore.setState({document:{id:'d',revision:2,blocks:next,settings} as never});
    mockBackend.on('get_pagination',()=>pageOf(next));
  });
  await waitFor(()=>expect(selected(container)).toEqual([]));
});

it('撤销后清空块选区，下一次删除不再按块执行',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  expect(selected(container)).toEqual(['act1']);
  const el=focusBlock(container,'act1');
  fireEvent.keyDown(el,{key:'z',metaKey:true});
  await waitFor(()=>expect(selected(container)).toEqual([]));
  const applyBlockBatch=vi.fn(async()=>({affected:['act1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  fireEvent.keyDown(el,{key:'Delete'});
  await act(async()=>{await Promise.resolve();});
  expect(applyBlockBatch).not.toHaveBeenCalled();
});

it('粘贴只在焦点位于正文时拦截',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  rememberBlockClipboard('他推门进来。',[{type:'action',text:'他推门进来。'}]);
  const applyBlockBatch=vi.fn(async()=>({affected:[]}));
  useEditorStore.setState({applyBlockBatch} as never);
  const search=document.createElement('input');
  document.body.appendChild(search);
  search.focus();
  const paste=new Event('paste',{bubbles:true,cancelable:true});
  Object.defineProperty(paste,'clipboardData',{value:{getData:()=> '他推门进来。'}});
  window.dispatchEvent(paste);
  await act(async()=>{await Promise.resolve();});
  expect(paste.defaultPrevented).toBe(false);
  expect(applyBlockBatch).not.toHaveBeenCalled();
});

it('块剪贴板粘贴：焦点在正文时按块插入',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  rememberBlockClipboard('他推门进来。\n张三',[{type:'action',text:'他推门进来。'},{type:'character',text:'张三'}]);
  const applyBlockBatch=vi.fn(async()=>({affected:['new1','new2']}));
  useEditorStore.setState({applyBlockBatch} as never);
  const paste=new Event('paste',{bubbles:true,cancelable:true});
  Object.defineProperty(paste,'clipboardData',{value:{getData:()=> '他推门进来。\n张三'}});
  window.dispatchEvent(paste);
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({
    insertAfter:'act1',
    insert:[{blockType:'action',text:'他推门进来。'},{blockType:'character',text:'张三'}],
  }));
});

it('三击括注整块选中，退格连同装饰括号整块删除',async()=>{
  const doc={id:'d',revision:1,blocks:[
    {id:'a',block_type:'action',text:'前一段'},
    {id:'par',block_type:'parenthetical',text:'低声'},
    {id:'b',block_type:'action',text:'后一段'},
  ],settings};
  const applyBlockBatch=vi.fn(async()=>({affected:['par']}));
  useEditorStore.setState({document:doc as never,activeBlockId:'par',applyBlockBatch} as never);
  mockBackend.on('get_pagination',()=>pageOf(doc.blocks as BlockLike[]));
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'par')).toBeTruthy());
  fireEvent.mouseDown(row(container,'par'),{button:0,detail:3});
  expect(selected(container)).toEqual(['par']);
  fireEvent.keyDown(document.activeElement as HTMLElement,{key:'Backspace'});
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['par']}));
});

it('合并对白行：gutter 选中整组并显示选中态，行内块可单独选',async()=>{
  const doc={id:'d',revision:1,blocks:[
    {id:'sc',block_type:'scene_heading',text:'1 内景 日'},
    {id:'c1',block_type:'character',text:'张三'},
    {id:'p1',block_type:'parenthetical',text:'低声'},
    {id:'d1',block_type:'dialogue',text:'我来了。'},
  ],settings};
  useEditorStore.setState({document:doc as never,activeBlockId:'c1'});
  mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:[
    {block_id:'sc',block_type:'scene_heading',text:'1 内景 日',start_line:0,line_count:1},
    {block_id:'c1',block_type:'character',text:'张三',start_line:1,line_count:1,group:'g1'},
    {block_id:'p1',block_type:'parenthetical',text:'低声',start_line:1,line_count:1,group:'g1'},
    {block_id:'d1',block_type:'dialogue',text:'我来了。',start_line:1,line_count:1,group:'g1'},
  ]}]}));
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('.merged-dialogue-row')).toBeTruthy());
  const merged=container.querySelector('.merged-dialogue-row') as HTMLElement;
  expect(merged.getAttribute('role')).toBe('option');
  fireEvent.mouseDown(merged.querySelector('.block-gutter') as HTMLElement,{button:0});
  expect(selected(container)).toEqual(['c1','p1','d1']);
  expect(merged.className).toContain('block-selected');
  expect(merged.getAttribute('aria-selected')).toBe('true');
  // 沿 gutter 拖过合并行：整组纳入范围
  fireEvent.mouseDown(gutter(container,'sc'),{button:0});
  fireEvent.mouseMove(merged.querySelector('.block-gutter') as HTMLElement,{buttons:1});
  expect(selected(container)).toEqual(['sc','c1','p1','d1']);
  // 行内块可单独选（Esc 清空后 Shift 点行内正文，以它自己为锚点）
  fireEvent.keyDown(document.activeElement as HTMLElement,{key:'Escape'});
  expect(selected(container)).toEqual([]);
  fireEvent.mouseDown(row(container,'d1'),{shiftKey:true});
  expect(selected(container)).toEqual(['d1']);
  expect(merged.className).toContain('block-selected');
});

it('无障碍：容器 aria-multiselectable、选中项 aria-selected、范围变化播报',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const list=container.querySelector('.pages-container') as HTMLElement;
  expect(list.getAttribute('role')).toBe('listbox');
  expect(list.getAttribute('aria-multiselectable')).toBe('true');
  fireEvent.mouseDown(gutter(container,'act1'),{button:0});
  fireEvent.mouseDown(gutter(container,'ch1'),{button:0,shiftKey:true});
  expect(row(container,'act1').getAttribute('aria-selected')).toBe('true');
  expect(row(container,'dlg1').getAttribute('aria-selected')).toBe('false');
  const live=container.querySelector('[role="status"]') as HTMLElement;
  expect(live.textContent).toContain('2 个段落');
});

it('元素菜单键盘选型后把焦点交回正文',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const changeBlockType=vi.fn(async()=>true);
  useEditorStore.setState({changeBlockType} as never);
  focusBlock(container,'act1');
  const button=container.querySelector('.float-menu-list button') as HTMLElement;
  expect(button).toBeTruthy();
  fireEvent.click(button);
  await waitFor(()=>expect(changeBlockType).toHaveBeenCalled());
  await waitFor(()=>expect(document.activeElement?.closest('.screenplay-block')).toBeTruthy());
});

it('同一源块跨分页片段：按 source offset 在单块内删除',async()=>{
  const doc={id:'d',revision:1,blocks:[
    {id:'act',block_type:'action',text:'前一段'},
    {id:'dlg',block_type:'dialogue',text:'前半后半'},
  ],settings};
  useEditorStore.setState({document:doc as never,activeBlockId:'dlg'});
  mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:[
    {block_id:'act',block_type:'action',text:'前一段',start_line:0,line_count:1,scene_id:null,split:false,more:false,contd:false,contd_character:null},
    {block_id:'dlg:part:1',block_type:'dialogue',text:'前半',start_line:1,line_count:1,scene_id:null,split:true,more:true,contd:false,contd_character:null,source_start:0},
    {block_id:'dlg:part:2',block_type:'dialogue',text:'后半',start_line:2,line_count:1,scene_id:null,split:true,more:false,contd:true,contd_character:'张三',source_start:2},
  ]}]}));
  const applyBlockBatch=vi.fn(async()=>({affected:['dlg']}));
  useEditorStore.setState({applyBlockBatch} as never);
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'dlg:part:1')).toBeTruthy());
  const first=editable(container,'dlg:part:1');
  const second=editable(container,'dlg:part:2');
  first.focus();
  const range=document.createRange();
  range.setStart(first.firstChild!,1);
  range.setEnd(second.firstChild!,1);
  const sel=window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
  fireEvent.keyDown(first,{key:'Backspace'});
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({replace:[{blockId:'dlg',text:'前半'}]}));
});
