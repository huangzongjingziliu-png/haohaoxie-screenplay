import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,waitFor} from '@testing-library/react';
import ScreenplayEditor from '@/editors/screenplay/ScreenplayEditor';
import {useEditorStore} from '@/stores/editor';
import {mockBackend} from './setup';
import {orderedBlockRange,sceneBlockIds,blocksToText,rememberBlockClipboard,readBlockClipboard} from '@/editors/screenplay/blockSelection';
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

beforeEach(()=>{
  mockBackend.reset();
  vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});
  useEditorStore.setState({document:{id:'d',revision:1,blocks,settings} as never,composing:false,zoom:1,activeBlockId:'act1'});
  mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:blocks.map((b,i)=>({block_id:b.id,block_type:b.block_type,text:b.text,start_line:i,line_count:1}))}]}));
});
afterEach(()=>{cleanup();vi.unstubAllGlobals();window.getSelection()?.removeAllRanges();});

const row=(container:HTMLElement,id:string)=>container.querySelector(`[data-block-id="${id}"]`) as HTMLElement;
const selected=(container:HTMLElement)=>Array.from(container.querySelectorAll('.block-row.block-selected')).map(r=>(r as HTMLElement).dataset.blockId);

it('纯逻辑：区间、场范围与块剪贴板',()=>{
  const ids=blocks.map(b=>b.id);
  expect(orderedBlockRange(ids,'act1','dlg1')).toEqual(['act1','ch1','dlg1']);
  expect(orderedBlockRange(ids,'dlg1','act1')).toEqual(['act1','ch1','dlg1']);
  expect(sceneBlockIds(blocks,'ch1')).toEqual(['sc1','act1','ch1','dlg1']);
  expect(sceneBlockIds(blocks,'act2')).toEqual(['sc2','act2']);
  expect(blocksToText(blocks.slice(1,3))).toBe('他推门进来。\n张三');
  rememberBlockClipboard('甲\n乙',[{type:'action',text:'甲'},{type:'dialogue',text:'乙'}]);
  expect(readBlockClipboard('别的文字')).toBeNull();
  expect(readBlockClipboard('甲\n乙')).toEqual([{type:'action',text:'甲'},{type:'dialogue',text:'乙'}]);
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

it('拖过块边界形成块选区',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'ch1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{buttons:1});
  fireEvent.mouseMove(row(container,'dlg1'),{buttons:1});
  expect(selected(container)).toEqual(['act1','ch1','dlg1']);
});

it('⌘A 先选当前场，再按一次选全文',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const el=container.querySelector('[data-block-id="act1"] .screenplay-block') as HTMLElement;
  el.focus();
  fireEvent.keyDown(el,{key:'a',metaKey:true});
  expect(selected(container)).toEqual(['sc1','act1','ch1','dlg1']);
  fireEvent.keyDown(document.body,{key:'a',metaKey:true});
  expect(selected(container)).toEqual(blocks.map(b=>b.id));
});

it('⌘C 按块复制到系统剪贴板（保留顺序）',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'dlg1'),{shiftKey:true});
  const writeText=vi.fn(()=>Promise.resolve());
  Object.defineProperty(navigator,'clipboard',{value:{writeText},configurable:true});
  fireEvent.keyDown(document.body,{key:'c',metaKey:true});
  await waitFor(()=>expect(writeText).toHaveBeenCalledWith('他推门进来。\n张三\n我来了。'));
});

it('⌘X 剪切：复制后按块删除，一次批量命令',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'ch1'),{shiftKey:true});
  const writeText=vi.fn(()=>Promise.resolve());
  Object.defineProperty(navigator,'clipboard',{value:{writeText},configurable:true});
  const applyBlockBatch=vi.fn(async()=>({affected:['act1','ch1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  fireEvent.keyDown(document.body,{key:'x',metaKey:true});
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['act1','ch1']}));
  await waitFor(()=>expect(writeText).toHaveBeenCalledWith('他推门进来。\n张三'));
});

it('块选区按退格/删除删除整块',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'ch1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'dlg1'),{shiftKey:true});
  const applyBlockBatch=vi.fn(async()=>({affected:['ch1','dlg1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  fireEvent.keyDown(document.body,{key:'Backspace'});
  // Shift 点击沿用当前焦点块作为锚点（act1）
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['act1','ch1','dlg1']}));
});

it('左侧 gutter：单击选整块、Shift 点扩展、退格整块删除（括注连同括号消失）',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const gutter=(id:string)=>container.querySelector(`[data-block-id="${id}"] .block-gutter`) as HTMLElement;
  expect(gutter('act1')).toBeTruthy();
  fireEvent.mouseDown(gutter('act1'),{button:0});
  expect(selected(container)).toEqual(['act1']);
  fireEvent.mouseDown(gutter('dlg1'),{button:0,shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1','dlg1']);
  const applyBlockBatch=vi.fn(async()=>({affected:['act1','ch1','dlg1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  fireEvent.keyDown(document.body,{key:'Backspace'});
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['act1','ch1','dlg1']}));
});

it('copy 事件兜底：原生菜单截走 ⌘C 时仍按块写入剪贴板',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'ch1'),{shiftKey:true});
  const setData=vi.fn();
  const ev=new Event('copy',{bubbles:true,cancelable:true});
  Object.defineProperty(ev,'clipboardData',{value:{setData}});
  window.dispatchEvent(ev);
  expect(ev.defaultPrevented).toBe(true);
  expect(setData).toHaveBeenCalledWith('text/plain','他推门进来。\n张三');
  expect(readBlockClipboard('他推门进来。\n张三')).toEqual([{type:'action',text:'他推门进来。'},{type:'character',text:'张三'}]);
});

