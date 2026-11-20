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
