import {afterEach,beforeEach,it,expect} from 'vitest';
import {cleanup,render,screen,fireEvent,waitFor} from '@testing-library/react';
import ModelRequiredDialog from '@/components/ModelRequiredDialog';
import {ipc} from '@/ipc/client';
import {mockBackend} from './setup';
beforeEach(()=>{mockBackend.reset();mockBackend.on('agent_list_models',()=>({activeId:'',profiles:[]}));});afterEach(cleanup);
it('缺少模型显示提醒并直接打开模型管理，不修改原稿',async()=>{
 render(<ModelRequiredDialog/>);mockBackend.on('agent_generate_skill',()=>{throw {error:'MODEL_REQUIRED',message:'请配置模型'};});
 await expect(ipc.agentGenerateSkill('检查对白')).rejects.toThrow('请配置模型');
 await screen.findByRole('dialog',{name:'需要配置模型'});fireEvent.click(screen.getByText('去配置模型'));
 await screen.findByRole('dialog',{name:'自定义模型'});expect(mockBackend.callsTo('replace_source_text')).toHaveLength(0);
});
it('自动项目记忆不弹窗打断写作，手动整理会提醒',async()=>{
 render(<ModelRequiredDialog/>);mockBackend.on('agent_refresh_memory',()=>{throw {error:'MODEL_REQUIRED',message:'请配置模型'};});
 await expect(ipc.agentRefreshMemory('a')).rejects.toThrow();expect(screen.queryByRole('dialog',{name:'需要配置模型'})).toBeNull();
 await expect(ipc.agentRefreshMemory('a',undefined,true)).rejects.toThrow();await screen.findByRole('dialog',{name:'需要配置模型'});
});
