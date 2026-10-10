import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import ImportDialog from '@/components/ImportDialog';
import { useEditorStore } from '@/stores/editor';
import { mockBackend } from './setup';

const mocks = vi.hoisted(() => ({ open: vi.fn(), confirm: vi.fn(), listener: undefined as undefined | ((e: { payload: unknown }) => void), unlisten: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.open, confirm: mocks.confirm, save: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: async (_: string, cb: (e: { payload: unknown }) => void) => { mocks.listener = cb; return mocks.unlisten; } }));
const applyResult = vi.fn();
beforeEach(() => {
  mockBackend.reset(); mocks.open.mockResolvedValue('/tmp/sample.pdf'); mocks.confirm.mockResolvedValue(false); mocks.unlisten.mockClear(); applyResult.mockClear();
  useEditorStore.setState({ project: { state: 'open', path: '/tmp/project.screenplay', title: '当前剧本', revision: 2, dirty: false, blockCount: 1 }, applyResult, toasts: [] });
});
afterEach(cleanup);

it('取消后保留当前剧本，忽略其他任务进度，解除监听', async () => {
  let rejectImport!: (reason: unknown) => void;
  mockBackend.on('import_file', () => new Promise((_, reject) => { rejectImport = reject; }));
  mockBackend.on('cancel_import', () => { rejectImport({ error: 'APP_STATE', message: '导入已取消' }); return true; });
  const close = vi.fn();
  render(<ImportDialog onClose={close} />);
  fireEvent.click(screen.getByRole('button', { name: 'PDF（含扫描件 OCR）' }));
  await waitFor(() => expect(mockBackend.callsTo('import_file')).toHaveLength(1));
  const id = mockBackend.callsTo('import_file')[0].args!.requestId;
  act(() => mocks.listener?.({ payload: { requestId: 'other', stage: 'ocr', completed: 0, total: 0 } }));
  expect(screen.queryByText('正在识别扫描文字…')).toBeNull();
  act(() => mocks.listener?.({ payload: { requestId: id, stage: 'credential', completed: 0, total: 0 } }));
  expect(screen.getByText(/正在检查模型配置/)).toBeTruthy();
  act(() => mocks.listener?.({ payload: { requestId: id, stage: 'pdf_pages', completed: 3, total: 10 } }));
  expect(screen.getByText('3 / 10 页')).toBeTruthy();
  expect(screen.getByRole('dialog',{name:'Agent工作状态'})).toBeTruthy();
  act(()=>mocks.listener?.({payload:{requestId:id,stage:'agent',completed:2,total:40}}));
  expect(screen.getByText('2 / 40 段')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '取消导入' }));
  await waitFor(() => expect(screen.getByRole('button', { name: '关闭' })).toBeTruthy());
  expect(applyResult).not.toHaveBeenCalled();
  expect(close).not.toHaveBeenCalled();
  expect(useEditorStore.getState().project.title).toBe('当前剧本');
  expect(useEditorStore.getState().toasts.some(t => t.message.includes('当前剧本未改变'))).toBe(true);
  expect(mocks.unlisten).toHaveBeenCalledTimes(1);
});

it('拒绝保存当前修改时不启动导入', async () => {
  useEditorStore.setState({ project: { ...useEditorStore.getState().project, dirty: true } });
  render(<ImportDialog onClose={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: 'PDF（含扫描件 OCR）' }));
  await waitFor(() => expect(mocks.confirm).toHaveBeenCalled());
  expect(mockBackend.callsTo('import_file')).toHaveLength(0);
});

it('成功导入后自动打开原文和低置信度校对入口', async () => {
  const dispatch = vi.spyOn(window, 'dispatchEvent');
  mockBackend.on('import_file', () => ({
    document: { id: 'doc2', revision: 4, blocks: [{ id: 'b', text: '疑似对白', block_type: 'action', confidence: 0.4 }], import_info: { source_type: 'txt', source_name: 'rough.txt', imported_at: '', original_text: '原稿', removed_noise: [], preserve_original: true } },
    source_text: '疑似对白', revision: 4,
  }));
  render(<ImportDialog onClose={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: '纯文本' }));
  await waitFor(() => expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: 'import-review-requested' })));
  expect(applyResult).toHaveBeenCalled();
  dispatch.mockRestore();
});

it('额度耗尽显示续转说明，重试空白新项目无需保存空文件', async () => {
  mocks.confirm.mockClear();
  useEditorStore.setState({project:{...useEditorStore.getState().project,path:null,dirty:true,blockCount:0}});
  mockBackend.on('import_file',()=>{throw {error:'INTERNAL',message:'免费模型请求额度已达上限。当前剧本未改变。'};});
  render(<ImportDialog onClose={vi.fn()}/>);
  fireEvent.click(screen.getByRole('button',{name:'PDF（含扫描件 OCR）'}));
  await waitFor(()=>expect(screen.getByText(/保持好好写剧本打开/)).toBeTruthy());
  expect(mocks.confirm).not.toHaveBeenCalled();
  expect(applyResult).not.toHaveBeenCalled();
});
