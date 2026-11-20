import {useEffect,useState} from 'react';
import {ipc} from '@/ipc/client';
import {tr} from '@/i18n';
import {useEditorStore} from '@/stores/editor';
import SkillCreator from './SkillCreator';

type SkillInfo={id:string;name:string;description:string;dimensions:string[];kind:'review'|'writing';builtin:boolean;enabled:boolean};

/** 设置 → 编剧助理 → 技能管理：启用/停用、改名、导入导出。 */
export default function SkillSettings({creating,onCreating}:{creating:boolean;onCreating:(value:boolean)=>void}){
 const toast=useEditorStore(s=>s.pushToast);
 const [skills,setSkills]=useState<SkillInfo[]>([]);
 const [issues,setIssues]=useState<string[]>([]);
 const [busy,setBusy]=useState('');
 const [renaming,setRenaming]=useState('');
 const [renameText,setRenameText]=useState('');
 const reload=()=>void ipc.agentListExperts().then(r=>{setSkills(r.experts);setIssues(r.issues);}).catch(e=>toast('error',String(e)));
 useEffect(()=>{reload();},[]);
 const run=async(key:string,fn:()=>Promise<unknown>,success?:string)=>{setBusy(key);try{await fn();if(success)toast('success',success);reload();window.dispatchEvent(new Event('skills-changed'));}catch(e){toast('error',String(e));}finally{setBusy('');}};
 const openDir=async(id?:string)=>{try{await ipc.agentOpenExpertsDir(id);}catch(e){toast('error',String(e));}};
 const importSkill=async()=>{try{const {open}=await import('@tauri-apps/plugin-dialog');const path=await open({multiple:false,title:tr("选择 SKILL.md（旧版 EXPERT.md 也可以）"),filters:[{name:'SKILL.md',extensions:['md']}]});if(typeof path!=='string')return;await run('import',()=>ipc.agentImportExpert(path),tr("技能已导入，已出现在助理面板中"));}catch(e){toast('error',String(e));}};
 const exportSkill=async(id:string,name:string)=>{try{const {save}=await import('@tauri-apps/plugin-dialog');const path=await save({defaultPath:`${name}.md`,filters:[{name:'SKILL.md',extensions:['md']}]});if(!path)return;await run('export-'+id,()=>ipc.agentExportExpert(id,path),tr("技能已导出"));}catch(e){toast('error',String(e));}};
 const builtin=skills.filter(s=>s.builtin),custom=skills.filter(s=>!s.builtin);
 const row=(s:SkillInfo)=><li key={s.id} className={'expert-admin-row'+(s.enabled?'':' is-off')}>
  <div className="expert-admin-head">
   <strong>{s.name}</strong>
   <span className="expert-admin-badge">{s.builtin?tr("内置"):tr("自定义")}</span>
   <span className={'expert-admin-badge kind'+(s.kind==='writing'?' writing':'')}>{s.kind==='writing'?tr("写作型"):tr("审稿型")}</span>
   {!s.enabled&&<span className="expert-admin-badge off">{tr("已停用")}</span>}
  </div>
  <p className="expert-admin-desc">{s.description||tr("（未填写说明）")}</p>
  <p className="expert-admin-dims">{s.dimensions.join(' · ')}</p>
  <div className="expert-admin-ops">
   <label className="expert-admin-toggle"><input type="checkbox" checked={s.enabled} disabled={busy==='toggle-'+s.id} onChange={ev=>void run('toggle-'+s.id,()=>ipc.agentSetExpertEnabled(s.id,ev.target.checked),ev.target.checked?tr("已启用"):tr("已停用"))}/>{tr("启用")}</label>
   {!s.builtin&&renaming!==s.id&&<button disabled={!!busy} onClick={()=>{setRenaming(s.id);setRenameText(s.name);}}>{tr("改名")}</button>}
   {!s.builtin&&<button disabled={!!busy} onClick={()=>void openDir(s.id)}>{tr("所在文件夹")}</button>}
   {!s.builtin&&<button disabled={!!busy} onClick={()=>void exportSkill(s.id,s.name)}>{tr("导出…")}</button>}
   {renaming===s.id&&<span className="expert-admin-rename"><input aria-label={tr("新名称")} value={renameText} maxLength={60} onChange={ev=>setRenameText(ev.target.value)} onKeyDown={ev=>{if(ev.key==='Enter'){void run('rename',()=>ipc.agentRenameExpert(s.id,renameText),tr("已改名")).then(()=>setRenaming(''));}if(ev.key==='Escape')setRenaming('');}}/><button disabled={!!busy} onClick={()=>void run('rename',()=>ipc.agentRenameExpert(s.id,renameText),tr("已改名")).then(()=>setRenaming(''))}>{tr("保存")}</button><button disabled={!!busy} onClick={()=>setRenaming('')}>{tr("取消")}</button></span>}
  </div>
 </li>;
 return <div>
  <h3>{tr("技能管理")}</h3>
  <p className="modal-hint">{tr("技能在设置里管理，在助理面板里使用。内置技能随版本提供；自定义技能以 SKILL.md 单文件保存，旧版 EXPERT.md 仍可读取。")}</p>
  <p className="modal-hint">{tr("类型：审稿型参加会审，对文本提判断与修改建议；写作型出现在助理对话里，做单步写作或改写。")}</p>
  <div className="expert-admin-toolbar">
   <button disabled={!!busy} onClick={()=>void openDir()}>{tr("打开技能文件夹")}</button>
   <button disabled={!!busy} onClick={()=>void importSkill()}>{tr("导入技能…")}</button>
   <button disabled={!!busy||creating} onClick={()=>onCreating(true)}>{tr("创建技能")}</button>
  </div>
  {!!issues.length&&<div className="expert-error"><p>{tr("以下自定义技能未能载入：")}</p>{issues.map(issue=><p key={issue}>{issue}</p>)}</div>}
  <h4>{tr("内置技能")} · {builtin.length}</h4>
  <ul className="expert-admin-list">{builtin.map(row)}</ul>
  <h4>{tr("自定义技能")} · {custom.length}</h4>
  {custom.length?<ul className="expert-admin-list">{custom.map(row)}</ul>:<p className="modal-hint">{tr("还没有自定义技能。可以在会审结束后点「存成我的技能」，或导入一个 SKILL.md。")}</p>}
  {creating&&<SkillCreator onClose={()=>onCreating(false)}/>}
 </div>;
}
