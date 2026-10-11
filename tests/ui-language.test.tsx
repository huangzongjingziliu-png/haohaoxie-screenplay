import {afterEach,expect,it} from 'vitest';
import {cleanup,render,screen,fireEvent,act} from '@testing-library/react';
import {effectiveLanguage,setUiLanguage,getUiLanguage,tr,useUiLanguage} from '@/i18n';
import ImportDialog from '@/components/ImportDialog';
import UnifiedSettings from '@/components/UnifiedSettings';
import {useEditorStore} from '@/stores/editor';
import {mockBackend} from './setup';
afterEach(()=>{cleanup();setUiLanguage('zh');mockBackend.reset();});
it('system language resolves independently of screenplay settings',()=>{
 expect(effectiveLanguage('system','zh-TW')).toBe('zh');
 expect(effectiveLanguage('system','en-US')).toBe('en');
 expect(effectiveLanguage('system','fr-FR')).toBe('en');
 const before=useEditorStore.getState().document;
 setUiLanguage('en');expect(tr('保存')).toBe('Save');expect(getUiLanguage()).toBe('en');
 expect(localStorage.getItem('haohaoxie.ui-language.v1')).toBe('en');
 expect(useEditorStore.getState().document).toBe(before);
 expect(tr('这是用户自己的剧本标题')).toBe('这是用户自己的剧本标题');
});
it('switching the UI updates labels without dropping import formats',()=>{
 function Harness(){useUiLanguage();return <ImportDialog onClose={()=>{}}/>;}
 render(<Harness/>);expect(screen.getByRole('button',{name:'PDF（含扫描件 OCR）'})).toBeTruthy();
 act(()=>setUiLanguage('en'));
 expect(screen.getByRole('heading',{name:'Import screenplay'})).toBeTruthy();
 expect(screen.getByRole('button',{name:'PDF (including scanned pages)'})).toBeTruthy();
 expect(screen.getByRole('button',{name:'Markdown'})).toBeTruthy();
 expect(screen.getAllByRole('button',{name:'Image (OCR)'})).toHaveLength(2);
});
it('General settings changes and remembers the interface language',()=>{
 mockBackend.on('agent_list_skills',()=>[]);mockBackend.on('agent_get_config',()=>({cloudAiEnabled:false}));
 render(<UnifiedSettings initial="general" onClose={()=>{}}/>);
 fireEvent.change(screen.getByLabelText('界面语言'),{target:{value:'en'}});
 expect(screen.getByRole('heading',{name:'Language'})).toBeTruthy();
 expect(screen.getByLabelText('Interface language')).toHaveProperty('value','en');
 fireEvent.change(screen.getByLabelText('Interface language'),{target:{value:'zh'}});
 expect(screen.getByLabelText('界面语言')).toHaveProperty('value','zh');
});
