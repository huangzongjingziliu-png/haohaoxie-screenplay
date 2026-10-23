import {beforeEach,afterEach,it,expect,vi} from 'vitest';
import {cleanup,render,screen,fireEvent,waitFor} from '@testing-library/react';
import ScreenplayEditor from '@/editors/screenplay/ScreenplayEditor';
import {useEditorStore} from '@/stores/editor';
import {mockBackend} from './setup';
beforeEach(()=>{
 mockBackend.reset();
 vi.stubGlobal('ResizeObserver',class {observe(){} disconnect(){}});
 useEditorStore.setState({document:{id:'empty',revision:0,blocks:[],settings:{}} as never,zoom:1,activeBlockId:null});
 mockBackend.on('get_pagination',()=>({pages:[{number:1,blocks:[],show_page_number:false}],profile:{}}));
});
afterEach(()=>{cleanup();vi.unstubAllGlobals();});
it('空白纸面提供起笔入口，点击才创建首段，连续点击不会重复创建',async()=>{
 let resolve!:(r:unknown)=>void;
 mockBackend.on('insert_block',()=>new Promise(r=>{resolve=r;}));
 render(<ScreenplayEditor/>);
 const start=await screen.findByRole('button',{name:'开始书写剧本'});
 expect(mockBackend.callsTo('insert_block')).toHaveLength(0);
 fireEvent.click(start);fireEvent.click(start);
 await waitFor(()=>expect(mockBackend.callsTo('insert_block')).toHaveLength(1));
 expect(mockBackend.callsTo('insert_block')[0].args).toMatchObject({afterId:null});
 resolve(Promise.reject(new Error('测试失败')));
 await waitFor(()=>expect(useEditorStore.getState().toasts.some(t=>String(t.message).includes('测试失败'))).toBe(true));
 fireEvent.click(start);
 await waitFor(()=>expect(mockBackend.callsTo('insert_block')).toHaveLength(2));
});
