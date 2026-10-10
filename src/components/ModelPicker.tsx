import {tr} from '@/i18n';
import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { ipc, type ModelProfile, type ModelProfiles } from '@/ipc/client';
import { useEditorStore } from '@/stores/editor';
const blank = (): ModelProfile & { apiKey: string } => ({ id: '', name: '', providerKind: 'compatible', model: '', baseUrl: '', apiKey: '' });
export default function ModelPicker({manage=false}:{manage?:boolean}={}) {
 const [all,setAll]=useState<ModelProfiles>({activeId:'',profiles:[]});
 const [open,setOpen]=useState(false),[edit,setEdit]=useState<ReturnType<typeof blank>|null>(null),[busy,setBusy]=useState(false);
 const toast=useEditorStore(s=>s.pushToast);
 const refresh=()=>{void ipc.agentListModels().then(setAll).catch(()=>{});};
 useEffect(()=>{refresh();window.addEventListener('models-changed',refresh);return()=>window.removeEventListener('models-changed',refresh);},[]);
 const run=async(action:()=>Promise<ModelProfiles>)=>{setBusy(true);try{setAll(await action());setEdit(null);window.dispatchEvent(new Event('models-changed'));}catch(e){toast('error',String(e));}finally{setBusy(false);}};
 const active=all.profiles.find(p=>p.id===all.activeId);
 return <>
 {manage?<button className="model-picker-button" onClick={()=>setOpen(true)}>{tr("管理自定义模型…")}</button>:<select className="model-picker-button" aria-label={tr("选择模型")} value={all.activeId} onChange={e=>{if(e.target.value==='__settings')window.dispatchEvent(new Event('open-model-settings'));else void run(()=>ipc.agentSelectModel(e.target.value));}}><option value="" disabled>{tr("选择模型")}</option>{all.profiles.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}<option value="__settings">{tr("配置模型…")}</option></select>}
 {open&&createPortal(<div className="modal-backdrop" style={{zIndex:1300}}><div className="modal model-manager" role="dialog" aria-label={tr("自定义模型")}>
 <h2>{tr("自定义模型")}</h2>
 <p className="modal-hint">{tr("密钥输入一次即可记住；您可以保存多个模型，并随时切换。")}</p>
 {all.profiles.map(p=><div key={p.id} className={'model-choice '+(p.id===all.activeId?'selected':'')}>
 <button disabled={busy} onClick={()=>void run(()=>ipc.agentSelectModel(p.id))}>◇ {p.name}<small>{p.model}</small>{p.id===all.activeId?' ✓':''}</button>
 <button disabled={busy} onClick={()=>setEdit({...p,apiKey:''})}>{tr("编辑")}</button>
 <button disabled={busy} onClick={()=>{if(window.confirm(tr("删除此模型配置？剧本内容不会改变。")))void run(()=>ipc.agentDeleteModel(p.id));}}>{tr("删除")}</button>
 </div>)}
 {!edit&&<button disabled={busy} onClick={()=>setEdit(blank())}>{tr("＋ 添加自定义模型")}</button>}
 {edit&&<form onSubmit={e=>{e.preventDefault();void run(()=>ipc.agentSaveModel(edit));}}>
 <label>{tr("显示名称")}<input required value={edit.name} onChange={e=>setEdit({...edit,name:e.target.value})} placeholder={tr("例如 Agnes 3.0 Flash")}/></label>
 <label>{tr("模型类型")}<select value={edit.providerKind} onChange={e=>setEdit({...edit,providerKind:e.target.value})}><option value="compatible">{tr("兼容 OpenAI 的服务")}</option><option value="openai">OpenAI</option><option value="ollama">{tr("本机 Ollama")}</option></select></label>
 <label>{tr("模型 ID")}<input required value={edit.model} onChange={e=>setEdit({...edit,model:e.target.value})} placeholder="agnes-3.0-flash"/></label>
 {edit.providerKind==='compatible'&&<label>{tr("服务地址")}<input required value={edit.baseUrl} onChange={e=>setEdit({...edit,baseUrl:e.target.value})} placeholder="https://apihub.agnes-ai.com/v1"/></label>}
 {edit.providerKind!=='ollama'&&<label>API Key<input type="password" autoComplete="off" required={!edit.id} value={edit.apiKey} onChange={e=>setEdit({...edit,apiKey:e.target.value})} placeholder={edit.id?tr("已保存，留空保留原密钥"):tr("输入一次，本机加密保存")}/></label>}
 <p className="modal-hint">{tr("密钥仅保存在这台电脑，普通配置和剧本文件中不含密钥。")}</p>
 <div className="modal-actions"><button type="button" disabled={busy} onClick={()=>setEdit(null)}>{tr("取消")}</button><button disabled={busy} type="submit">{busy?tr("保存中…"):tr("保存并使用")}</button></div>
 </form>}
 <div className="modal-actions"><button disabled={busy} onClick={()=>{setOpen(false);setEdit(null);}}>{tr("完成")}</button></div>
 </div></div>,document.body)}
 </>;
}
