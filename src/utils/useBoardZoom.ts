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
    const down=(e:PointerEvent)=>{if(e.button===2){right.current=true;used.current=false;}};
    const up=()=>{right.current=false;};
    const wheel=(e:WheelEvent)=>{if(!right.current && !e.ctrlKey && !e.metaKey)return;e.preventDefault();if(gesture)return;used.current=true;const delta=e.deltaY*(e.deltaMode===1?16:e.deltaMode===2?canvas.clientHeight:1);changeZoom(current.current*Math.exp(-delta*.003),e.clientX,e.clientY);};
    const context=(e:Event)=>{if(used.current){e.preventDefault();used.current=false;}};
    const begin=(e:Event)=>{e.preventDefault();gesture=true;start=current.current;};
    const update=(e:Event)=>{e.preventDefault();const g=e as Event & {scale:number;clientX?:number;clientY?:number};if(Number.isFinite(g.scale))changeZoom(start*g.scale,g.clientX,g.clientY);};
    const end=()=>{gesture=false;};
    canvas.addEventListener('pointerdown',down);window.addEventListener('pointerup',up);window.addEventListener('pointercancel',up);window.addEventListener('blur',up);
    canvas.addEventListener('wheel',wheel,{passive:false});canvas.addEventListener('contextmenu',context);canvas.addEventListener('gesturestart',begin,{passive:false});canvas.addEventListener('gesturechange',update,{passive:false});canvas.addEventListener('gestureend',end);
    return ()=>{canvas.removeEventListener('pointerdown',down);window.removeEventListener('pointerup',up);window.removeEventListener('pointercancel',up);window.removeEventListener('blur',up);canvas.removeEventListener('wheel',wheel);canvas.removeEventListener('contextmenu',context);canvas.removeEventListener('gesturestart',begin);canvas.removeEventListener('gesturechange',update);canvas.removeEventListener('gestureend',end);};
  },[]);
  return {canvasRef,zoom,changeZoom};
}
