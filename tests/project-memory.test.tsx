import {beforeEach,afterEach,it,expect,vi} from 'vitest';
import {render,cleanup,act} from '@testing-library/react';
import {useProjectMemory} from '@/hooks/useProjectMemory';
import {useEditorStore} from '@/stores/editor';
import {mockBackend} from './setup';
function Harness(){useProjectMemory();return null;}
beforeEach(()=>{vi.useFakeTimers();mockBackend.reset();useEditorStore.setState({sourceText:'正文',document:{id:'memory-project',settings:{project_memory:{auto_enabled:true}}} as never});mockBackend.on('agent_list_models',()=>({activeId:'model-a',profiles:[]}));mockBackend.on('agent_refresh_memory',()=>null);});
afterEach(()=>{cleanup();vi.useRealTimers();});
it('automatic memory waits for an editing pause and does not recur on memory-only changes',async()=>{
 render(<Harness/>);await act(async()=>{await vi.advanceTimersByTimeAsync(19999);});expect(mockBackend.callsTo('agent_refresh_memory')).toHaveLength(0);
 await act(async()=>{await vi.advanceTimersByTimeAsync(1);});expect(mockBackend.callsTo('agent_refresh_memory')).toHaveLength(1);
 await act(async()=>{const doc=useEditorStore.getState().document!;useEditorStore.setState({document:{...doc,settings:{...doc.settings,project_memory:{...doc.settings.project_memory,markdown:'整理结果'}}}});await vi.advanceTimersByTimeAsync(30000);});expect(mockBackend.callsTo('agent_refresh_memory')).toHaveLength(1);
});
it('without a configured model no memory request is made',async()=>{
 mockBackend.on('agent_list_models',()=>({activeId:'',profiles:[]}));render(<Harness/>);await act(async()=>{await vi.advanceTimersByTimeAsync(20000);});expect(mockBackend.callsTo('agent_refresh_memory')).toHaveLength(0);
});
it('successful discussion triggers project organization without writing personalization',async()=>{
 render(<Harness/>);await act(async()=>{window.dispatchEvent(new CustomEvent('project-memory-discussion',{detail:'建议（未确认）：增加冲突'}));await vi.advanceTimersByTimeAsync(0);});expect(mockBackend.callsTo('agent_refresh_memory')[0].args).toMatchObject({projectId:'memory-project',discussion:'建议（未确认）：增加冲突',force:false});expect(mockBackend.callsTo('agent_save_memory')).toHaveLength(0);
});
