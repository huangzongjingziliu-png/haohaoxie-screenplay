import {render,screen,fireEvent,cleanup} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import HelpPanel from '@/components/HelpPanel';
import zh from '@/help/zh.json';
import en from '@/help/en.json';
import {setUiLanguage} from '@/i18n';
vi.mock('@/ipc/client',()=>({ipc:{openHelpLink:vi.fn().mockResolvedValue(undefined)}}));
afterEach(()=>{cleanup();setUiLanguage('zh');});
it('keeps both guides complete and English contacts email-only',()=>{expect(zh.map(s=>s.id)).toEqual(en.map(s=>s.id));expect(JSON.stringify(en)).not.toContain('19995145710');expect(JSON.stringify(en)).not.toContain('851194703');expect(JSON.stringify(en)).toContain('huangzongjing.ziliu@gmail.com');});
it('searches the guide and opens contextual model settings',()=>{setUiLanguage('zh');render(<HelpPanel initial="models" onClose={vi.fn()}/>);const listener=vi.fn();window.addEventListener('open-model-settings',listener);fireEvent.click(screen.getByText('去配置自定义模型'));expect(listener).toHaveBeenCalledOnce();window.removeEventListener('open-model-settings',listener);fireEvent.change(screen.getByLabelText('搜索教程'),{target:{value:'401'}});expect(screen.getByRole('heading',{name:'遇到问题怎么办'})).toBeTruthy();});
it('switches guide language without changing app language and closes on Escape',()=>{setUiLanguage('zh');const close=vi.fn();render(<HelpPanel initial="contact" onClose={close}/>);fireEvent.change(screen.getByLabelText('教程语言'),{target:{value:'en'}});expect(screen.getByText('huangzongjing.ziliu@gmail.com')).toBeTruthy();expect(screen.queryByText('19995145710')).toBeNull();fireEvent.keyDown(screen.getByRole('dialog'),{key:'Escape'});expect(close).toHaveBeenCalledOnce();});

it('recommends domestic cloud platforms in Chinese and international ones in English',()=>{const chinese=zh.find(s=>s.id==='models')!;const english=en.find(s=>s.id==='models')!;expect(chinese.providers!.map(p=>p.name)).toEqual(['DeepSeek','硅基流动 SiliconFlow','本机 Ollama']);expect(english.providers!.map(p=>p.name)).toEqual(['OpenAI','OpenRouter','Groq','Local Ollama']);expect(JSON.stringify(zh)).toContain('ziliu_huangzongjing');expect(JSON.stringify(zh)).not.toContain('19995145710');});
