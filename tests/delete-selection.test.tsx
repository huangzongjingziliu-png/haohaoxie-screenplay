import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,waitFor} from '@testing-library/react';
import ScreenplayEditor from '@/editors/screenplay/ScreenplayEditor';
import {useEditorStore} from '@/stores/editor';
import {mockBackend} from './setup';

const text='第一段动作描述，不能被并进上一段。';
const settings={typography:'chinese-reading',font_family:'Songti SC',font_size:14,line_height:1.6,page_width_inch:8.27,page_height_inch:11.69};

beforeEach(()=>{
  mockBackend.reset();
  vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});
  useEditorStore.setState({document:{id:'d',revision:1,blocks:[{id:'prev',block_type:'action',text:'上一段动作。'},{id:'action',block_type:'action',text}],settings} as never,composing:false,zoom:1,activeBlockId:null});
  mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:[{block_id:'prev',block_type:'action',text:'上一段动作。',start_line:0,line_count:1},{block_id:'action',block_type:'action',text,start_line:1,line_count:1}]}]}));
});
afterEach(()=>{cleanup();vi.unstubAllGlobals();window.getSelection()?.removeAllRanges();});

const selectAll=(el:HTMLElement)=>{
  const range=document.createRange();
  range.selectNodeContents(el);
  const sel=window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
};

it('选中整段按退格交给浏览器删除，不并入上一块',async()=>{
  const mergeBlock=vi.fn();
  useEditorStore.setState({mergeBlock});
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('[data-block-id="action"] .screenplay-block')).toBeTruthy());
  const el=container.querySelector('[data-block-id="action"] .screenplay-block') as HTMLElement;
  el.focus();
  selectAll(el);
  const notPrevented=fireEvent.keyDown(el,{key:'Backspace'});
  expect(notPrevented).toBe(true);
  expect(mergeBlock).not.toHaveBeenCalled();
});

it('无选区且光标在段首时仍按专业编辑器习惯并入上一块',async()=>{
  const mergeBlock=vi.fn();
  useEditorStore.setState({mergeBlock});
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('[data-block-id="action"] .screenplay-block')).toBeTruthy());
  const el=container.querySelector('[data-block-id="action"] .screenplay-block') as HTMLElement;
  el.focus();
  const range=document.createRange();
  range.setStart(el.firstChild!,0);
  range.collapse(true);
  const sel=window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
  const notPrevented=fireEvent.keyDown(el,{key:'Backspace'});
  expect(notPrevented).toBe(false);
  // 合并前需要先提交草稿，因此是异步调用
  await waitFor(()=>expect(mergeBlock).toHaveBeenCalled());
});

// ---------------------------------------------------------------------------
// 括注：括号是 contentEditable=false 的装饰 span，位于正文 div 之外。
// 整段选中（含括号）时选区端点不在正文里，必须显式删除。
// ---------------------------------------------------------------------------

const parText='低声说';
const setupParenthetical=()=>{
  useEditorStore.setState({
    document:{id:'d',revision:1,blocks:[
      {id:'prev',block_type:'action',text:'上一段动作。'},
      {id:'par',block_type:'parenthetical',text:parText},
    ],settings} as never,
    composing:false,zoom:1,activeBlockId:null,
  });
  mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:[
    {block_id:'prev',block_type:'action',text:'上一段动作。',start_line:0,line_count:1},
    {block_id:'par',block_type:'parenthetical',text:parText,start_line:1,line_count:1},
  ]}]}));
};

const selectShell=(container:HTMLElement)=>{
  const shell=container.querySelector('.parenthetical-shell') as HTMLElement;
  const el=shell.querySelector('.screenplay-block') as HTMLElement;
  el.focus();
  const range=document.createRange();
  range.selectNodeContents(shell);
  const sel=window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
  return {shell,el};
};

it('括注整段（含括号）选中后退格，显式清空正文且不误合并',async()=>{
  setupParenthetical();
  const changeBlockText=vi.fn(async()=>true);
  const mergeBlock=vi.fn();
  useEditorStore.setState({changeBlockText,mergeBlock});
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('.parenthetical-shell')).toBeTruthy());
  const {el}=selectShell(container);
  const notPrevented=fireEvent.keyDown(el,{key:'Backspace'});
  expect(notPrevented).toBe(false);
  expect(el.textContent).toBe('');
  await waitFor(()=>expect(changeBlockText).toHaveBeenCalledWith('par','',expect.anything()),{timeout:1500});
  expect(mergeBlock).not.toHaveBeenCalled();
});

