import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,waitFor} from '@testing-library/react';
import ScreenplayEditor from '@/editors/screenplay/ScreenplayEditor';
import {useEditorStore} from '@/stores/editor';
import {registerPendingEditor} from '@/utils/pendingEdits';
import {mockBackend} from './setup';

const settings={typography:'chinese-reading',font_family:'Songti SC',font_size:14,line_height:1.6,page_width_inch:8.27,page_height_inch:11.69};
const setup=(text:string)=>{
  useEditorStore.setState({document:{id:'d',revision:1,blocks:[{id:'a',block_type:'action',text}],settings} as never,composing:false,zoom:1,activeBlockId:null});
  mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:[{block_id:'a',block_type:'action',text,start_line:0,line_count:1}]}]}));
};
const caretAt=(el:HTMLElement,offset:number)=>{
  const range=document.createRange();
  range.setStart(el.firstChild!,offset);
  range.collapse(true);
  const sel=window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
};
const selectRange=(el:HTMLElement,start:number,end:number)=>{
  const range=document.createRange();
  range.setStart(el.firstChild!,start);
  range.setEnd(el.firstChild!,end);
  const sel=window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
};

beforeEach(()=>{
  mockBackend.reset();
  vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});
});
afterEach(()=>{cleanup();vi.unstubAllGlobals();window.getSelection()?.removeAllRanges();});

it('段首退格合并前先提交防抖中的草稿，避免刚输入的文字丢失',async()=>{
  const blocks=[{id:'prev',block_type:'action',text:'上一段动作。'},{id:'a',block_type:'action',text:'第二段'}];
  useEditorStore.setState({document:{id:'d',revision:1,blocks,settings} as never,composing:false,zoom:1,activeBlockId:null});
  mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:[{block_id:'prev',block_type:'action',text:'上一段动作。',start_line:0,line_count:1},{block_id:'a',block_type:'action',text:'第二段',start_line:1,line_count:1}]}]}));
  const order:string[]=[];
  const off=registerPendingEditor(async()=>{order.push('flush');});
  const mergeBlock=vi.fn(async()=>{order.push('merge');});
  useEditorStore.setState({mergeBlock});
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('[data-block-id="a"] .screenplay-block')).toBeTruthy());
  const el=container.querySelector('[data-block-id="a"] .screenplay-block') as HTMLElement;
  el.focus();
  caretAt(el,0);
  fireEvent.keyDown(el,{key:'Backspace'});
  await waitFor(()=>expect(order).toEqual(['flush','merge']));
  off();
});

it('段落中间粘贴多行：光标前的留在本段，原段尾跟随最后一行',async()=>{
  setup('前半后半');
  const applyBlockBatch=vi.fn(async()=>({affected:['a','new1']}));
  const changeBlockText=vi.fn(async()=>true);
  const insertBlock=vi.fn(async()=>{});
  useEditorStore.setState({applyBlockBatch,changeBlockText,insertBlock} as never);
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('[data-block-id="a"] .screenplay-block')).toBeTruthy());
  const el=container.querySelector('[data-block-id="a"] .screenplay-block') as HTMLElement;
  el.focus();
  caretAt(el,2);
  fireEvent.paste(el,{clipboardData:{getData:()=>'X\nY'}});
  // 整次粘贴只提交一个批次：替换首行 + 插入其余行同属一个事务。
  await waitFor(()=>expect(applyBlockBatch).toHaveBeenCalledTimes(1));
  expect(applyBlockBatch).toHaveBeenCalledWith({
    replace:[{blockId:'a',text:'前半X'}],
    insertAfter:'a',
    insert:[{blockType:'action',text:'Y后半'}],
  });
  // 不再逐行调用旧的单块入口，失败时也不会留下半截状态。
  expect(changeBlockText).not.toHaveBeenCalled();
  expect(insertBlock).not.toHaveBeenCalled();
});

it('选中文字后按回车：先删除选区再拆分，选中文字不得留在原位',async()=>{
  setup('你好世界');
  const changeBlockText=vi.fn(async()=>true);
  const insertBlock=vi.fn(async()=>{useEditorStore.setState({activeBlockId:'inserted'});});
  useEditorStore.setState({changeBlockText,insertBlock});
  const {container}=render(<ScreenplayEditor/>);
  await waitFor(()=>expect(container.querySelector('[data-block-id="a"] .screenplay-block')).toBeTruthy());
  const el=container.querySelector('[data-block-id="a"] .screenplay-block') as HTMLElement;
  el.focus();
  selectRange(el,1,3);
  fireEvent.keyDown(el,{key:'Enter'});
  await waitFor(()=>expect(insertBlock).toHaveBeenCalled());
  expect(changeBlockText).toHaveBeenCalledWith('a','你');
  expect(insertBlock).toHaveBeenCalledWith('a',expect.any(String),'界');
});
