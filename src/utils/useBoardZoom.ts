import { useEffect, useRef, useState } from 'react';
export const clampBoardZoom=(value:number)=>Math.min(2,Math.max(.35,value));
export function useBoardZoom() {
  const canvasRef=useRef<HTMLDivElement>(null);
  const [zoom,setZoom]=useState(1);
  const current=useRef(1),right=useRef(false),used=useRef(false);
  const changeZoom=(value:number,x?:number,y?:number)=> {
    const next=clampBoardZoom(value),canvas=canvasRef.current,old=current.current;
    current.current=next;setZoom(next);
    if (canvas) { const r=canvas.getBoundingClientRect(),ax=x===undefined?canvas.clientWidth/2:x-r.left,ay=y===undefined?canvas.clientHeight/2:y-r.top;
      const left=(canvas.scrollLeft+ax)*next/old-ax,top=(canvas.scrollTop+ay)*next/old-ay;
      requestAnimationFrame(()=>{canvas.scrollLeft=left;canvas.scrollTop=top;});
    }
  };
  useEffect(()=> {
    const canvas=canvasRef.current;if(!canvas)return;
    let gesture=false,start=1;
    let pendingContext:{target:EventTarget|null;x:number;y:number}|null=null;
    const down=(e:MouseEvent)=>{if(e.button===2){right.current=true;used.current=false;pendingContext=null;}};
    const release=(e:MouseEvent)=>{
      if(e.button!==2&&(e.buttons&2)!==0)return;
      right.current=false;
      const pending=pendingContext;pendingContext=null;
      // WebKit can open a context menu on right-button DOWN. Defer the app's
      // menu until release so it cannot swallow the wheel while held.
      if(pending&&!used.current&&pending.target instanceof Node&&pending.target.isConnected)
        pending.target.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2,clientX:pending.x,clientY:pending.y}));
    };
    const reset=()=>{right.current=false;pendingContext=null;used.current=false;};
    const wheel=(e:WheelEvent)=>{
      if(!(right.current||(e.buttons&2)!==0||e.ctrlKey||e.metaKey))return;
      e.preventDefault();e.stopPropagation();if(gesture)return;used.current=true;pendingContext=null;
      const delta=e.deltaY*(e.deltaMode===1?16:e.deltaMode===2?canvas.clientHeight:1);
      changeZoom(current.current*Math.exp(-delta*.003),e.clientX,e.clientY);
    };
    const context=(e:MouseEvent)=>{
      e.preventDefault();
      if(right.current){pendingContext={target:e.target,x:e.clientX,y:e.clientY};e.stopPropagation();}
      else if(used.current)e.stopPropagation();
    };
    const begin=(e:Event)=>{e.preventDefault();gesture=true;start=current.current;};
    const update=(e:Event)=>{e.preventDefault();const g=e as Event & {scale:number;clientX?:number;clientY?:number};if(Number.isFinite(g.scale))changeZoom(start*g.scale,g.clientX,g.clientY);};
    const end=()=>{gesture=false;};
    const captureWheel={passive:false,capture:true};
    canvas.addEventListener('pointerdown',down,true);canvas.addEventListener('mousedown',down,true);
    window.addEventListener('pointerup',release,true);window.addEventListener('mouseup',release,true);window.addEventListener('pointercancel',reset,true);window.addEventListener('blur',reset);
    canvas.addEventListener('wheel',wheel,captureWheel);canvas.addEventListener('contextmenu',context,true);canvas.addEventListener('gesturestart',begin,{passive:false});canvas.addEventListener('gesturechange',update,{passive:false});canvas.addEventListener('gestureend',end);
    return ()=>{canvas.removeEventListener('pointerdown',down,true);canvas.removeEventListener('mousedown',down,true);window.removeEventListener('pointerup',release,true);window.removeEventListener('mouseup',release,true);window.removeEventListener('pointercancel',reset,true);window.removeEventListener('blur',reset);canvas.removeEventListener('wheel',wheel,true);canvas.removeEventListener('contextmenu',context,true);canvas.removeEventListener('gesturestart',begin);canvas.removeEventListener('gesturechange',update);canvas.removeEventListener('gestureend',end);reset();};
  },[]);
  return {canvasRef,zoom,changeZoom};
}
