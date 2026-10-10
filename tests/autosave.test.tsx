import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { useAutoSave } from '@/hooks/useAutoSave';
import { registerPendingEditor } from '@/utils/pendingEdits';
import { useEditorStore } from '@/stores/editor';
import { ipc } from '@/ipc/client';
function Harness() { useAutoSave(); return <input aria-label="draft" />; }
beforeEach(() => {
  vi.useFakeTimers();
  useEditorStore.setState({ project: { state: 'open', path: '/tmp/qa.screenplay', dirty: true } as never, refreshProjectInfo: vi.fn().mockResolvedValue(undefined), refreshHistory: vi.fn().mockResolvedValue(undefined) });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.useRealTimers(); });
it('自动保存先提交最后输入，组字期间跳过，完成后继续保存', async () => {
  const order: string[] = [];
  const unregister = registerPendingEditor(async () => { order.push('flush'); });
  const tick = vi.spyOn(ipc, 'autosaveTick').mockImplementation(async () => { order.push('save'); return 'saved'; });
  const { getByLabelText } = render(<Harness />);
  fireEvent.compositionStart(getByLabelText('draft'));
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(tick).not.toHaveBeenCalled();
  fireEvent.compositionEnd(getByLabelText('draft'));
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(order).toEqual(['flush', 'save']);
  unregister();
});
it('保存失败会显示错误且不清除dirty，下次可重试', async () => {
  const tick = vi.spyOn(ipc, 'autosaveTick').mockRejectedValueOnce(new Error('磁盘不可写')).mockResolvedValue('saved');
  const events: string[] = [];
  const listener = (e: Event) => events.push((e as CustomEvent).detail.state);
  window.addEventListener('save-status', listener);
  render(<Harness />);
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(events).toContain('error');
  expect(useEditorStore.getState().project.dirty).toBe(true);
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(tick).toHaveBeenCalledTimes(2);
  expect(events[events.length - 1]).toBe('idle');
  window.removeEventListener('save-status', listener);
});
