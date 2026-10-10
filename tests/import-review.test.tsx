import { afterEach, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import ImportReview from '@/components/ImportReview';
import { useEditorStore } from '@/stores/editor';
import type { ScreenplayDocument } from '@/ipc/types/ScreenplayDocument';

const sourceDoc = () => ({
  id: 'doc', revision: 1, blocks: [{ id: 'b1', block_type: 'action', text: '老板娘：走吧', confidence: 0.45 }],
  import_info: { source_type: 'txt', imported_at: '', original_text: '原始完整剧本文本', removed_noise: ['页脚 第 1 页'], preserve_original: true },
} as unknown as ScreenplayDocument);
afterEach(cleanup);

it('展示低置信度、完整原文和去噪记录，并将人工确认写回块类型', async () => {
  const changeBlockType = vi.fn(async (id: string, type: string) => {
    expect(id).toBe('b1');
    expect(type).toBe('dialogue');
    const doc = useEditorStore.getState().document!;
    useEditorStore.setState({ document: { ...doc, revision: 2, blocks: doc.blocks.map(b => ({ ...b, block_type: 'dialogue', confidence: 1 })) } as never });
  });
  useEditorStore.setState({ document: sourceDoc(), changeBlockType, pushToast: vi.fn() });
  render(<ImportReview onClose={vi.fn()} />);
  expect(screen.getAllByText('老板娘：走吧')).toHaveLength(2);
  fireEvent.click(screen.getByText(/查看完整提取原文/));
  expect(screen.getByText('原始完整剧本文本')).toBeTruthy();
  fireEvent.click(screen.getByText(/去噪记录/));
  expect(screen.getByText('页脚 第 1 页')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('元素类型'), { target: { value: 'dialogue' } });
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: '确认此类型' })); });
  expect(changeBlockType).toHaveBeenCalledWith('b1', 'dialogue');
  expect(screen.getByText('所有低置信度元素均已确认。')).toBeTruthy();
});
