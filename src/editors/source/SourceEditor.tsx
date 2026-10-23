import {sourceSegments,sourceBlockAt} from '@/utils/sourcePositions';
import {tr} from '@/i18n';
import { useCallback, useEffect, useRef, useState, useLayoutEffect, useMemo } from 'react';
import { useEditorStore } from '@/stores/editor';
import { registerPendingEditor } from '@/utils/pendingEdits';

/** Keep uncommitted input until IPC succeeds; flush it before saving. */
export default function SourceEditor() {
  const sourceText = useEditorStore((s) => s.sourceText);
  const changeSourceText = useEditorStore((s) => s.changeSourceText);
  const pushToast = useEditorStore((s) => s.pushToast);
  const blocks=useEditorStore(s=>s.document?.blocks);
  const defaultSize=useEditorStore(s=>s.document?.settings?.font_size ?? 14);
  const [sizeOverride,setSizeOverride]=useState<number|null>(()=>{const n=Number(localStorage.getItem("haohaoxie.source-font-size"));return n>=8&&n<=36?n:null;});
  const fontSize=sizeOverride??defaultSize;
  const resizeText=(n:number|null)=>{setSizeOverride(n);if(n===null)localStorage.removeItem("haohaoxie.source-font-size");else localStorage.setItem("haohaoxie.source-font-size",String(n));};
  const active=useEditorStore(s=>s.activeBlockId);
  const [draft,setDraft]=useState(sourceText);
  const layerRef=useRef<HTMLDivElement>(null);
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
    setDraft(sourceText);
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

  useLayoutEffect(()=>{
    const textarea=textareaRef.current,layer=layerRef.current;if(!textarea||!layer)return;
    const update=()=>{layer.style.width=`${textarea.clientWidth}px`;layer.style.transform=`translate(${-textarea.scrollLeft}px, ${-textarea.scrollTop}px)`;};
    update();const observer=typeof ResizeObserver!=='undefined'?new ResizeObserver(update):null;observer?.observe(textarea);return()=>observer?.disconnect();
  },[fontSize]);
  useEffect(() => registerPendingEditor(flush), [flush]);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const submit = () => void flush().catch((e) => pushToast('error', String(e)));
  const schedule = () => {
    if(textareaRef.current)setDraft(textareaRef.current.value);
    if (!edited.current) baseRevision.current = useEditorStore.getState().document?.revision;
    edited.current = true;
    window.clearTimeout(timer.current);
    if (!composing.current) timer.current = window.setTimeout(submit, 400);
  };

  const segments=useMemo(()=>draft===sourceText?sourceSegments(draft,blocks??[]):[],[draft,sourceText,blocks]);
  const syncSelection=()=>{const el=textareaRef.current;if(el&&el.value===sourceText){const id=sourceBlockAt(segments,el.selectionStart);if(id)useEditorStore.getState().setActiveBlock(id);}};
  useEffect(()=>{if(document.activeElement===textareaRef.current)syncSelection();},[sourceText,blocks]);
  const mirrored:React.ReactNode[]=[];let cursor=0;
  for(const segment of segments){mirrored.push(draft.slice(cursor,segment.from));mirrored.push(<span key={segment.from} data-source-blocks={segment.ids.join(" ")} className={active&&segment.ids.includes(active)?"source-corresponding active":"source-corresponding"}>{draft.slice(segment.from,segment.to)}</span>);cursor=segment.to;}
  mirrored.push(draft.slice(cursor),"\n");
  return <div className="source-editor">
    <div className="source-editor-hint">{tr("自由写作 · 支持普通中文剧本和 Fountain 格式")}</div>
    <div className="source-input-wrap"><div className="source-text-layer" style={{fontSize:`${fontSize}pt`}} ref={layerRef} aria-hidden="true">{mirrored}</div>
    <textarea style={{fontSize:`${fontSize}pt`}} ref={textareaRef} defaultValue={sourceText} spellCheck={false}
      aria-label={tr("自由剧本文本")} placeholder={tr("在此输入或粘贴剧本文本…")}
      onScroll={e=>{if(layerRef.current){layerRef.current.style.transform=`translate(${-e.currentTarget.scrollLeft}px, ${-e.currentTarget.scrollTop}px)`;layerRef.current.style.width=`${e.currentTarget.clientWidth}px`;}}}
      onSelect={syncSelection}
      onKeyUp={syncSelection}
      onPointerUp={syncSelection}
      onInput={schedule}
      onCompositionStart={() => { composing.current = true; window.clearTimeout(timer.current); useEditorStore.getState().setComposing(true); }}
      onCompositionEnd={() => {
        composing.current = false;
        useEditorStore.getState().setComposing(false);
        requestAnimationFrame(schedule);
      }}
      onBlur={() => { if (!composing.current) void flush().then(()=>{const el=textareaRef.current;if(!el||edited.current||document.activeElement===el)return;const latest=useEditorStore.getState().sourceText;el.value=latest;setDraft(latest);acknowledged.current=latest;}).catch(e=>pushToast('error',String(e))); }} /></div>
    <div className="source-font-tools"><label htmlFor="source-font-size">{tr("文字大小")}</label><input id="source-font-size" aria-label={tr("自由文本字号")} type="range" min="8" max="36" step="1" value={fontSize} onChange={e=>resizeText(Number(e.target.value))}/><output>{fontSize} pt</output><button onClick={()=>resizeText(null)}>{tr("恢复默认字号")}</button></div>
  </div>;
}
