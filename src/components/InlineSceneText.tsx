import {useEffect,useLayoutEffect,useRef,useState} from 'react';
import {tr} from '@/i18n';
import {registerPendingEditor} from '@/utils/pendingEdits';
/** Preview and editing use the same text element, metrics and scroll position. */
export default function InlineSceneText({value,label,title=false,onSave}:{value:string;label:string;title?:boolean;onSave:(text:string,original:string)=>Promise<void>}) {
 const [editing,setEditing]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const root=useRef<HTMLDivElement>(null),input=useRef<HTMLDivElement>(null),original=useRef(value),composing=useRef(false),saving=useRef(false),live=useRef(true),point=useRef<{x:number;y:number}|null>(null);
 useEffect(()=>{live.current=true;return()=>{live.current=false;};},[]);
 useLayoutEffect(()=>{if(!editing&&input.current&&input.current.textContent!==value)input.current.textContent=value;},[value,editing]);
 useLayoutEffect(()=>{if(!editing||!input.current)return;const el=input.current,top=el.scrollTop;el.focus({preventScroll:true});const doc=document as Document&{caretRangeFromPoint?:(x:number,y:number)=>Range|null};const range=point.current&&doc.caretRangeFromPoint?.(point.current.x,point.current.y);if(range&&el.contains(range.startContainer)){range.collapse(true);const selection=window.getSelection();selection?.removeAllRanges();selection?.addRange(range);}el.scrollTop=top;point.current=null;},[editing]);
 useEffect(()=>{if(!editing)return;const outside=(event:PointerEvent)=>{if(!root.current?.contains(event.target as Node))input.current?.blur();};document.addEventListener('pointerdown',outside,true);return()=>document.removeEventListener('pointerdown',outside,true);},[editing]);
 const begin=()=>{original.current=value;setError('');setEditing(true);};
 const pending=useRef<Promise<void>|null>(null);
 const save=async()=>{if(pending.current){await pending.current;return;}if(composing.current)throw new Error(tr('请先完成输入法组字'));const currentText=input.current?.innerText??input.current?.textContent??'';const text=title?currentText.replace(/[\r\n]+/g,' ').trim():currentText;
  if(title&&!text){const message=tr('场景标题不能为空');setError(message);input.current?.focus();throw new Error(message);}
  if(text.length>1000){const message=tr('便签文字不能超过1000字');setError(message);input.current?.focus();throw new Error(message);}
  if(text===original.current){setEditing(false);return;}saving.current=true;setBusy(true);
  const task=(async()=>{try{await onSave(text,original.current);if(live.current)setEditing(false);}catch(e){if(live.current){setError(String(e));requestAnimationFrame(()=>input.current?.focus({preventScroll:true}));}throw e;}finally{saving.current=false;if(live.current)setBusy(false);}})();
  pending.current=task;try{await task;}finally{pending.current=null;}
 };
 useEffect(()=>{const unregister=registerPendingEditor(async()=>{if(pending.current)await pending.current;else if(editing)await save();});return()=>{if(pending.current)void pending.current.finally(unregister).catch(()=>{});else unregister();};},[editing,onSave]);
 return <div ref={root} className={title?'scene-text-area scene-title-inline':'scene-text-area scene-summary-area'}>
  <div ref={input} className={title?'scene-title-text':'scene-summary-inline scene-summary'} contentEditable={editing&&!busy?'plaintext-only':false} suppressContentEditableWarning role={editing?'textbox':'button'} aria-multiline={editing&&!title?true:undefined} aria-label={label} data-placeholder={title?'':tr('双击填写场景概括')} data-editing={editing?'true':undefined} tabIndex={0} title={editing?undefined:tr('双击编辑')}
   onDoubleClick={e=>{e.stopPropagation();if(!editing){point.current={x:e.clientX,y:e.clientY};begin();}}}
   onPointerDown={e=>{if(e.button===0&&(editing||!title))e.stopPropagation();}}
   onClick={e=>{if(editing)e.stopPropagation();}}
   onDragStart={e=>{if(editing||!title){e.preventDefault();e.stopPropagation();}}}
   onWheel={e=>{if(!title&&e.currentTarget.scrollHeight>e.currentTarget.clientHeight)e.stopPropagation();}}
   onBlur={()=>{if(editing&&!composing.current)void save().catch(()=>{});}}
   onCompositionStart={()=>{composing.current=true;}}
   onCompositionEnd={()=>{composing.current=false;if(document.activeElement!==input.current)requestAnimationFrame(()=>{if(live.current)void save().catch(()=>{});});}}
   onPaste={e=>{if(!editing)return;e.preventDefault();const text=e.clipboardData.getData('text/plain');document.execCommand('insertText',false,title?text.replace(/[\r\n]+/g,' '):text);}}
   onKeyDown={e=>{if(!editing){if(e.key==='Enter'){e.preventDefault();e.stopPropagation();begin();}return;}e.stopPropagation();if(e.nativeEvent.isComposing||e.keyCode===229)return;if(e.key==='Escape'){e.preventDefault();if(input.current)input.current.textContent=value;setEditing(false);setError('');}else if(e.key==='Enter'&&(title||e.metaKey||e.ctrlKey)){e.preventDefault();input.current?.blur();}}}/>
  {busy&&<small className="scene-text-status" role="status">{tr('保存中…')}</small>}{error&&<small className="scene-text-status" role="alert">{error}</small>}
 </div>;
}