it('cut 事件兜底：原生菜单截走 ⌘X 时复制并按块删除',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'ch1'),{shiftKey:true});
  const applyBlockBatch=vi.fn(async()=>({affected:['act1','ch1']}));
  useEditorStore.setState({applyBlockBatch} as never);
  const setData=vi.fn();
  const ev=new Event('cut',{bubbles:true,cancelable:true});
  Object.defineProperty(ev,'clipboardData',{value:{setData}});
  window.dispatchEvent(ev);
  expect(setData).toHaveBeenCalledWith('text/plain','他推门进来。\n张三');
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['act1','ch1']}));
});

it('无块选区时不拦截原生复制/剪切',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const ev=new Event('copy',{bubbles:true,cancelable:true});
  Object.defineProperty(ev,'clipboardData',{value:{setData:vi.fn()}});
  window.dispatchEvent(ev);
  expect(ev.defaultPrevented).toBe(false);
});

it('已有块选区时 Shift+↓/↑ 按块扩展范围',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(container.querySelector('[data-block-id="act1"] .block-gutter')!,{button:0});
  expect(selected(container)).toEqual(['act1']);
  fireEvent.keyDown(document.body,{key:'ArrowDown',shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1']);
  fireEvent.keyDown(document.body,{key:'ArrowDown',shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1','dlg1']);
  fireEvent.keyDown(document.body,{key:'ArrowUp',shiftKey:true});
  expect(selected(container)).toEqual(['act1','ch1']);
});

it('三击括注整块选中，退格连同装饰括号整块删除',async()=>{
  const doc={id:'d',revision:1,blocks:[
    {id:'a',block_type:'action',text:'前一段'},
    {id:'par',block_type:'parenthetical',text:'低声'},
    {id:'b',block_type:'action',text:'后一段'},
  ],settings};
  const applyBlockBatch=vi.fn(async()=>({affected:['par']}));
  useEditorStore.setState({document:doc as never,activeBlockId:'par',applyBlockBatch} as never);
  mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:doc.blocks.map((b,i)=>({block_id:b.id,block_type:b.block_type,text:b.text,start_line:i,line_count:1}))}]}));
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'par')).toBeTruthy());
  fireEvent.mouseDown(row(container,'par'),{button:0,detail:3});
  expect(selected(container)).toEqual(['par']);
  fireEvent.keyDown(document.body,{key:'Backspace'});
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({delete:['par']}));
});

it('Esc 在编辑器外输入框聚焦时也能清掉块选区',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const gutter=container.querySelector('[data-block-id="act1"] .block-gutter') as HTMLElement;
  fireEvent.mouseDown(gutter,{button:0});
  expect(selected(container)).toEqual(['act1']);
  const input=document.createElement('input');
  document.body.appendChild(input);
  input.focus();
  fireEvent.keyDown(input,{key:'Escape'});
  expect(selected(container)).toEqual([]);
  input.remove();
});

it('⌘V 按块粘贴：保留类型与顺序，插到选中块之后',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  fireEvent.mouseDown(row(container,'act1'),{shiftKey:true});
  fireEvent.mouseDown(row(container,'ch1'),{shiftKey:true});
  const writeText=vi.fn(()=>Promise.resolve());
  Object.defineProperty(navigator,'clipboard',{value:{writeText},configurable:true});
  fireEvent.keyDown(document.body,{key:'c',metaKey:true});
  const applyBlockBatch=vi.fn(async()=>({affected:['new1','new2']}));
  useEditorStore.setState({applyBlockBatch} as never);
  const paste=new Event('paste',{bubbles:true,cancelable:true});
  Object.defineProperty(paste,'clipboardData',{value:{getData:()=> '他推门进来。\n张三' }});
  window.dispatchEvent(paste);
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledWith({
    insertAfter:'ch1',
    insert:[{blockType:'action',text:'他推门进来。'},{blockType:'character',text:'张三'}],
  }));
});

it('应用外复制的纯文本不按块粘贴（不劫持）',async()=>{
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(row(container,'act1')).toBeTruthy());
  const el=container.querySelector('[data-block-id="act1"] .screenplay-block') as HTMLElement;
  el.focus();
  const applyBlockBatch=vi.fn(async()=>({affected:[]}));
  useEditorStore.setState({applyBlockBatch} as never);
  const paste=new Event('paste',{bubbles:true,cancelable:true});
  Object.defineProperty(paste,'clipboardData',{value:{getData:()=> '外部文字' }});
  window.dispatchEvent(paste);
  expect(applyBlockBatch).not.toHaveBeenCalled();
});
