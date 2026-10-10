import {afterEach,beforeEach,it,expect} from 'vitest';
import {cleanup,render,fireEvent,waitFor} from '@testing-library/react';
import ModelPicker from '@/components/ModelPicker';
import {mockBackend} from './setup';
beforeEach(()=>{mockBackend.reset();mockBackend.on('agent_list_models',()=>({activeId:'',profiles:[]}));mockBackend.on('agent_save_model',()=>({activeId:'a',profiles:[]}));});afterEach(cleanup);
it('模型管理保存新密钥后销毁密码输入框',async()=>{const view=render(<ModelPicker manage/>);fireEvent.click(view.getByText('管理自定义模型…'));fireEvent.click(view.getByText('＋ 添加自定义模型'));fireEvent.change(view.getByLabelText('显示名称'),{target:{value:'测试模型'}});fireEvent.change(view.getByLabelText('模型 ID'),{target:{value:'model'}});fireEvent.change(view.getByLabelText('服务地址'),{target:{value:'https://example.com/v1'}});fireEvent.change(view.getByLabelText('API Key'),{target:{value:'test-only-secret'}});fireEvent.click(view.getByText('保存并使用'));await waitFor(()=>expect(mockBackend.callsTo('agent_save_model')).toHaveLength(1));expect(mockBackend.callsTo('agent_save_model')[0].args?.profile).toMatchObject({apiKey:'test-only-secret'});await waitFor(()=>expect(view.queryByLabelText('API Key')).toBeNull());});
