import {beforeEach,afterEach,expect,it,vi} from 'vitest';
import {act,cleanup,fireEvent,render,waitFor} from '@testing-library/react';
import ScreenplayEditor from '@/editors/screenplay/ScreenplayEditor';
import {useEditorStore} from '@/stores/editor';
import {flushPendingEdits} from '@/utils/pendingEdits';
import {fragmentContext,sourceBlockId} from '@/utils/pageFragments';
import {mockBackend} from './setup';
const original='第一段😀 第二段对白。';
const cut=original.indexOf('第二');
const settings={typography:'chinese-reading',font_family:'Songti SC',font_size:14,line_height:1.6,page_width_inch:8.27,page_height_inch:11.69};
beforeEach(()=>{mockBackend.reset();vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});useEditorStore.setState({document:{id:'d',revision:1,blocks:[{id:'dialogue',block_type:'dialogue',text:original}],settings} as never,composing:false,zoom:1,activeBlockId:null});
 mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:[{block_id:'dialogue',block_type:'dialogue',text:original.slice(0,cut),source_start:0,source_end:cut,start_line:0,line_count:1}]},{number:2,blocks:[{block_id:'dialogue:contd:2',block_type:'character',text:"人物 (CONT'D)",contd:true,start_line:0,line_count:1},{block_id:'dialogue:part:2',block_type:'dialogue',text:original.slice(cut),source_start:cut,source_end:original.length,start_line:2,line_count:1}]}]}));
});
afterEach(()=>{cleanup();vi.unstubAllGlobals();});
it('editing the first page preserves the unseen remainder',async()=>{
 const change=vi.fn().mockResolvedValue(true);useEditorStore.setState({changeBlockText:change});
 const {container}=render(<ScreenplayEditor/>);
 await waitFor(()=>expect(container.querySelector('[data-block-id="dialogue"] .screenplay-block')).toBeTruthy());
 const el=container.querySelector('[data-block-id="dialogue"] .screenplay-block') as HTMLElement;
 el.focus();el.textContent='改了开头😀 ';fireEvent.input(el);
 await act(()=>flushPendingEdits());
 expect(change).toHaveBeenCalledWith('dialogue','改了开头😀 '+original.slice(cut),1);
});
it('editing a continuation page targets the real block and preserves the beginning',async()=>{
 const change=vi.fn().mockResolvedValue(true);useEditorStore.setState({changeBlockText:change});
 const {container}=render(<ScreenplayEditor/>);
 await waitFor(()=>expect(container.querySelector('[data-block-id="dialogue:part:2"] .screenplay-block')).toBeTruthy());
 const el=container.querySelector('[data-block-id="dialogue:part:2"] .screenplay-block') as HTMLElement;
 el.focus();el.textContent='改了结尾。';fireEvent.input(el);
 await act(()=>flushPendingEdits());
 expect(change).toHaveBeenCalledWith('dialogue',original.slice(0,cut)+'改了结尾。',1);
 expect(container.querySelector('[data-block-id="dialogue:contd:2"] .screenplay-block')?.getAttribute('contenteditable')).toBe('false');
});
it('fragment offsets use JS UTF-16 boundaries and reject stale invalid ranges',()=>{
 expect(fragmentContext(original,cut,original.length).prefix).toBe('第一段😀 ');
 expect(()=>fragmentContext(original,0,original.length+1)).toThrow();
 expect(sourceBlockId('dialogue:part:2')).toBe('dialogue');
});
it('two consecutive edits retain the original hidden suffix while pagination is catching up',async()=>{
 const change=vi.fn(async(_id:string,text:string)=>{const d=useEditorStore.getState().document!;useEditorStore.setState({document:{...d,revision:d.revision+1,blocks:[{...d.blocks[0],text}]}});return true;});
 useEditorStore.setState({changeBlockText:change});
 const {container}=render(<ScreenplayEditor/>);await waitFor(()=>expect(container.querySelector('[data-block-id="dialogue"] .screenplay-block')).toBeTruthy());
 const el=container.querySelector('[data-block-id="dialogue"] .screenplay-block') as HTMLElement;
 el.focus();el.textContent='第一次修改加长了😀 ';fireEvent.input(el);await act(()=>flushPendingEdits());
 el.textContent='第二次继续加长了😀 ';fireEvent.input(el);await act(()=>flushPendingEdits());
 expect(change.mock.calls[1][1]).toBe('第二次继续加长了😀 '+original.slice(cut));
});
