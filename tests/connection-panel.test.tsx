import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import ConnectionPanel from '@/components/ConnectionPanel';
import { mockBackend } from './setup';
const info = { cli: '/Applications/好好写剧本.app/Contents/Resources/resources/agent/screenplay', mcp: '/Applications/好好写剧本.app/Contents/Resources/resources/agent/screenplay-mcp', available: true, installDir: '/tmp/local/bin' };
const copy = vi.fn().mockResolvedValue(undefined);
const originalClipboard = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
beforeEach(() => {
  mockBackend.reset(); copy.mockClear();
  mockBackend.on('agent_connection_info', () => info);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: copy } });
});
afterEach(() => {
  cleanup();
  if (originalClipboard) Object.defineProperty(navigator, 'clipboard', originalClipboard);
  else delete (navigator as any).clipboard;
});
it('复制对话说明使用好好写剧本工具只读验证，不要求安装 Codex CLI 或偷偷修改客户端配置', async () => {
  const { getByRole, getByLabelText, getByText } = render(<ConnectionPanel onClose={() => undefined} />);
  fireEvent.click(getByText('通过 CLI 连接外部 Agent'));
  const button = getByRole('button', { name: '复制给 Agent 对话框' });
  await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(button);
  await waitFor(() => expect(copy).toHaveBeenCalledOnce());
  const prompt = copy.mock.calls[0][0];
  expect(prompt).toContain(`'${info.cli}' status`);
  expect(prompt).toContain('此次只读');
  expect(prompt).toContain('不要声称已经连接');
  expect(prompt).not.toContain('codex mcp add');
  expect(document.querySelector('[aria-label="MCP接入配置"]')).toBeNull();
  expect(mockBackend.calls.map(c => c.cmd)).toEqual(['agent_connection_info']);
});
it('不存在操作工具时禁用对话复制，避免提供失效路径', async () => {
  mockBackend.on('agent_connection_info', () => ({ ...info, available: false }));
  const { getByRole, getByText } = render(<ConnectionPanel onClose={() => undefined} />);
  await waitFor(() => expect(mockBackend.callsTo('agent_connection_info')).toHaveLength(1));
  fireEvent.click(getByText('通过 CLI 连接外部 Agent'));
  expect((getByRole('button', { name: '复制给 Agent 对话框' }) as HTMLButtonElement).disabled).toBe(true);
});

it('本机检查成功不能冒充外部 Agent 已连接，复制配置不触发安装', async () => {
  mockBackend.on('check_agent_connection', () => 'bridge(GUI 当前项目) 项目: 测试');
  const { getByRole, getByText } = render(<ConnectionPanel onClose={() => undefined} />);
  const check = getByRole('button', { name: '检查本机工具与当前项目' });
  await waitFor(() => expect((check as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(check);
  await waitFor(() => expect(getByText('bridge(GUI 当前项目) 项目: 测试')).toBeTruthy());
  expect(getByText('外部 Agent：连接状态尚未确认')).toBeTruthy();
  fireEvent.click(getByRole('button', { name: '复制给 Agent 对话框' }));
  await waitFor(() => expect(copy).toHaveBeenCalledOnce());
  expect(copy.mock.calls[0][0]).toContain(info.cli);
  expect(mockBackend.callsTo('install_agent_cli')).toHaveLength(0);
});
