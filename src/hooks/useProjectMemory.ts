import {useEffect} from 'react';
import {ipc} from '@/ipc/client';
import {useEditorStore} from '@/stores/editor';
import {flushPendingEdits} from '@/utils/pendingEdits';
export function useProjectMemory(){
 const notes=useEditorStore(s=>s.document?.settings?.project_memory?.author_notes);
 const id=useEditorStore(s=>s.document?.id),source=useEditorStore(s=>s.sourceText),enabled=useEditorStore(s=>s.document?.settings.project_memory?.auto_enabled??true);
 useEffect(()=>{
  if(!id||!enabled)return;
  let timer:ReturnType<typeof setTimeout>;
  let disposed=false;
  const run=async(discussion?:string)=>{
   if(disposed||useEditorStore.getState().document?.id!==id)return;
   const status=(text:string)=>window.dispatchEvent(new CustomEvent('project-memory-status',{detail:text}));
   try{const models=await ipc.agentListModels();if(!models.activeId){status('请配置模型后自动整理项目记忆');return;}await flushPendingEdits();status('正在整理项目记忆…');await ipc.agentRefreshMemory(id,discussion);if(!disposed)status('项目记忆已更新');}
   catch(e){if(!disposed)status(String(e));}
  };
  const schedule=()=>{clearTimeout(timer);timer=setTimeout(()=>void run(),20000);};
  const chat=(e:Event)=>{clearTimeout(timer);void run((e as CustomEvent<string>).detail);};
  schedule();window.addEventListener('models-changed',schedule);window.addEventListener('project-memory-discussion',chat);
  return()=>{disposed=true;clearTimeout(timer);window.removeEventListener('models-changed',schedule);window.removeEventListener('project-memory-discussion',chat);};
 },[id,source,enabled,notes]);
}
