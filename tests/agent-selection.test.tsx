import {fireEvent,render,waitFor,cleanup} from '@testing-library/react';
import {afterEach,beforeEach,it,expect,vi} from 'vitest';
import AgentSelectionPanel from '@/components/AgentSelectionPanel';
import {useEditorStore} from '@/stores/editor';import {mockBackend} from './setup';
beforeEach(()=>{mockBackend.reset();useEditorStore.setState({document:{id:'qa',revision:3} as never,applyResult:vi.fn()});});afterEach(cleanup);
it('局部修改仅应用所选段落，必须先预览',async()=>{
 mockBackend.on('agent_transform_text',()=> '新对白');mockBackend.on('agent_apply_text',()=>({document:{id:'qa',revision:4},revision:4}));const close=vi.fn();
 const v=render(<AgentSelectionPanel target={{projectId:'qa',revision:3,label:'对白',text:'原句',blockId:'line',fullText:'前缀原句后缀',start:2,end:4}} onClose={close} onConfigure={()=>{}}/>);
 fireEvent.click(v.getByText('生成预览'));await waitFor(()=>expect(v.getByLabelText('局部修改结果')).toBeTruthy());expect(mockBackend.callsTo('agent_apply_text')).toHaveLength(0);
 fireEvent.click(v.getByText('应用到所选范围'));await waitFor(()=>expect(close).toHaveBeenCalled());expect(mockBackend.callsTo('agent_apply_text')[0].args).toMatchObject({blockId:'line',text:'前缀新对白后缀',expectedRevision:3});
});
it('预览后版本变化拒绝覆盖其他人的修改',async()=>{mockBackend.on('agent_transform_text',()=> '修改');const v=render(<AgentSelectionPanel target={{projectId:'qa',revision:3,label:'场景',text:'原文',blockIds:['s','b']}} onClose={()=>{}} onConfigure={()=>{}}/>);fireEvent.click(v.getByText('生成预览'));await waitFor(()=>expect(v.getByLabelText('局部修改结果')).toBeTruthy());useEditorStore.setState({document:{id:'qa',revision:4} as never});fireEvent.click(v.getByText('应用到所选范围'));await waitFor(()=>expect(v.getByRole('alert').textContent).toContain('发生了变化'));expect(mockBackend.callsTo('agent_apply_selection')).toHaveLength(0);});
