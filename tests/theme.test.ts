import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { initTheme, paperInkColor, readTheme, resolveTheme, saveTheme } from '@/utils/theme';

let listeners: Array<() => void> = [];
let systemDark = false;

beforeEach(() => {
  localStorage.clear();
  document.body.className = '';
  listeners = [];
  systemDark = false;
  vi.stubGlobal('matchMedia', (query: string) => ({
    media: query,
    get matches() {
      return systemDark;
    },
    addEventListener: (_event: string, fn: () => void) => listeners.push(fn),
    removeEventListener: (_event: string, fn: () => void) => {
      listeners = listeners.filter((item) => item !== fn);
    },
  }));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it('默认保持浅色，选择会被记住', () => {
  expect(readTheme()).toBe('light');
  saveTheme('dark');
  expect(document.body.classList.contains('dark')).toBe(true);
  expect(localStorage.getItem('muben.appearance-theme.v1')).toBe('dark');
  expect(readTheme()).toBe('dark');
});

it('跟随系统会随外观变化即时切换，离开后停止监听', () => {
  saveTheme('system');
  expect(document.body.classList.contains('dark')).toBe(false);
  const stop = initTheme();
  systemDark = true;
  listeners.forEach((fn) => fn());
  expect(document.body.classList.contains('dark')).toBe(true);
  expect(document.documentElement.dataset.theme).toBe('dark');
  stop();
  expect(listeners).toHaveLength(0);
});

it('固定深色不随系统外观变化', () => {
  saveTheme('dark');
  const stop = initTheme();
  systemDark = false;
  listeners.forEach((fn) => fn());
  expect(resolveTheme()).toBe('dark');
  expect(document.body.classList.contains('dark')).toBe(true);
  stop();
});

it('纯黑纸面文字在深色下交给 CSS 变量处理，其他颜色保持原样', () => {
  expect(paperInkColor('#000000')).toBe('var(--paper-ink, #000000)');
  expect(paperInkColor('#2355aa')).toBe('#2355aa');
});
