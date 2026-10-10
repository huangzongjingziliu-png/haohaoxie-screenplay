import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import NewProjectDialog from '@/components/NewProjectDialog';
import { useEditorStore } from '@/stores/editor';
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn() }));
import { save } from '@tauri-apps/plugin-dialog';
const newProject = vi.fn(async () => { useEditorStore.setState({ document: { id: 'new' } as never }); });
const persist = vi.fn().mockResolvedValue({ path: '/tmp/选定.screenplay' });
beforeEach(() => {
  vi.mocked(save).mockReset(); newProject.mockClear(); persist.mockClear();
  useEditorStore.setState({ document: null, newProject, save: persist });
});
afterEach(cleanup);
it('取消选位置不创建项目；选择路径后才创建并保存到选定文件', async () => {
  vi.mocked(save).mockResolvedValueOnce(null).mockResolvedValueOnce('/tmp/选定.screenplay');
  const { getByRole } = render(<NewProjectDialog onClose={() => undefined} onCreated={() => undefined} />);
  const button = getByRole('button', { name: '选择保存位置并创建' });
  fireEvent.click(button);
  await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  expect(newProject).not.toHaveBeenCalled();
  await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(button);
  await waitFor(() => expect(persist).toHaveBeenCalledWith('/tmp/选定.screenplay'));
});
it('暂不保存可创建试写项目，不弹文件对话框也不保存', async () => {
  const { getByRole } = render(<NewProjectDialog onClose={() => undefined} onCreated={() => undefined} />);
  fireEvent.click(getByRole('button', { name: '暂不保存，开始写作' }));
  await waitFor(() => expect(newProject).toHaveBeenCalledTimes(1));
  expect(save).not.toHaveBeenCalled(); expect(persist).not.toHaveBeenCalled();
});
