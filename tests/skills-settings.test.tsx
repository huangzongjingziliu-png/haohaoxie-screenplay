import {act, cleanup, fireEvent, render, screen, waitFor} from '@testing-library/react';
import {afterEach, beforeEach, expect, it} from 'vitest';
import SkillSettings from '@/components/SkillSettings';
import {mockBackend} from './setup';

beforeEach(() => {
  mockBackend.reset();
  mockBackend.on('agent_list_experts', () => ({
    experts: [
      {id: 'beat-sheet', name: '布莱克·斯奈德的节拍表', description: '检查节拍', dimensions: ['开场', '中点'], kind: 'review', builtin: true, enabled: true},
      {id: 'my-skill', name: '我的写作技能', description: '只看代价', dimensions: [], kind: 'writing', builtin: false, enabled: false},
    ],
    issues: [],
  }));
  mockBackend.on('agent_set_expert_enabled', () => null);
  mockBackend.on('agent_rename_expert', () => null);
});
afterEach(() => cleanup());

it('列出内置与自定义技能，显示类型与停用状态，勾选会写回启用状态', async () => {
  render(<SkillSettings creating={false} onCreating={() => {}} />);
  await waitFor(() => expect(screen.getByText('布莱克·斯奈德的节拍表')).toBeTruthy());
  expect(screen.getByText('我的写作技能')).toBeTruthy();
  expect(screen.getByText('审稿型')).toBeTruthy();
  expect(screen.getByText('写作型')).toBeTruthy();
  expect(screen.getByText('已停用')).toBeTruthy();
  expect(screen.queryByText(/专家/)).toBeNull();

  const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[];
  act(() => { boxes[1].click(); });
  await waitFor(() => expect(mockBackend.callsTo('agent_set_expert_enabled').length).toBe(1));
  expect(mockBackend.callsTo('agent_set_expert_enabled')[0].args).toEqual({expertId: 'my-skill', enabled: true});
});

it('自定义技能可改名，内置技能不出现改名入口', async () => {
  render(<SkillSettings creating={false} onCreating={() => {}} />);
  await waitFor(() => expect(screen.getByText('我的写作技能')).toBeTruthy());
  const renameButtons = screen.getAllByText('改名');
  expect(renameButtons.length).toBe(1);

  fireEvent.click(renameButtons[0]);
  const input = screen.getByLabelText('新名称') as HTMLInputElement;
  fireEvent.change(input, {target: {value: '代价写作'}});
  fireEvent.click(screen.getByText('保存'));
  await waitFor(() => expect(mockBackend.callsTo('agent_rename_expert').length).toBe(1));
  expect(mockBackend.callsTo('agent_rename_expert')[0].args).toEqual({expertId: 'my-skill', name: '代价写作'});
});
