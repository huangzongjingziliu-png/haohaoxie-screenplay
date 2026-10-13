import {render,screen,fireEvent,waitFor,cleanup} from '@testing-library/react';
import {beforeEach,afterEach,it,expect,vi} from 'vitest';
import EpisodeControls from '@/components/EpisodeControls';
import {useEditorStore} from '@/stores/editor';
import {mockBackend} from './setup';
import type {ScreenplayDocument} from '@/ipc/types';
vi.mock('@/utils/pendingEdits',()=>({flushPendingEdits:vi.fn().mockResolvedValue(undefined)}));
const doc=()=>({id:'project',revision:9,metadata:{title:'剧集',author:'',language:'zh'},blocks:[],scenes:[],characters:[],locations:[],series:{active_id:'ep2',episodes:[{id:'ep1',title:'第一集',draft:null},{id:'ep2',title:'第二集',draft:null}]}} as unknown as ScreenplayDocument);
beforeEach(()=>{mockBackend.reset();useEditorStore.setState({document:doc(),applyResult:vi.fn().mockResolvedValue(undefined)});mockBackend.on('edit_series',a=>({revision:10,document:doc(),source_text:'',affected:[],pagination:{pages:[]}}));});afterEach(cleanup);
it('switches using the current revision after flushing pending edits',async()=>{render(<EpisodeControls/>);fireEvent.change(screen.getByLabelText('当前分集'),{target:{value:'ep1'}});await waitFor(()=>expect(mockBackend.callsTo('edit_series')).toHaveLength(1));expect(mockBackend.callsTo('edit_series')[0].args).toMatchObject({action:'switch',id:'ep1',expectedRevision:9});});
it('shows all requested export formats and shows a Done button',()=>{render(<EpisodeControls/>);fireEvent.click(screen.getByText('分集'));expect(screen.getByRole('dialog',{name:'分集管理'})).toBeTruthy();const select=screen.getByLabelText('分集导出格式');expect([...select.querySelectorAll('option')].map(o=>o.value)).toEqual(['pdf','docx','md','fdx','fountain','txt','html']);expect(screen.getByRole('button',{name:'完成'})).toBeTruthy();});
it('offers a safe conversion for a legacy single screenplay',()=>{const legacy=doc();delete legacy.series;useEditorStore.setState({document:legacy});render(<EpisodeControls/>);fireEvent.click(screen.getByText('分集'));expect(screen.getByText('把现有剧本作为第 1 集，原有内容会保留。')).toBeTruthy();fireEvent.click(screen.getByText('启用分集'));});

it('asks before discarding an unsubmitted episode name on Done',()=>{render(<EpisodeControls/>);fireEvent.click(screen.getByText('分集'));fireEvent.change(screen.getByLabelText('分集名称'),{target:{value:'未提交名称'}});fireEvent.click(screen.getByRole('button',{name:'完成'}));expect(screen.getByRole('alertdialog',{name:'确认关闭'})).toBeTruthy();fireEvent.click(screen.getByRole('button',{name:'取消'}));expect(screen.getByRole('dialog',{name:'分集管理'})).toBeTruthy();fireEvent.click(screen.getByRole('button',{name:'完成'}));fireEvent.click(screen.getByRole('button',{name:'确认'}));expect(screen.queryByRole('dialog',{name:'分集管理'})).toBeNull();expect(mockBackend.callsTo('edit_series')).toHaveLength(0);});
