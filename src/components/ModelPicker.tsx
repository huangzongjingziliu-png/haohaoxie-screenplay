import ModalBackdrop from './ModalBackdrop';
import ModelSetup from './ModelSetup';
import {tr} from '@/i18n';
import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { ipc, type ModelProfile, type ModelProfiles } from '@/ipc/client';
import { useEditorStore } from '@/stores/editor';
const blank = (): ModelProfile & { apiKey: string } => ({ id: '', name: '', providerKind: 'compatible', model: '', baseUrl: '', apiKey: '' });
export default function ModelPicker({manage=false,standalone=false,onClose}:{manage?:boolean;standalone?:boolean;onClose?:()=>void}={}) {
 const [cloudAllowed,setCloudAllowed]=useState(false);
 const [all,setAll]=useState<ModelProfiles>({activeId:'',profiles:[]});
 const [open,setOpen]=useState(standalone),[edit,setEdit]=useState<ReturnType<typeof blank>|null>(null),[busy,setBusy]=useState(false);
 const [test,setTest]=useState<{id:string;message:string;ok:boolean}|null>(null);
 const [adding,setAdding]=useState(false);
 const [freeFirst,setFreeFirst]=useState(false);
 const [setupDirty,setSetupDirty]=useState(false),[setupPending,setSetupPending]=useState(false);
 const testConnection=async(p:ModelProfile&{apiKey?:string})=>{setBusy(true);setTest({id:p.id||'draft',message:tr("正在测试连接…"),ok:false});try{const r=await ipc.agentTestModel(p);setTest({id:p.id||'draft',message:tr("API 可用，已收到模型响应")+' · '+r.model+' · '+r.latencyMs+' ms',ok:true});}catch(e){setTest({id:p.id||'draft',message:tr("连接失败：")+String(e),ok:false});}finally{setBusy(false);}};
 const close=()=>{setOpen(false);setEdit(null);setAdding(false);onClose?.();};
 const toast=useEditorStore(s=>s.pushToast);
 const refresh=()=>{void ipc.agentListModels().then(setAll).catch(()=>{});if(standalone)void ipc.agentGetConfig().then(c=>setCloudAllowed(c.cloudAiEnabled)).catch(()=>{});};
 useEffect(()=>{refresh();window.addEventListener('models-changed',refresh);return()=>window.removeEventListener('models-changed',refresh);},[]);
 const run=async(action:()=>Promise<ModelProfiles>)=>{setBusy(true);try{setAll(await action());setEdit(null);window.dispatchEvent(new Event('models-changed'));return true;}catch(e){toast('error',String(e));return false;}finally{setBusy(false);}};
 const active=all.profiles.find(p=>p.id===all.activeId);
 return <>
 {!standalone&&(manage?<button className="model-picker-button" onClick={()=>setOpen(true)}>{tr("管理自定义模型…")}</button>:<select className="model-picker-button" aria-label={tr("选择模型")} value={all.activeId} onChange={e=>{if(e.target.value==='__settings')window.dispatchEvent(new Event('open-model-settings'));else void run(()=>ipc.agentSelectModel(e.target.value));}}><option value="" disabled>{tr("选择模型")}</option>{!adding&&all.profiles.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}<option value="__settings">{tr("配置模型…")}</option></select>)}
 {open&&createPortal(<ModalBackdrop onDismiss={close} disabled={busy||setupPending} dirty={adding?setupDirty:undefined} message={tr("放弃尚未保存的模型配置并关闭？")} style={{zIndex:standalone?2300:1300}}><div className="modal model-manager" role="dialog" aria-label={tr("自定义模型")}>
 <h2>{tr("自定义模型")}</h2><button onClick={()=>window.dispatchEvent(new CustomEvent('open-help',{detail:'models'}))}>{tr("不会配置？查看教程")}</button>
 <p className="modal-hint">{tr("密钥输入一次即可记住；您可以保存多个模型，并随时切换。")}</p>{active&&(active.providerKind==='ollama'||active.baseUrl==='https://openrouter.ai/api/v1'&&(active.model==='openrouter/free'||active.model.endsWith(':free')))&&<p className="model-free-notice">{tr("当前使用免费模型。云端免费服务可能限流或停用；本机模型受电脑性能限制。重要改稿建议准备稳定的备用模型。")}</p>}
 {!adding&&all.profiles.map(p=><div key={p.id} className={'model-choice '+(p.id===all.activeId?'selected':'')}>
 <button disabled={busy} onClick={()=>void run(()=>ipc.agentSelectModel(p.id))}>◇ {p.name}<small>{p.model}</small>{p.id===all.activeId?' ✓':''}</button>
 <button disabled={busy} onClick={()=>void testConnection(p)}>{tr("测试连接")}</button><button disabled={busy} onClick={()=>{setTest(null);setEdit({...p,apiKey:''});}}>{tr("编辑")}</button>
 <button disabled={busy} onClick={()=>{if(window.confirm(tr("删除此模型配置？剧本内容不会改变。")))void run(()=>ipc.agentDeleteModel(p.id));}}>{tr("删除")}</button>
 </div>)}
 {standalone&&<label><input type="checkbox" disabled={busy} checked={cloudAllowed} onChange={e=>{const enabled=e.target.checked;setBusy(true);void ipc.agentSetConfig({cloudAiEnabled:enabled}).then(()=>{setCloudAllowed(enabled);window.dispatchEvent(new Event('models-changed'));}).catch(e=>toast('error',String(e))).finally(()=>setBusy(false));}}/>{tr("允许云模型处理剧本（本机 Ollama 无需开启）")}</label>}
 {!edit&&!adding&&<><button disabled={busy} onClick={()=>{setFreeFirst(false);setAdding(true);}}>{tr("＋ 添加自定义模型")}</button><button disabled={busy} onClick={()=>{setFreeFirst(true);setAdding(true);}}>{tr('先试免费模型')}</button></>}
 {adding&&<ModelSetup freeFirst={freeFirst} onDirtyChange={setSetupDirty} onPendingChange={setSetupPending} busy={busy} onCancel={()=>setAdding(false)} onSave={async (profile,allowCloud)=>{if(await run(()=>ipc.agentSaveModel(profile))){if(allowCloud&&profile.providerKind!=='ollama')try{await ipc.agentSetConfig({cloudAiEnabled:true});window.dispatchEvent(new Event('models-changed'));}catch(error){toast('error',String(error));}setAdding(false);}}}/>}
 {edit&&<form onSubmit={e=>{e.preventDefault();void run(()=>ipc.agentSaveModel(edit));}}>
 <label>{tr("显示名称")}<input disabled={busy} required value={edit.name} onChange={e=>{setTest(null);setEdit({...edit,name:e.target.value});}} placeholder={tr("例如 Agnes 3.0 Flash")}/></label>
 <label>{tr("模型类型")}<select disabled={busy} value={edit.providerKind} onChange={e=>{setTest(null);setEdit({...edit,providerKind:e.target.value});}}><option value="compatible">{tr("兼容 OpenAI 的服务")}</option><option value="openai">OpenAI</option><option value="ollama">{tr("本机 Ollama")}</option></select></label>
 <label>{tr("模型 ID")}<input disabled={busy} required value={edit.model} onChange={e=>{setTest(null);setEdit({...edit,model:e.target.value});}} placeholder="agnes-3.0-flash"/></label>
 {edit.providerKind==='compatible'&&<label>{tr("服务地址")}<input disabled={busy} required value={edit.baseUrl} onChange={e=>{setTest(null);setEdit({...edit,baseUrl:e.target.value});}} placeholder="https://apihub.agnes-ai.com/v1"/></label>}
 {edit.providerKind!=='ollama'&&<label>API Key<input disabled={busy} type="password" autoComplete="off" required={!edit.id} value={edit.apiKey} onChange={e=>{setTest(null);setEdit({...edit,apiKey:e.target.value});}} placeholder={edit.id?tr("已保存，留空保留原密钥"):tr("输入一次，本机加密保存")}/></label>}
 <p className="modal-hint">{tr("密钥仅保存在这台电脑，普通配置和剧本文件中不含密钥。")}</p>
 <p className="modal-hint">{tr("测试只发送一条独立消息，不发送剧本，也不会保存或切换模型。")}</p><button type="button" disabled={busy} onClick={()=>void testConnection(edit)}>{tr("测试连接")}</button>
 <div className="modal-actions"><button type="button" disabled={busy} onClick={()=>setEdit(null)}>{tr("取消")}</button><button disabled={busy} type="submit">{busy?tr("保存中…"):tr("保存并使用")}</button></div>
 </form>}
 {test&&<p role={test.ok?"status":"alert"}>{test.message}</p>}
 <div className="modal-actions"><button disabled={busy||setupPending} onClick={close}>{tr("完成")}</button></div>
 </div></ModalBackdrop>,document.body)}
 </>;
}
