import {act, cleanup, fireEvent, render, screen, waitFor} from '@testing-library/react';
import {afterEach, beforeEach, expect, it} from 'vitest';
import AssistantSettings from '@/components/AssistantSettings';
import {mockBackend} from './setup';

beforeEach(() => {
  mockBackend.reset();
  mockBackend.on('agent_list_experts', () => ({
    experts: [
      {id: 'beat-sheet', name: '节拍表专家', description: '检查节拍', dimensions: ['开场', '中点'], kind: 'review', builtin: true, enabled: true},
      {id: 'my-expert', name: '我的专家', description: '只看代价', dimensions: ['代价'], kind: 'writing', builtin: false, enabled: false},
    ],
    issues: [],
  }));
  mockBackend.on('agent_set_expert_enabled', () => null);
  mockBackend.on('agent_rename_expert', () => null);
});
afterEach(() => cleanup());

it('列出内置与自定义专家，显示类型与停用状态，勾选会写回启用状态', async () => {
  render(<AssistantSettings />);
  await waitFor(() => expect(screen.getByText('节拍表专家')).toBeTruthy());
  expect(screen.getByText('我的专家')).toBeTruthy();
  expect(screen.getByText('审稿型')).toBeTruthy();
  expect(screen.getByText('写作型')).toBeTruthy();
  expect(screen.getByText('已停用')).toBeTruthy();
  expect(screen.queryByText(/技能/)).toBeNull();

  const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[];
  act(() => { boxes[1].click(); });
  await waitFor(() => expect(mockBackend.callsTo('agent_set_expert_enabled').length).toBe(1));
  expect(mockBackend.callsTo('agent_set_expert_enabled')[0].args).toEqual({expertId: 'my-expert', enabled: true});
});

it('自定义专家可改名，内置专家不出现改名入口', async () => {
  render(<AssistantSettings />);
  await waitFor(() => expect(screen.getByText('我的专家')).toBeTruthy());
  const renameButtons = screen.getAllByText('改名');
  expect(renameButtons.length).toBe(1);

  fireEvent.click(renameButtons[0]);
  const input = screen.getByLabelText('新名称') as HTMLInputElement;
  fireEvent.change(input, {target: {value: '代价专家'}});
  fireEvent.click(screen.getByText('保存'));
  await waitFor(() => expect(mockBackend.callsTo('agent_rename_expert').length).toBe(1));
  expect(mockBackend.callsTo('agent_rename_expert')[0].args).toEqual({expertId: 'my-expert', name: '代价专家'});
});
