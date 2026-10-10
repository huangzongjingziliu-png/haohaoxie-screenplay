import { useEffect, useRef, useState } from 'react';
import { moveScene } from '@/utils/scenes';
import { useEditorStore } from '@/stores/editor';
/** Pointer dragging works consistently in the macOS WebView and keeps taps as selection. */
export function useSceneDrag(axis: 'horizontal' | 'vertical' = 'horizontal', onDrop?: (from:string,to:string,side:'before'|'after')=>Promise<unknown>) {
  const [pointerTarget, setPointerTarget] = useState<string | null>(null);
  const [insertion, setInsertion] = useState<{id:string;side:'before'|'after'} | null>(null);
  const cleanupRef = useRef<(()=>void) | null>(null);
  useEffect(()=>()=>cleanupRef.current?.(),[]);
  const startDrag = (e: React.PointerEvent, id:string) => {
    if (e.button !== 0 || e.metaKey || e.ctrlKey || (e.target as HTMLElement).closest('button,input,textarea,select')) return;
    e.preventDefault();
    e.currentTarget instanceof HTMLElement && e.currentTarget.focus();
    cleanupRef.current?.();
    const x=e.clientX,y=e.clientY;
    let moved=false;
    const root=(e.currentTarget as HTMLElement).closest('.board-cards,.navigator-body');
    const targetAt=(event:PointerEvent)=> {
      /* captured before React clears event.currentTarget */

      let target=document.elementFromPoint?.(event.clientX,event.clientY)?.closest<HTMLElement>('[data-scene-id]');
      if (!target && root) {
        const bounds=root.getBoundingClientRect();
        if (event.clientX>=bounds.left && event.clientX<=bounds.right && event.clientY>=bounds.top && event.clientY<=bounds.bottom) {
          const items=Array.from(root.querySelectorAll<HTMLElement>('[data-scene-id]'));
          target=items.sort((a,b)=> { const distance=(el:HTMLElement)=> {const r=el.getBoundingClientRect();return Math.hypot(Math.max(r.left-event.clientX,0,event.clientX-r.right),Math.max(r.top-event.clientY,0,event.clientY-r.bottom));};return distance(a)-distance(b);})[0];
        }
      }
      if (!target || target.dataset.sceneId===id) return null;
      const r=target.getBoundingClientRect();
      const side=axis==='horizontal' ? (event.clientX<(r.left+r.right)/2?'before':'after') : (event.clientY<(r.top+r.bottom)/2?'before':'after');
      return {id:target.dataset.sceneId!,side} as const;
    };
    const move=(event:PointerEvent)=> { if (Math.hypot(event.clientX-x,event.clientY-y)<8 && !moved) return; moved=true;const target=targetAt(event);setPointerTarget(target?.id ?? null);setInsertion(target); };
    const cleanup=()=>{window.removeEventListener('pointermove',move);window.removeEventListener('pointerup',up);window.removeEventListener('pointercancel',cancel);cleanupRef.current=null;setPointerTarget(null);setInsertion(null);};
    const up=(event:PointerEvent)=>{const target=targetAt(event);cleanup();if(moved && target) void (onDrop ? onDrop(id,target.id,target.side) : moveScene(id,target.id,target.side)).catch(error=>useEditorStore.getState().pushToast('error',String(error)));};
    const cancel=()=>cleanup();
    cleanupRef.current=cleanup;
    window.addEventListener('pointermove',move);window.addEventListener('pointerup',up);window.addEventListener('pointercancel',cancel);
  };
  return {startDrag,pointerTarget,insertion};
}
