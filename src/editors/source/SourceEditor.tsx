import {tr} from '@/i18n';
import { useCallback, useEffect, useRef } from 'react';
import { useEditorStore } from '@/stores/editor';
import { registerPendingEditor } from '@/utils/pendingEdits';

/** Keep uncommitted input until IPC succeeds; flush it before saving. */
export default function SourceEditor() {
  const sourceText = useEditorStore((s) => s.sourceText);
  const changeSourceText = useEditorStore((s) => s.changeSourceText);
  const pushToast = useEditorStore((s) => s.pushToast);
  const composing = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const acknowledged = useRef(sourceText);
  const edited = useRef(false);
  const baseRevision = useRef<number>();
  const timer = useRef<number>();
  const inFlight = useRef<Promise<void> | null>(null);

  useEffect(() => {
    const el = textareaRef.current;
    if (!el || composing.current || edited.current || inFlight.current) return;
    if (el.value !== sourceText) {
      const start = el.selectionStart;
      const end = el.selectionEnd;
      el.value = sourceText;
      if (document.activeElement === el) el.setSelectionRange(Math.min(start, sourceText.length), Math.min(end, sourceText.length));
    }
    acknowledged.current = sourceText;
  }, [sourceText]);

  const flush = useCallback(async function flushDraft(): Promise<void> {
    window.clearTimeout(timer.current);
    if (composing.current) throw new Error(tr("请先完成输入法组字"));
    if (inFlight.current) await inFlight.current;
    const el = textareaRef.current;
    if (!el || !edited.current) return;
    const text = el.value;
    if (text === acknowledged.current) { edited.current = false; return; }
    const task = (async () => {
      if (!await changeSourceText(text, baseRevision.current)) throw new Error(tr("文字尚未提交成功，请重试后再保存"));
      acknowledged.current = text;
      edited.current = el.value !== text;
      baseRevision.current = useEditorStore.getState().document?.revision;
    })();
    inFlight.current = task;
    try { await task; } finally { inFlight.current = null; }
    if (edited.current) await flushDraft();
  }, [changeSourceText]);

  useEffect(() => registerPendingEditor(flush), [flush]);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const submit = () => void flush().catch((e) => pushToast('error', String(e)));
  const schedule = () => {
    if (!edited.current) baseRevision.current = useEditorStore.getState().document?.revision;
    edited.current = true;
    window.clearTimeout(timer.current);
    if (!composing.current) timer.current = window.setTimeout(submit, 400);
  };

  return <div className="source-editor">
    <div className="source-editor-hint">{tr("自由写作 · 支持普通中文剧本和 Fountain 格式")}</div>
    <textarea ref={textareaRef} defaultValue={sourceText} spellCheck={false}
      aria-label={tr("自由剧本文本")} placeholder={tr("在此输入或粘贴剧本文本…")}
      onInput={schedule}
      onCompositionStart={() => { composing.current = true; window.clearTimeout(timer.current); useEditorStore.getState().setComposing(true); }}
      onCompositionEnd={() => {
        composing.current = false;
        useEditorStore.getState().setComposing(false);
        requestAnimationFrame(schedule);
      }}
      onBlur={() => { if (!composing.current) submit(); }} />
  </div>;
}
