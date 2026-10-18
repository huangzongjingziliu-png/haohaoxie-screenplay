import {beforeEach,afterEach,it,expect,vi} from 'vitest';
import {render,screen,fireEvent,waitFor,cleanup} from '@testing-library/react';
import VersionPanel from '@/components/VersionPanel';
import {useEditorStore} from '@/stores/editor';
import type {ScreenplayDocument} from '@/ipc/types';
import {mockBackend} from './setup';
const versions=[{id:'old',name:'第一稿',note:'原版结尾',created_at:'2026-10-18T00:00:00Z',revision:1,auto:false,origin:'gui-left'},{id:'new',name:'自动检查点',note:'',created_at:'2026-10-18T01:00:00Z',revision:2,auto:true,origin:'system'}];
beforeEach(()=>{mockBackend.reset();useEditorStore.setState({document:{id:'project-a',revision:3} as ScreenplayDocument,applyResult:vi.fn().mockResolvedValue(undefined)});mockBackend.on('list_versions',()=>versions);mockBackend.on('list_branches',()=>[]);mockBackend.on('current_branch',()=> 'main');mockBackend.on('audit_log',()=>[]);mockBackend.on('preview_version',()=> '这是以前的结尾');});
afterEach(()=>{cleanup();vi.restoreAllMocks();});
it('preview reads an old draft without applying it or sending a restore command',async()=>{
 render(<VersionPanel onClose={vi.fn()}/>);await screen.findByText('原版结尾');fireEvent.click(screen.getAllByRole('button',{name:'预览旧稿'})[1]);await screen.findByText('这是以前的结尾');expect(mockBackend.callsTo('restore_version')).toHaveLength(0);expect(useEditorStore.getState().document?.revision).toBe(3);
});
it('restoration requires confirmation and carries the project identity',async()=>{
 vi.spyOn(window,'confirm').mockReturnValue(true);mockBackend.on('restore_version',()=>({revision:4}));render(<VersionPanel onClose={vi.fn()}/>);await screen.findByText('原版结尾');fireEvent.click(screen.getAllByRole('button',{name:'预览旧稿'})[1]);await screen.findByText('这是以前的结尾');fireEvent.click(screen.getByText('恢复到这一稿'));await waitFor(()=>expect(mockBackend.callsTo('restore_version')).toHaveLength(1));expect(mockBackend.callsTo('restore_version')[0].args).toMatchObject({versionId:'old',projectId:'project-a'});
});
it('a cancelled restoration does not modify the project',async()=>{
 vi.spyOn(window,'confirm').mockReturnValue(false);render(<VersionPanel onClose={vi.fn()}/>);await screen.findByText('原版结尾');fireEvent.click(screen.getAllByRole('button',{name:'预览旧稿'})[0]);await screen.findByText('这是以前的结尾');fireEvent.click(screen.getByText('恢复到这一稿'));expect(mockBackend.callsTo('restore_version')).toHaveLength(0);
});
