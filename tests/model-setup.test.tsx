import {beforeEach,afterEach,it,expect,vi} from 'vitest';
import {cleanup,render,screen,fireEvent,waitFor} from '@testing-library/react';
import ModelSetup from '@/components/ModelSetup';
import {mockBackend} from './setup';
import {setUiLanguage} from '@/i18n';
import {modelServices} from '@/components/ModelSetup';
beforeEach(()=>{mockBackend.reset();setUiLanguage('zh');});
afterEach(()=>{cleanup();setUiLanguage('zh');});
it('includes all thirteen reference providers alongside local and custom options',()=>{
 expect(modelServices.filter(service=>!['custom','ollama'].includes(service.id))).toHaveLength(13);
 expect(modelServices.find(service=>service.id==='ollama')?.kind).toBe('ollama');
 expect(modelServices.find(service=>service.id==='custom')?.url).toBe('');
});
it('separates regions, keeps branded cards and offers custom API even when search has no results',()=>{
 const {container}=render(<ModelSetup busy={false} onSave={vi.fn()} onCancel={vi.fn()}/>);
 expect(screen.getByText('百炼 · Alibaba Cloud')).toBeTruthy();
 expect(screen.getByText('Gemini')).toBeTruthy();
 expect(container.querySelectorAll('.model-service-card img')).toHaveLength(14);
 for(const name of ['Anthropic · Claude','OpenAI','Kimi · 月之暗面','魔搭 · ModelScope','Z.AI'])expect(screen.getByText(name)).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'中国大陆服务'}));
 expect(screen.queryByText('Gemini')).toBeNull();expect(screen.getByText('Kimi · 月之暗面')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'国际服务'}));
 expect(screen.getByText('Gemini')).toBeTruthy();expect(screen.queryByText('百炼 · Alibaba Cloud')).toBeNull();
 fireEvent.change(screen.getByLabelText('搜索模型服务'),{target:{value:'no-matching-provider'}});
 expect(screen.getByText('没有匹配的服务，换个词试试。')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:/自定义 API/}));
 expect(screen.getByLabelText('接口地址 Base URL（平台的门牌号）')).toBeTruthy();
});
it('shows all regions initially and filters free options without labelling all models free',()=>{
 render(<ModelSetup busy={false} onSave={vi.fn()} onCancel={vi.fn()}/>);
 expect(screen.getByText('SiliconFlow · 硅基流动')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'免费资源'}));
 expect(screen.queryByText('DeepSeek')).toBeNull();expect(screen.getByText('Ollama')).toBeTruthy();
});
it('international free router pre-fills its address and ID, but never supplies a shared key',()=>{
 setUiLanguage('en');const save=vi.fn();render(<ModelSetup busy={false} onSave={save} onCancel={vi.fn()}/>);
 fireEvent.click(screen.getByRole('button',{name:/OpenRouter Free models/}));
 expect((screen.getByLabelText('Base URL (the service address)') as HTMLInputElement).value).toBe('https://openrouter.ai/api/v1');
 expect((screen.getByLabelText('API Key (your access pass)') as HTMLInputElement).value).toBe('');
 fireEvent.click(screen.getByText('Enter model ID manually'));expect((screen.getByLabelText('Model ID (which assistant)') as HTMLInputElement).value).toBe('openrouter/free');
 expect(screen.getByRole('button',{name:'Save & use'}).hasAttribute('disabled')).toBe(true);expect(save).not.toHaveBeenCalled();
});
it('local model discovery saves the installed model without a key or enabling cloud processing',async()=>{
 const save=vi.fn().mockResolvedValue(undefined);mockBackend.on('agent_available_models',()=>[{id:'local:8b',free:true}]);
 render(<ModelSetup busy={false} onSave={save} onCancel={vi.fn()}/>);fireEvent.click(screen.getByRole('button',{name:/Ollama 本机免费/}));
 fireEvent.click(screen.getByText('查找本机模型'));await screen.findByRole('button',{name:/local:8b/});
 fireEvent.click(screen.getByText('保存并使用'));await waitFor(()=>expect(save).toHaveBeenCalledWith(expect.objectContaining({providerKind:'ollama',model:'local:8b',apiKey:''}),false));
 expect(mockBackend.callsTo('agent_save_model')).toHaveLength(0);
});
it('unknown-price models are excluded from the explicitly-free filter',async()=>{
 mockBackend.on('agent_available_models',()=>[{id:'confirmed',free:true},{id:'unknown-price',free:null}]);
 render(<ModelSetup busy={false} onSave={vi.fn()} onCancel={vi.fn()}/>);fireEvent.click(screen.getByRole('button',{name:/SiliconFlow/}));fireEvent.change(screen.getByLabelText('API Key（你的通行证）'),{target:{value:'test-only-key'}});
 fireEvent.click(screen.getByText('获取模型列表'));await screen.findByRole('button',{name:/unknown-price/});
 fireEvent.click(screen.getByLabelText('仅显示已标明免费的模型'));expect(screen.queryByRole('button',{name:/unknown-price/})).toBeNull();expect(screen.getByRole('button',{name:/confirmed/})).toBeTruthy();
 expect(mockBackend.callsTo('agent_test_model')).toHaveLength(0);
});
it('uses the edited OpenAI address for discovery and the saved connection',async()=>{
 mockBackend.on('agent_available_models',()=>[{id:'available-model',free:null}]);
 const save=vi.fn().mockResolvedValue(undefined);
 render(<ModelSetup busy={false} onSave={save} onCancel={vi.fn()}/>);
 fireEvent.click(screen.getByRole('button',{name:/^OpenAI /}));
 fireEvent.change(screen.getByLabelText('接口地址 Base URL（平台的门牌号）'),{target:{value:'https://example.com/v1'}});
 fireEvent.change(screen.getByLabelText('API Key（你的通行证）'),{target:{value:'test-only-key'}});
 fireEvent.click(screen.getByText('获取模型列表'));
 await screen.findByRole('button',{name:/available-model/});
 fireEvent.click(screen.getByRole('button',{name:/available-model/}));
 fireEvent.click(screen.getByText('保存并使用'));
 await waitFor(()=>expect(save).toHaveBeenCalledWith(expect.objectContaining({providerKind:'compatible',baseUrl:'https://example.com/v1',model:'available-model'}),false));
});
it('keeps a failed discovery on the connection step without persisting a model',async()=>{
 mockBackend.on('agent_available_models',()=>{throw {error:'INVALID_ARGUMENT',message:'HTTP 401'};});
 const save=vi.fn();render(<ModelSetup busy={false} onSave={save} onCancel={vi.fn()}/>);fireEvent.click(screen.getByRole('button',{name:/SiliconFlow/}));fireEvent.change(screen.getByLabelText('API Key（你的通行证）'),{target:{value:'invalid-test-key'}});fireEvent.click(screen.getByText('获取模型列表'));await waitFor(()=>expect(screen.getByRole('alert').textContent).toContain('HTTP 401'));expect(screen.getByLabelText('API Key（你的通行证）')).toBeTruthy();expect(save).not.toHaveBeenCalled();
});
