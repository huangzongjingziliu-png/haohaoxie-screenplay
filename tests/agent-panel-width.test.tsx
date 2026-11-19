import { act, cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it } from 'vitest';
import AgentPanel from '@/components/AgentPanel';
import { useEditorStore } from '@/stores/editor';
import { mockBackend } from './setup';

const firePointer = (target: EventTarget, type: string, clientX: number) => {
  act(() => { target.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0, clientX })); });
};

beforeEach(() => {
  window.localStorage.clear();
  mockBackend.reset();
  mockBackend.on('agent_list_skills', () => []);
  mockBackend.on('agent_list_experts', () => ({ experts: [], issues: [] }));
  useEditorStore.setState({ document: { id: 'qa', revision: 1, blocks: [] } as never });
});
afterEach(() => { cleanup(); window.localStorage.clear(); });

it('面板默认 340px，拖动后限制在 300–480 并记住宽度', async () => {
  const view = render(<AgentPanel onClose={() => undefined} />);
  const panel = view.container.querySelector('.agent-panel') as HTMLElement;
  expect(panel.style.width).toBe('340px');

  const handle = view.getByRole('separator');
  firePointer(handle, 'pointerdown', 1000);
  expect(document.body.classList.contains('resizing-agent-panel')).toBe(true);
  firePointer(document, 'pointermove', 900);
  expect(panel.style.width).toBe('440px');
  firePointer(document, 'pointerup', 900);
  await waitFor(() => expect(window.localStorage.getItem('hhx.agentPanelWidth')).toBe('440'));

  firePointer(handle, 'pointerdown', 1000);
  firePointer(document, 'pointermove', 0);
  expect(panel.style.width).toBe('480px');
  firePointer(document, 'pointerup', 0);

  cleanup();
  const reopened = render(<AgentPanel onClose={() => undefined} />);
  expect((reopened.container.querySelector('.agent-panel') as HTMLElement).style.width).toBe('480px');
});
