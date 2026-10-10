import { useEffect } from 'react';
import { useEditorStore } from '@/stores/editor';

/**
 * 全局消息条。
 *
 * 存在的意义：原代码里所有错误都被 `console.error` 吞掉，
 * 用户点了按钮没反应也不知道为什么。这里让每个错误都可见。
 */
export default function Toasts() {
  const toasts = useEditorStore((s) => s.toasts);
  const dismiss = useEditorStore((s) => s.dismissToast);

  useEffect(() => {
    if (toasts.length === 0) return;
    // 成功提示 3 秒自动消失；错误保留 8 秒（需要用户看清）
    const timers = toasts.map((t) =>
      window.setTimeout(() => dismiss(t.id), t.kind === 'error' ? 8000 : 3000)
    );
    return () => timers.forEach((t) => window.clearTimeout(t));
  }, [toasts, dismiss]);

  if (toasts.length === 0) return null;

  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast-${t.kind}`} onClick={() => dismiss(t.id)}>
          {t.code && <span className="toast-code">{t.code}</span>}
          <span className="toast-message">{t.message}</span>
        </div>
      ))}
    </div>
  );
}