it('括注整段选中后按 Delete 同样清空',async()=>{
  setupParenthetical();
  const changeBlockText=vi.fn(async()=>true);
  useEditorStore.setState({changeBlockText});
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('.parenthetical-shell')).toBeTruthy());
  const {el}=selectShell(container);
  expect(fireEvent.keyDown(el,{key:'Delete'})).toBe(false);
  expect(el.textContent).toBe('');
  await waitFor(()=>expect(changeBlockText).toHaveBeenCalledWith('par','',expect.anything()),{timeout:1500});
});

it('括注整段选中后 ⌘X 写入剪贴板并清空',async()=>{
  setupParenthetical();
  const changeBlockText=vi.fn(async()=>true);
  useEditorStore.setState({changeBlockText});
  const writeText=vi.fn(()=>Promise.resolve());
  Object.defineProperty(navigator,'clipboard',{value:{writeText},configurable:true});
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('.parenthetical-shell')).toBeTruthy());
  const {el}=selectShell(container);
  expect(fireEvent.keyDown(el,{key:'x',metaKey:true})).toBe(false);
  await waitFor(()=>expect(writeText).toHaveBeenCalledWith('（'+parText+'）'));
  // 剪切是事务：写剪贴板成功后才清空正文
  await waitFor(()=>expect(el.textContent).toBe(''));
  await waitFor(()=>expect(changeBlockText).toHaveBeenCalledWith('par','',expect.anything()),{timeout:1500});
});

it('括号内含括号时整段选中删除也正确',async()=>{
  const nested='他说（其实很小声）';
  useEditorStore.setState({
    document:{id:'d',revision:1,blocks:[
      {id:'prev',block_type:'action',text:'上一段动作。'},
      {id:'par',block_type:'parenthetical',text:nested},
    ],settings} as never,
    composing:false,zoom:1,activeBlockId:null,
  });
  mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:[
    {block_id:'prev',block_type:'action',text:'上一段动作。',start_line:0,line_count:1},
    {block_id:'par',block_type:'parenthetical',text:nested,start_line:1,line_count:1},
  ]}]}));
  const changeBlockText=vi.fn(async()=>true);
  useEditorStore.setState({changeBlockText});
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('.parenthetical-shell')).toBeTruthy());
  const {el}=selectShell(container);
  expect(fireEvent.keyDown(el,{key:'Backspace'})).toBe(false);
  expect(el.textContent).toBe('');
  await waitFor(()=>expect(changeBlockText).toHaveBeenCalledWith('par','',expect.anything()),{timeout:1500});
});

it('只选中括注里的半段文字时不动自定义删除逻辑',async()=>{
  setupParenthetical();
  const changeBlockText=vi.fn(async()=>true);
  useEditorStore.setState({changeBlockText});
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('.parenthetical-shell')).toBeTruthy());
  const el=container.querySelector('.parenthetical-shell .screenplay-block') as HTMLElement;
  el.focus();
  const range=document.createRange();
  range.setStart(el.firstChild!,0);
  range.setEnd(el.firstChild!,2);
  const sel=window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
  expect(fireEvent.keyDown(el,{key:'Backspace'})).toBe(true);
  expect(changeBlockText).not.toHaveBeenCalled();
});

it('选区跨到相邻块时合并首块前缀与末块后缀，并按块删除末块',async()=>{
  const nextText='下一段文字';
  useEditorStore.setState({
    document:{id:'d',revision:1,blocks:[
      {id:'action',block_type:'action',text},
      {id:'next',block_type:'action',text:nextText},
    ],settings} as never,
    composing:false,zoom:1,activeBlockId:null,
  });
  mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:[
    {block_id:'action',block_type:'action',text,start_line:0,line_count:1},
    {block_id:'next',block_type:'action',text:nextText,start_line:1,line_count:1},
  ]}]}));
  const applyBlockBatch=vi.fn(async(_batch: unknown)=>({document:null}));
  useEditorStore.setState({applyBlockBatch} as never);
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('[data-block-id="next"] .screenplay-block')).toBeTruthy());
  const first=container.querySelector('[data-block-id="action"] .screenplay-block') as HTMLElement;
  const second=container.querySelector('[data-block-id="next"] .screenplay-block') as HTMLElement;
  first.focus();
  const range=document.createRange();
  range.setStart(first.firstChild!,2);
  range.setEnd(second.firstChild!,1);
  const sel=window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
  expect(fireEvent.keyDown(first,{key:'Backspace'})).toBe(false);
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalled());
  const batch=applyBlockBatch.mock.calls[0][0] as {replace?:{blockId:string;text:string}[];delete?:string[]};
  expect(batch.replace?.[0]).toEqual({blockId:'action',text:'第一'+'一段文字'});
  expect(batch.delete).toEqual(['next']);
});
