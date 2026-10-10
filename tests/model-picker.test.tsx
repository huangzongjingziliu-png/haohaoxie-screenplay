import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import ModelPicker from '@/components/ModelPicker';
import { mockBackend } from './setup';
const profiles=[
 {id:'a',name:'Agnes',providerKind:'compatible',model:'agnes-3.0-flash',baseUrl:'https://example.com/v1'},
 {id:'b',name:'第二个模型',providerKind:'compatible',model:'second',baseUrl:'https://example.net/v1'},
];
beforeEach(()=>{
 mockBackend.reset();
 let active='a';
 mockBackend.on('agent_list_models',()=>({activeId:active,profiles}));
 mockBackend.on('agent_select_model',args=>{active=args!.id as string;return {activeId:active,profiles};});
 mockBackend.on('agent_save_model',()=>({activeId:active,profiles}));
});
afterEach(cleanup);
it('右下角显示当前模型并能切换到另一个保存的模型',async()=>{
 render(<ModelPicker/>);
 await waitFor(()=>expect((screen.getByLabelText('选择模型') as HTMLSelectElement).value).toBe('a'));
 fireEvent.change(screen.getByLabelText('选择模型'),{target:{value:'b'}});
 await waitFor(()=>expect((screen.getByLabelText('选择模型') as HTMLSelectElement).value).toBe('b'));
 expect(mockBackend.callsTo('agent_select_model')[0].args?.id).toBe('b');
});
it('编辑模型不显示已有密钥，留空保留原密钥',async()=>{
 render(<ModelPicker manage/>);
 await waitFor(()=>expect(mockBackend.callsTo('agent_list_models')).toHaveLength(1));
 fireEvent.click(screen.getByText('管理自定义模型…'));
 fireEvent.click(screen.getAllByRole('button',{name:'编辑'})[0]);
 expect((screen.getByPlaceholderText('已保存，留空保留原密钥') as HTMLInputElement).value).toBe('');
 fireEvent.click(screen.getByRole('button',{name:'保存并使用'}));
 await waitFor(()=>expect(mockBackend.callsTo('agent_save_model')).toHaveLength(1));
 expect((mockBackend.callsTo('agent_save_model')[0].args?.profile as {apiKey:string}).apiKey).toBe('');
});
