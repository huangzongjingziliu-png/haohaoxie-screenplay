import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import SourceEditor from '@/editors/source/SourceEditor';
import RecoveryPrompt from '@/components/RecoveryPrompt';
import { useEditorStore } from '@/stores/editor';
import { flushPendingEdits } from '@/utils/pendingEdits';
import { mockBackend } from './setup';

beforeEach(() => { mockBackend.reset(); useEditorStore.setState({ sourceText: '原稿', document: { id: 'doc', revision: 1 } as never, composing: false, toasts: [] }); });
afterEach(() => cleanup());

describe('待提交文字安全', () => {
  it('保存前立即提交尚在防抖中的文字', async () => {
    const change = vi.fn().mockResolvedValue(true);
    useEditorStore.setState({ changeSourceText: change });
    render(<SourceEditor />);
    fireEvent.input(screen.getByRole('textbox'), { target: { value: '最后一句' } });
    await act(() => flushPendingEdits());
    expect(change).toHaveBeenCalledWith('最后一句', expect.anything());
  });
  it('提交期间又输入文字，保存仍等待最后一句提交', async () => {
    let finish!: (ok: boolean) => void;
    const change = vi.fn().mockImplementationOnce(() => new Promise<boolean>((resolve) => { finish = resolve; })).mockResolvedValue(true);
    useEditorStore.setState({ changeSourceText: change });
    render(<SourceEditor />);
    const input = screen.getByRole('textbox');
    fireEvent.input(input, { target: { value: '第一句' } });
    const pending = flushPendingEdits();
    fireEvent.input(input, { target: { value: '第一句。最后一句。' } });
    finish(true);
    await act(() => pending);
    expect(change).toHaveBeenCalledTimes(2);
    expect(change.mock.calls[1][0]).toBe('第一句。最后一句。');
  });
  it('失败后保留输入且允许重试，不能当作已经提交', async () => {
    const change = vi.fn().mockResolvedValueOnce(false).mockResolvedValue(true);
    useEditorStore.setState({ changeSourceText: change });
    render(<SourceEditor />);
    const input = screen.getByRole('textbox') as HTMLTextAreaElement;
    fireEvent.input(input, { target: { value: '不可丢失' } });
    await expect(flushPendingEdits()).rejects.toThrow('尚未提交');
    expect(input.value).toBe('不可丢失');
    await flushPendingEdits();
    expect(change).toHaveBeenCalledTimes(2);
  });
  it('聚焦但没有本地修改时仍同步外部更新', () => {
    render(<SourceEditor />);
    const input = screen.getByRole('textbox') as HTMLTextAreaElement;
    input.focus();
    act(() => useEditorStore.setState({ sourceText: 'MCP 更新' }));
    expect(input.value).toBe('MCP 更新');
  });
  it('输入期间收到外部更新仍用原 Revision 提交，避免覆盖外部修改', async () => {
    const change = vi.fn().mockResolvedValue(true);
    useEditorStore.setState({ changeSourceText: change });
    render(<SourceEditor />);
    fireEvent.input(screen.getByRole('textbox'), { target: { value: '我的草稿' } });
    act(() => useEditorStore.setState({ sourceText: '外部新稿', document: { id: 'doc', revision: 2 } as never }));
    await flushPendingEdits();
    expect(change).toHaveBeenCalledWith('我的草稿', 1);
  });
  it('组字期间拒绝保存，保留当前输入', async () => {
    render(<SourceEditor />);
    fireEvent.compositionStart(screen.getByRole('textbox'));
    await expect(flushPendingEdits()).rejects.toThrow('组字');
  });
});

describe('恢复提示', () => {
  it('恢复后提示消失，只提交一次恢复', async () => {
    mockBackend.on('pending_recovery', () => ({ title: '剧本', blockCount: 2, revision: 3, savedAt: '', preview: '测试' }));
    mockBackend.on('adopt_recovery', () => undefined);
    useEditorStore.setState({ bootstrap: vi.fn().mockResolvedValue(undefined) });
    render(<RecoveryPrompt />);
    const button = await screen.findByRole('button', { name: '恢复这些内容' });
    await act(async () => { fireEvent.click(button); });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(mockBackend.callsTo('adopt_recovery')).toHaveLength(1);
  });
});

it('未保存项目首次保存会打开路径选择，取消不会假保存', async () => {
  useEditorStore.setState({ project: { state: 'unsaved', path: null, title: '新剧本', revision: 1, dirty: true, blockCount: 1 } });
  const result = await useEditorStore.getState().save();
  expect(result).toBeNull();
  expect(mockBackend.callsTo('save_project')).toHaveLength(0);
  expect(useEditorStore.getState().project.dirty).toBe(true);
});
it('自由文本结束编辑后显示后端统一的标题，提交期间不替换正在输入的文字',async()=>{
  useEditorStore.setState({changeSourceText:vi.fn(async()=>{useEditorStore.setState({sourceText:'内景。咖啡馆——白天'});return true;})});
  render(<SourceEditor/>);
  const input=screen.getByRole('textbox') as HTMLTextAreaElement;
  input.focus();fireEvent.input(input,{target:{value:'内，咖啡馆 白天'}});
  await act(async()=>{input.blur();});
  expect(input.value).toBe('内景。咖啡馆——白天');
});
