import {setUiLanguage} from '@/i18n';
setUiLanguage('zh');
/**
 * 测试环境准备。
 *
 * 关键：mock Tauri IPC，让前端测试能验证「前端是否正确调用后端命令、
 * 是否正确处理结构化错误」，而不需要真的启动 GUI。
 *
 * 注意：这**不能替代**真实 GUI E2E（见 qa/evidence/A/e2e/），
 * 两者互补：单元测试验证逻辑，E2E 验证真实运行时行为。
 */

import { vi } from 'vitest';

// ---- mock @tauri-apps/api/core ----
const invokeCalls: Array<{ cmd: string; args?: Record<string, unknown> }> = [];

/** 测试用例可以通过这个钩子注册命令实现 */
export const mockBackend = {
  handlers: new Map<string, (args: Record<string, unknown>) => unknown>(),
  calls: invokeCalls,
  reset() {
    this.handlers.clear();
    invokeCalls.length = 0;
  },
  on(cmd: string, fn: (args: Record<string, unknown>) => unknown) {
    this.handlers.set(cmd, fn);
  },
  callsTo(cmd: string) {
    return invokeCalls.filter((c) => c.cmd === cmd);
  },
};

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    invokeCalls.push({ cmd, args });
    const h = mockBackend.handlers.get(cmd);
    if (!h) {
      return Promise.reject({
        error: 'APP_STATE',
        message: `测试 mock 未注册命令: ${cmd}`,
      });
    }
    try {
      return Promise.resolve(h(args ?? {}));
    } catch (e) {
      return Promise.reject(e);
    }
  },
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: () => Promise.resolve(() => {}),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: () => Promise.resolve(null),
  save: () => Promise.resolve(null),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: () => Promise.reject(new Error('未 mock')),
  writeFile: () => Promise.reject(new Error('未 mock')),
}));

// jsdom 不实现 scrollIntoView
Element.prototype.scrollIntoView = vi.fn();