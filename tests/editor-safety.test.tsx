import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import SourceEditor from '@/editors/source/SourceEditor';
import RecoveryPrompt from '@/components/RecoveryPrompt';
import { useEditorStore } from '@/stores/editor';
import { flushPendingEdits, hasPendingDraft, setDraftPending, subscribeDrafts } from '@/utils/pendingEdits';
import { saveStateLabel } from '@/components/StatusBar';
import { mockBackend } from './setup';

/** 模块加载时的真实 bootstrap；部分用例会覆盖它，冲突重试用例需要还原。 */
const realBootstrap = useEditorStore.getState().bootstrap;

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

it('revision 冲突后重新取基准并重试一次，文字不再卡在旧 revision', async () => {
  let calls = 0;
  mockBackend.on('replace_block', () => {
    calls += 1;
    if (calls === 1) throw JSON.stringify({ error: 'REVISION_CONFLICT', expected: 1, actual: 2, message: '外部已修改' });
    return { affected: ['a'], revision: 3, document: { id: 'doc', revision: 3, blocks: [{ id: 'a', block_type: 'action', text: '新文字' }] }, source_text: '新文字' };
  });
  mockBackend.on('bootstrap', () => ({
    project: { state: 'open', path: '/tmp/x', title: 't', dirty: false, revision: 2, blockCount: 1 },
    document: { id: 'doc', revision: 2, blocks: [{ id: 'a', block_type: 'action', text: '旧' }] },
    sourceText: '旧',
  }));
  useEditorStore.setState({ document: { id: 'doc', revision: 1, blocks: [{ id: 'a', block_type: 'action', text: '旧' }] } as never, composing: false, toasts: [], bootstrap: realBootstrap });

  const ok = await useEditorStore.getState().changeBlockText('a', '新文字');

  expect(mockBackend.callsTo('bootstrap')).toHaveLength(1);
  expect(useEditorStore.getState().document?.revision).toBe(3);
  expect(ok).toBe(true);
  const retries = mockBackend.callsTo('replace_block');
  expect(retries).toHaveLength(2);
  expect(retries[0].args?.expectedRevision).toBe(1);
  expect(retries[1].args?.expectedRevision).toBe(2);
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

describe('保存状态三态', () => {
  it('输入未提交优先于等待写盘；脏但已提交是等待写盘；干净是已写盘', () => {
    expect(saveStateLabel({ error: false, pendingDraft: true, hasPath: true, dirty: true, saving: false }).text).toBe('✎ 输入未提交');
    expect(saveStateLabel({ error: false, pendingDraft: false, hasPath: true, dirty: true, saving: false }).text).toBe('● 等待写盘');
    expect(saveStateLabel({ error: false, pendingDraft: false, hasPath: true, dirty: false, saving: false }).text).toBe('○ 已写盘');
    // 保存失败优先级最高，且不能因为还有草稿就掩盖错误
    expect(saveStateLabel({ error: true, pendingDraft: true, hasPath: true, dirty: true, saving: false }).text).toBe('保存失败 · 请重试');
  });

  it('草稿登记可订阅、可清零', () => {
    let notified = 0;
    const off = subscribeDrafts(() => { notified += 1; });
    setDraftPending('t:a', true);
    expect(hasPendingDraft()).toBe(true);
    expect(notified).toBeGreaterThan(0);
    setDraftPending('t:a', false);
    expect(hasPendingDraft()).toBe(false);
    off();
  });
});
