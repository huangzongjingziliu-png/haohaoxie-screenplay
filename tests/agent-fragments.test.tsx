import {afterEach,beforeEach,it,expect,vi} from 'vitest';
import {cleanup,render,fireEvent,waitFor} from '@testing-library/react';
import AgentContextMenu from '@/components/AgentContextMenu';
import {useEditorStore} from '@/stores/editor';
beforeEach(()=>{useEditorStore.setState({document:{id:'doc',revision:3,blocks:[{id:'line',block_type:'dialogue',text:'前文 后文'}]} as never,toasts:[]});window.getSelection()?.removeAllRanges();});
afterEach(cleanup);
it('sending a continuation-page fragment keeps the full dialogue as application context',async()=>{
 const receive=vi.fn();window.addEventListener('agent-selection',receive);
 try{
  const v=render(<><div data-block-id="line:part:2"><div className="screenplay-block" contentEditable suppressContentEditableWarning data-source-start="3" data-source-end="5">后文</div></div><AgentContextMenu/></>);
  fireEvent.contextMenu(v.getByText('后文'),{clientX:10,clientY:10});fireEvent.click(v.getByRole('menuitem'));
  await waitFor(()=>expect(receive).toHaveBeenCalledOnce());
  expect(receive.mock.calls[0][0].detail).toMatchObject({blockId:'line',fullText:'前文 后文',text:'后文',start:3,end:5});
 }finally{window.removeEventListener('agent-selection',receive);}
});
it('an external change between opening the menu and sending does not get blessed with a new revision',async()=>{
 const receive=vi.fn();window.addEventListener('agent-selection',receive);
 try{
  const v=render(<><div data-block-id="line"><div className="screenplay-block" contentEditable suppressContentEditableWarning>前文 后文</div></div><AgentContextMenu/></>);
  fireEvent.contextMenu(v.getByText('前文 后文'),{clientX:10,clientY:10});
  useEditorStore.setState({document:{id:'doc',revision:4,blocks:[{id:'line',text:'外部改稿'}]} as never});
  fireEvent.click(v.getByRole('menuitem'));
  await waitFor(()=>expect(useEditorStore.getState().toasts[0]?.message).toContain('已变化'));
  expect(receive).not.toHaveBeenCalled();
 }finally{window.removeEventListener('agent-selection',receive);}
});
