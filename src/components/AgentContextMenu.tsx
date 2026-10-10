import {tr,fmt} from '@/i18n';
import {useEffect,useState} from 'react';
import {useEditorStore} from '@/stores/editor';
import {flushPendingEdits} from '@/utils/pendingEdits';
import {sendSceneToAgent,type AgentTarget} from '@/utils/agentSelection';
export default function AgentContextMenu(){
 const [menu,setMenu]=useState<{x:number;y:number;send:()=>Promise<void>}|null>(null);
 useEffect(()=>{const context=(e:MouseEvent)=>{
 if(e.defaultPrevented)return;
 const el=e.target as HTMLElement,scene=el.closest<HTMLElement>('[data-scene-id]');
 const area=el.closest<HTMLTextAreaElement>('.source-editor textarea');
 const row=el.closest<HTMLElement>('[data-block-id]');
 if(!scene&&!area&&!row)return;
 const selection=window.getSelection();
 let fullText='',text='',start=0,end=0,blockId:string|undefined,blockIds:string[]|undefined;
 const selectedRows=selection&&!selection.isCollapsed&&selection.rangeCount?Array.from(document.querySelectorAll<HTMLElement>('.screenplay-editor [data-block-id]')).filter(n=>selection.getRangeAt(0).intersectsNode(n)):[];
 if(area){fullText=area.value;start=area.selectionStart;end=area.selectionEnd;if(start===end)return;text=fullText.slice(start,end);}
 else if(selectedRows.length>1){blockIds=selectedRows.map(n=>n.dataset.blockId!).filter(Boolean);}
 else if(row){const editable=row.querySelector<HTMLElement>('.screenplay-block');if(!editable)return;fullText=editable.innerText;blockId=row.dataset.blockId;
 if(selection?.rangeCount&&!selection.isCollapsed&&editable.contains(selection.anchorNode)&&editable.contains(selection.focusNode)){const range=selection.getRangeAt(0),prefix=range.cloneRange();prefix.selectNodeContents(editable);prefix.setEnd(range.startContainer,range.startOffset);start=prefix.toString().length;text=range.toString();end=start+text.length;}else{text=fullText;end=text.length;}}
 e.preventDefault();setMenu({x:Math.min(e.clientX+10,window.innerWidth-230),y:Math.min(e.clientY,window.innerHeight-80),send:async()=>{
 if(scene){await sendSceneToAgent(scene.dataset.sceneId!);return;}
 await flushPendingEdits();const doc=useEditorStore.getState().document;if(!doc)return;
 if(blockIds){const ids=blockIds;text=doc.blocks.filter(b=>ids.includes(b.id)).map(b=>b.block_type==='scene_heading'?'.'+b.text:b.block_type==='character'?'@'+b.text:b.text).join('\n\n');}
 const target:AgentTarget={blockIds,label:blockIds?fmt("所选 {0} 个完整段落",blockIds.length):area?tr("自由模式所选文字"):tr("标准剧本所选文字"),projectId:doc.id,revision:doc.revision,text,fullText,start,end,blockId};
 window.dispatchEvent(new CustomEvent('agent-selection',{detail:target}));
 }});};
 const close=()=>setMenu(null);const key=(e:KeyboardEvent)=>{if(e.key==='Escape')close();};document.addEventListener('contextmenu',context);window.addEventListener('pointerdown',close);window.addEventListener('wheel',close);window.addEventListener('keydown',key);return()=>{document.removeEventListener('contextmenu',context);window.removeEventListener('pointerdown',close);window.removeEventListener('wheel',close);window.removeEventListener('keydown',key);};},[]);
 return menu?<div className="agent-context-menu" role="menu" style={{left:menu.x,top:menu.y}} onPointerDown={e=>e.stopPropagation()}><button role="menuitem" onClick={()=>{void menu.send().catch(e=>useEditorStore.getState().pushToast('error',String(e)));setMenu(null);}}>{tr("发送给 Agent 修改…")}</button></div>:null;
}
