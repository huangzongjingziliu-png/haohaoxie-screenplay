import {tr} from '@/i18n';
import {useState} from 'react';
import {ipc,onImportProgress} from '@/ipc/client';
import AgentWorkStatus from './AgentWorkStatus';
import {useEditorStore} from '@/stores/editor';
import {flushPendingEdits} from '@/utils/pendingEdits';
import type {AgentTarget} from '@/utils/agentSelection';
export default function AgentSelectionPanel({target,onClose,onConfigure}:{target:AgentTarget;onClose:()=>void;onConfigure:()=>void}){
 const [instruction,setInstruction]=useState(target.instruction??(target.source?tr("忠实整理为标准剧本格式，保持剧情与对白。"):tr("润色所选内容，保持原意、情节与剧本格式。")));const [proposal,setProposal]=useState('');const [busy,setBusy]=useState(false);const [error,setError]=useState('');
 const [progress,setProgress]=useState<{completed:number;total:number}|null>(null);
 const run=async()=>{setBusy(true);setError('');setProgress(null);const id=crypto.randomUUID();let unlisten:(()=>void)|undefined;try{unlisten=await onImportProgress(e=>{if(e.requestId===id)setProgress(e);});setProposal(await ipc.agentTransformText(target.text,instruction,!!target.source,id));}catch(e){setError(String(e));}finally{unlisten?.();setBusy(false);}};
 const apply=async()=>{setBusy(true);setError('');try{await flushPendingEdits();const store=useEditorStore.getState(),doc=store.document;if(doc?.id!==target.projectId||doc.revision!==target.revision)throw new Error(tr("剧本在预览期间发生了变化，请重新选择并生成，避免覆盖您的修改。"));
 let result;
 if(target.blockIds)result=await ipc.agentApplySelection(target.blockIds,proposal,target.revision);
 else if(target.blockId) result=await ipc.agentApplyText((target.fullText??'').slice(0,target.start)+proposal+(target.fullText??'').slice(target.end),target.revision,target.blockId);
 else result=await ipc.agentApplyText((target.fullText??'').slice(0,target.start)+proposal+(target.fullText??'').slice(target.end),target.revision);
 await store.applyResult(result);onClose();}catch(e){setError(String(e));}finally{setBusy(false);}};
 return <div className="modal-backdrop">{busy&&<AgentWorkStatus label={progress?tr("正在逐段转换剧本…"):tr("正在准备处理…")} completed={progress?.completed} total={progress?.total}/>}<section className="modal agent-selection-dialog" role="dialog" aria-modal="true" aria-label={tr("局部修改预览")}><h2>{target.source?tr("附件转剧本"):tr("局部修改")}</h2><p>{target.label} · {target.text.length}{tr(" 字")}</p><p className="modal-hint">{target.blockIds?tr("将处理所选的完整段落。"):''}{tr("所选内容会发送给您配置的模型。生成后可编辑预览，应用可撤销。")}</p><label>{tr("修改要求")}<textarea aria-label={tr("局部修改要求")} value={instruction} onChange={e=>setInstruction(e.target.value)}/></label><details><summary>{tr("查看原文")}</summary><pre>{target.text}</pre></details>{error&&<p role="alert">{error}</p>}<button disabled={busy} onClick={onConfigure}>{tr("配置模型")}</button><button disabled={busy||!instruction.trim()} onClick={()=>void run()}>{busy?tr("处理中…"):tr("生成预览")}</button>{proposal&&<><label>{tr("修改结果")}<textarea aria-label={tr("局部修改结果")} value={proposal} onChange={e=>setProposal(e.target.value)}/></label><button disabled={busy||!proposal.trim()} onClick={()=>void apply()}>{target.source?tr("替换当前剧本"):tr("应用到所选范围")}</button></>}<button onClick={onClose}>{tr("取消")}</button></section></div>;
}
