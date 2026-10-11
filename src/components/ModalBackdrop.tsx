import {useRef, useState, type ReactNode, type CSSProperties} from 'react';
import {tr} from '@/i18n';

export default function ModalBackdrop({children,onDismiss,disabled=false,dirty,message,className='modal-backdrop',style}:{children:ReactNode;onDismiss:(edited?:boolean)=>void;disabled?:boolean;dirty?:boolean;message?:string;className?:string;style?:CSSProperties}) {
 const [confirming,setConfirming]=useState(false);
 const initial=useRef(new Map<HTMLElement,string>());
 const changed=useRef(new Set<HTMLElement>());
 const value=(el:HTMLInputElement|HTMLTextAreaElement|HTMLSelectElement)=>el instanceof HTMLInputElement&&['checkbox','radio'].includes(el.type)?String(el.checked):el.value;
 return <div className={className} style={style}
  onPointerDownCapture={e=>{
   if(e.target===e.currentTarget)return;
   e.currentTarget.querySelectorAll<HTMLInputElement|HTMLTextAreaElement|HTMLSelectElement>('input,textarea,select').forEach(el=>{if(!initial.current.has(el))initial.current.set(el,value(el));});
  }}
  onFocusCapture={e=>{
   const el=e.target;
   if(el instanceof HTMLInputElement||el instanceof HTMLTextAreaElement||el instanceof HTMLSelectElement){
    if(!initial.current.has(el))initial.current.set(el,value(el));
   }
  }}
  onChangeCapture={e=>{
   const el=e.target;
   if(el instanceof HTMLInputElement||el instanceof HTMLTextAreaElement||el instanceof HTMLSelectElement){
    if(value(el)!==initial.current.get(el))changed.current.add(el);else changed.current.delete(el);
   }
  }}
  onClick={e=>{
   if(e.target!==e.currentTarget||disabled)return;
   const unsaved=dirty??([...changed.current].some(el=>el.isConnected)||[...initial.current].some(([el,before])=>el.isConnected&&value(el as HTMLInputElement|HTMLTextAreaElement|HTMLSelectElement)!==before));
   if(unsaved)setConfirming(true);else onDismiss(false);
  }}>{children}{confirming&&<div className="dismiss-confirm-backdrop" onClick={e=>e.stopPropagation()}><section className="modal dismiss-confirm" role="alertdialog" aria-modal="true" aria-label={tr("确认关闭")}><h3>{tr("确认关闭")}</h3><p>{message??tr("放弃尚未提交的修改并关闭？")}</p><div className="modal-actions"><button onClick={()=>setConfirming(false)}>{tr("取消")}</button><button className="primary" onClick={()=>{setConfirming(false);onDismiss(true);}}>{tr("确认")}</button></div></section></div>}</div>;
}
