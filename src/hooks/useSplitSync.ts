import {useEffect,useRef} from 'react';
import {useEditorStore} from '@/stores/editor';
import {correspondingScroll,sourceBlockAt,sourceSegments,type ScrollAnchor} from '@/utils/sourcePositions';

/** Synchronize by paragraph identity, not page percentage; never move the other caret. */
export function useSplitSync(){
 const expectedRef=useRef(new WeakMap<HTMLElement,number>());
 const mode=useEditorStore(s=>s.viewMode),doc=useEditorStore(s=>s.document),text=useEditorStore(s=>s.sourceText),active=useEditorStore(s=>s.activeBlockId);
 useEffect(()=>{
  if(mode!=='split'||!doc)return;
  let teardown=()=>{};let boundSource:HTMLElement|null=null,boundPaper:HTMLElement|null=null;
  const bind=()=>{
  const source=document.querySelector<HTMLTextAreaElement>('.source-editor textarea'),paper=document.querySelector<HTMLElement>('.paper-workspace .screenplay-editor');
  if(source===boundSource&&paper===boundPaper)return;
  teardown();boundSource=source;boundPaper=paper;
  if(!source||!paper)return;
  const ranges=sourceSegments(text,doc.blocks),expected=expectedRef.current;let frame=0;
  const positions=(kind:'source'|'paper'):ScrollAnchor[]=>{
   const scroller=kind==='source'?source:paper,box=scroller.getBoundingClientRect(),scale=kind==='paper'&&scroller.offsetWidth?box.width/scroller.offsetWidth:1;
   const elements=kind==='source'?document.querySelectorAll<HTMLElement>('.source-text-layer [data-source-blocks]'):paper.querySelectorAll<HTMLElement>('[data-block-id]');
   const anchors:ScrollAnchor[]=[];
   elements.forEach(el=>{const rect=el.getBoundingClientRect();const ids=kind==='source'?(el.dataset.sourceBlocks??'').split(' '):[el.dataset.blockId!];ids.forEach(id=>anchors.push({id,top:scroller.scrollTop+(rect.top-box.top)/(scale||1),height:rect.height/(scale||1)}));});
   const merged=new Map<string,ScrollAnchor>();for(const anchor of anchors){const previous=merged.get(anchor.id);if(previous){const end=Math.max(previous.top+previous.height,anchor.top+anchor.height);previous.top=Math.min(previous.top,anchor.top);previous.height=end-previous.top;}else merged.set(anchor.id,anchor);}
   return [...merged.values()].sort((a,b)=>a.top-b.top);
  };
  const move=(el:HTMLElement,top:number)=>{el.scrollTop=top;expected.set(el,el.scrollTop);};
  const onScroll=(kind:'source'|'paper')=>{
   const driver=kind==='source'?source:paper,target=kind==='source'?paper:source;
   const ignored=expected.get(driver);if(ignored!==undefined){expected.delete(driver);if(Math.abs(driver.scrollTop-ignored)<2)return;}
   if(source.value!==text)return; // Uncommitted input has no authoritative ranges yet.
   cancelAnimationFrame(frame);frame=requestAnimationFrame(()=>{
    const top=correspondingScroll(positions(kind),positions(kind==='source'?'paper':'source'),driver.scrollTop,driver.clientHeight,target.clientHeight);
    if(top!==null)move(target,top);
   });
  };
  const sourceScroll=()=>onScroll('source'),paperScroll=()=>onScroll('paper');
  const select=()=>{if(source.value===text){const id=sourceBlockAt(ranges,source.selectionStart);if(id)useEditorStore.getState().setActiveBlock(id);}};
  source.addEventListener('scroll',sourceScroll);paper.addEventListener('scroll',paperScroll);
  ['select','keyup','pointerup'].forEach(name=>source.addEventListener(name,select));
  // Selecting an element reveals its counterpart without stealing focus or selection.
  const focused=document.activeElement;
  if(focused===source&&source.value===text)select();
  if(active&&(focused===source||paper.contains(focused))){
   const kind=focused===source?'paper':'source',target=kind==='paper'?paper:source;
   const anchor=positions(kind).find(a=>a.id===active);
   if(anchor&&(anchor.top<target.scrollTop+24||anchor.top+anchor.height>target.scrollTop+target.clientHeight-24))move(target,Math.max(0,anchor.top-target.clientHeight*.3));
  }
  teardown=()=>{cancelAnimationFrame(frame);source.removeEventListener('scroll',sourceScroll);paper.removeEventListener('scroll',paperScroll);['select','keyup','pointerup'].forEach(name=>source.removeEventListener(name,select));};
  };
  bind();const observer=new MutationObserver(bind);observer.observe(document.querySelector('.editors')??document.body,{childList:true,subtree:true});
  return()=>{observer.disconnect();teardown();};
 },[mode,doc,text,active]);
}
