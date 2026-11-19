import {useEffect,useState} from 'react';
import {ipc} from '@/ipc/client';
import {tr} from '@/i18n';
import {useEditorStore} from '@/stores/editor';

type ExpertInfo={id:string;name:string;description:string;dimensions:string[];builtin:boolean;enabled:boolean};
type SkillInfo={id:string;name:string;description:string;builtin:boolean};

/** 设置 → 编剧助理：专家库的启用/停用、改名、导入导出，以及技能的只读展示。 */
export default function AssistantSettings(){
 const toast=useEditorStore(s=>s.pushToast);
 const [experts,setExperts]=useState<ExpertInfo[]>([]);
 const [issues,setIssues]=useState<string[]>([]);
 const [skills,setSkills]=useState<SkillInfo[]>([]);
 const [busy,setBusy]=useState('');
 const [renaming,setRenaming]=useState('');
 const [renameText,setRenameText]=useState('');
 const reload=()=>void ipc.agentListExperts().then(r=>{setExperts(r.experts);setIssues(r.issues);}).catch(e=>toast('error',String(e)));
 useEffect(()=>{reload();void ipc.agentListSkills().then(setSkills).catch(()=>{});},[]);
 const run=async(key:string,fn:()=>Promise<unknown>,success?:string)=>{setBusy(key);try{await fn();if(success)toast('success',success);reload();window.dispatchEvent(new Event('experts-changed'));}catch(e){toast('error',String(e));}finally{setBusy('');}};
 const openDir=async(id?:string)=>{try{await ipc.agentOpenExpertsDir(id);}catch(e){toast('error',String(e));}};
 const importExpert=async()=>{try{const {open}=await import('@tauri-apps/plugin-dialog');const path=await open({multiple:false,title:tr("选择 EXPERT.md 或包含它的文件夹"),filters:[{name:'EXPERT.md',extensions:['md']}]});if(typeof path!=='string')return;await run('import',()=>ipc.agentImportExpert(path),tr("专家已导入，已出现在助理面板中"));}catch(e){toast('error',String(e));}};
 const exportExpert=async(id:string,name:string)=>{try{const {save}=await import('@tauri-apps/plugin-dialog');const path=await save({defaultPath:`${name}.md`,filters:[{name:'EXPERT.md',extensions:['md']}]});if(!path)return;await run('export-'+id,()=>ipc.agentExportExpert(id,path),tr("专家已导出"));}catch(e){toast('error',String(e));}};
 const builtin=experts.filter(e=>e.builtin),custom=experts.filter(e=>!e.builtin);
 const row=(e:ExpertInfo)=><li key={e.id} className={'expert-admin-row'+(e.enabled?'':' is-off')}>
  <div className="expert-admin-head">
   <strong>{e.name}</strong>
   <span className="expert-admin-badge">{e.builtin?tr("内置"):tr("自定义")}</span>
   {!e.enabled&&<span className="expert-admin-badge off">{tr("已停用")}</span>}
  </div>
  <p className="expert-admin-desc">{e.description||tr("（未填写说明）")}</p>
  <p className="expert-admin-dims">{e.dimensions.join(' · ')}</p>
  <div className="expert-admin-ops">
   <label className="expert-admin-toggle"><input type="checkbox" checked={e.enabled} disabled={busy==='toggle-'+e.id} onChange={ev=>void run('toggle-'+e.id,()=>ipc.agentSetExpertEnabled(e.id,ev.target.checked),ev.target.checked?tr("已启用"):tr("已停用"))}/>{tr("启用")}</label>
   {!e.builtin&&renaming!==e.id&&<button disabled={!!busy} onClick={()=>{setRenaming(e.id);setRenameText(e.name);}}>{tr("改名")}</button>}
   {!e.builtin&&<button disabled={!!busy} onClick={()=>void openDir(e.id)}>{tr("所在文件夹")}</button>}
   {!e.builtin&&<button disabled={!!busy} onClick={()=>void exportExpert(e.id,e.name)}>{tr("导出…")}</button>}
   {renaming===e.id&&<span className="expert-admin-rename"><input aria-label={tr("新名称")} value={renameText} maxLength={60} onChange={ev=>setRenameText(ev.target.value)} onKeyDown={ev=>{if(ev.key==='Enter'){void run('rename',()=>ipc.agentRenameExpert(e.id,renameText),tr("已改名")).then(()=>setRenaming(''));}if(ev.key==='Escape')setRenaming('');}}/><button disabled={!!busy} onClick={()=>void run('rename',()=>ipc.agentRenameExpert(e.id,renameText),tr("已改名")).then(()=>setRenaming(''))}>{tr("保存")}</button><button disabled={!!busy} onClick={()=>setRenaming('')}>{tr("取消")}</button></span>}
  </div>
 </li>;
 return <div className="assistant-settings">
  <h3>{tr("编剧助理")}</h3>
  <p className="modal-hint">{tr("专家在设置里管理，在助理面板里使用。内置专家随版本提供；自定义专家以 EXPERT.md 单文件保存，导入导出都沿用这个格式。")}</p>
  <div className="expert-admin-toolbar">
   <button disabled={!!busy} onClick={()=>void openDir()}>{tr("打开专家文件夹")}</button>
   <button disabled={!!busy} onClick={()=>void importExpert()}>{tr("导入专家…")}</button>
  </div>
  {!!issues.length&&<div className="expert-error"><p>{tr("以下自定义专家未能载入：")}</p>{issues.map(issue=><p key={issue}>{issue}</p>)}</div>}
  <h4>{tr("内置专家")} · {builtin.length}</h4>
  <ul className="expert-admin-list">{builtin.map(row)}</ul>
  <h4>{tr("自定义专家")} · {custom.length}</h4>
  {custom.length?<ul className="expert-admin-list">{custom.map(row)}</ul>:<p className="modal-hint">{tr("还没有自定义专家。可以在会审结束后点「存成我的专家」，或导入一个 EXPERT.md。")}</p>}
  <h4>{tr("技能（只读）")}</h4>
  <p className="modal-hint">{tr("技能负责片段级的写作动作。这里只读展示已安装技能；当前不支持在本页编辑或新建，安装与管理仍在「模型与技能」中。")}</p>
  <div className="expert-skill-list">{skills.map(s=><div className="expert-skill-row" key={s.id}><strong>{s.name}</strong><span className="expert-admin-badge">{s.builtin?tr("内置"):tr("自定义")}</span><p>{s.description}</p></div>)}</div>
 </div>;
}
