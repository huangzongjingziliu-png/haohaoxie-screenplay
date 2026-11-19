/**
 * 显示模式：浅色 / 深色 / 跟随系统。
 *
 * - 偏好存本机 localStorage，重启后仍生效；
 * - 「跟随系统」通过 matchMedia 监听 macOS 外观，切换即时生效；
 * - 只影响应用界面，导出（PDF / DOCX / HTML）由 Rust 排版决定，不读这里的状态。
 */
export type ThemeMode = 'light' | 'dark' | 'system';

const STORAGE_KEY = 'muben.appearance-theme.v1';
const DARK_QUERY = '(prefers-color-scheme: dark)';

/** 未设置过时保持升级前的浅色外观。 */
export function readTheme(): ThemeMode {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return value === 'dark' || value === 'system' ? value : 'light';
  } catch {
    return 'light';
  }
}

function darkQuery(): MediaQueryList | null {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
    ? window.matchMedia(DARK_QUERY)
    : null;
}

export function resolveTheme(mode: ThemeMode = readTheme()): 'light' | 'dark' {
  if (mode === 'system') return darkQuery()?.matches ? 'dark' : 'light';
  return mode;
}

/** 把解析后的外观写到 body/html，CSS 以 `body.dark` 为深色开关。 */
export function applyTheme(mode: ThemeMode = readTheme()) {
  if (typeof document === 'undefined') return;
  const resolved = resolveTheme(mode);
  document.body.classList.toggle('dark', resolved === 'dark');
  document.documentElement.dataset.theme = resolved;
}

export function saveTheme(mode: ThemeMode) {
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    /* 存储不可用时仍当前会话生效 */
  }
  applyTheme(mode);
}

/** 启动时应用偏好，并在「跟随系统」下监听外观变化。返回取消监听的函数。 */
export function initTheme(): () => void {
  applyTheme();
  const query = darkQuery();
  if (!query) return () => undefined;
  const onChange = () => {
    if (readTheme() === 'system') applyTheme('system');
  };
  query.addEventListener('change', onChange);
  return () => query.removeEventListener('change', onChange);
}

/**
 * 纸面文字色：默认黑色在深色纸面上会看不见，
 * 交给 CSS 变量 `--paper-ink` 换成浅色；其他自定义颜色保持原样。
 */
export function paperInkColor(color: string): string {
  const match = /^#([0-9a-f]{6})$/i.exec(color.trim());
  if (!match) return color;
  const value = parseInt(match[1], 16);
  const luminance =
    0.2126 * ((value >> 16) & 255) + 0.7152 * ((value >> 8) & 255) + 0.0722 * (value & 255);
  return luminance < 40 ? `var(--paper-ink, ${color})` : color;
}
