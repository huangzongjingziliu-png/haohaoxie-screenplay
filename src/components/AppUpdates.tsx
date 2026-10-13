import {create} from 'zustand';
import {useEffect} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import {open} from '@tauri-apps/plugin-shell';
import {tr} from '@/i18n';
import {useEditorStore} from '@/stores/editor';
import {flushPendingEdits} from '@/utils/pendingEdits';
import {ipc} from '@/ipc/client';
type Available={version:string;notes:string;url:string;automatic:boolean};
type Phase='idle'|'checking'|'downloading'|'ready'|'installing';
const KEY='haohaoxie.updates.v1';
function preference(){try{return JSON.parse(localStorage.getItem(KEY)||'{}') as {auto?:boolean;last?:number};}catch{return {};}}
export const useUpdates=create<{auto:boolean;last:number;available:Available|null;phase:Phase;error:string;message:string;opened:boolean;received:number;total:number|null}>(()=>({auto:preference().auto!==false,last:preference().last||0,available:null,phase:'idle',error:'',message:'',opened:false,received:0,total:null}));
function persist(){const {auto,last}=useUpdates.getState();try{localStorage.setItem(KEY,JSON.stringify({auto,last}));}catch{/* Keep session settings. */}}
export async function checkUpdates(manual=false){
 if(!('__TAURI_INTERNALS__' in window)||useUpdates.getState().phase!=='idle')return;
 useUpdates.setState({phase:'checking',error:'',message:'',...(manual?{opened:true}:{})});
 try{const available=await invoke<Available|null>('check_update');useUpdates.setState({available,last:Date.now(),message:available?'':'目前没有更新的版本'});persist();}
 catch(e){if(manual)useUpdates.setState({error:String(e)});useUpdates.setState({last:Date.now()});persist();}
 finally{useUpdates.setState({phase:'idle'});}
}
export function useUpdateScheduler(){useEffect(()=>{
 const tick=()=>{const s=useUpdates.getState();if(s.auto&&Date.now()-s.last>=24*60*60*1000)void checkUpdates();};
 const startup=window.setTimeout(tick,12000),timer=window.setInterval(tick,60*60*1000);
 let stop:(()=>void)|undefined,disposed=false;
 if('__TAURI_INTERNALS__' in window)void listen<{received:number;total:number|null}>('update-progress',e=>useUpdates.setState(e.payload)).then(fn=>{if(disposed)fn();else stop=fn;}).catch(()=>{});
 return()=>{disposed=true;clearTimeout(startup);clearInterval(timer);stop?.();};
 },[]);}
export function UpdateButton(){const available=useUpdates(s=>s.available);return available?<button className="update-indicator" title={tr('有新版可用，查看更新')} aria-label={tr('查看更新')} onClick={()=>useUpdates.setState({opened:true})}><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5"><path d="M12 20V4m-7 7 7-7 7 7"/></svg></button>:null;}
export function UpdateSettings(){const s=useUpdates();return <><h3>{tr('软件更新')}</h3><label><input type="checkbox" checked={s.auto} onChange={e=>{useUpdates.setState({auto:e.target.checked});persist();if(e.target.checked)void checkUpdates();}}/>{tr('每天自动检查 GitHub 新版本')}</label><p className="modal-hint">{tr('后台检查不会打断写作。下载和重启安装都由你确认，无需配置模型。')}</p><button disabled={s.phase!=='idle'} onClick={()=>void checkUpdates(true)}>{tr('检查更新')}</button>{s.available&&<button onClick={()=>useUpdates.setState({opened:true})}>{tr('查看更新')} · {s.available.version}</button>}</>;}
async function download(){useUpdates.setState({phase:'downloading',error:'',received:0,total:null});try{await invoke('download_update');useUpdates.setState({phase:'ready'});}catch(e){useUpdates.setState({phase:'idle',error:String(e)});}}
async function install(){
 useUpdates.setState({phase:'installing',error:''});
 try{
  await flushPendingEdits();
  const store=useEditorStore.getState(),projectId=store.document?.id;
  if(store.project.state!=='empty'&&(store.project.dirty||store.project.state==='unsaved')){if(!await store.save()){useUpdates.setState({phase:'ready',error:tr('未完成保存，更新尚未安装')});return;}}
  if(useEditorStore.getState().document?.id!==projectId)throw new Error(tr('项目已切换，请重新确认更新'));
  const info=await ipc.projectInfo();
  if(info.dirty||info.state==='unsaved')throw new Error(tr('剧本有新修改，请保存后再更新'));
  await invoke('install_update',{projectId:projectId??null,revision:info.revision});
 }catch(e){useUpdates.setState({phase:'ready',error:String(e)});}
}
export default function UpdateDialog(){const s=useUpdates();if(!s.opened)return null;const busy=s.phase==='checking'||s.phase==='downloading'||s.phase==='installing';const close=()=>{if(!busy)useUpdates.setState({opened:false});};return <div className="modal-backdrop" onClick={e=>{if(e.target===e.currentTarget)close();}}><section className="modal update-dialog" role="dialog" aria-modal="true" aria-label={tr('软件更新')}><h2>{tr('软件更新')}{s.available?` · ${s.available.version}`:''}</h2>{s.available&&<><pre className="update-notes">{s.available.notes||tr('请前往 GitHub 查看此版本的更新内容。')}</pre><button onClick={()=>{const url=s.available!.url;if(url.startsWith('https://github.com/huangzongjingziliu-png/haohaoxie-screenplay/releases/tag/'))void open(url).catch(e=>useUpdates.setState({error:String(e)}));}}>{tr('在 GitHub 查看更新说明')}</button>{!s.available.automatic&&<p>{tr('此版本暂不支持应用内安装，请从 GitHub 下载安装包。')}</p>}</>}{s.message&&<p>{tr(s.message)}</p>}{s.phase==='checking'&&<p>{tr('正在检查更新…')}</p>}{s.phase==='downloading'&&<><p>{tr('正在下载并验证签名…')} {s.total?`${Math.min(100,Math.round(s.received/s.total*100))}%`:`${(s.received/1048576).toFixed(1)} MB`}</p><progress value={s.total?s.received:undefined} max={s.total||1}/></>}{s.phase==='ready'&&<p>{tr('更新包已验证。保存工程后即可重启安装。')}</p>}{s.phase==='installing'&&<p>{tr('正在保存和安装，请稍候…')}</p>}{s.error&&<p className="update-error" role="alert">{s.error}</p>}<div className="modal-actions"><button disabled={busy} onClick={close}>{tr(s.available?'稍后更新':'关闭')}</button>{s.available?.automatic&&s.phase==='idle'&&<button className="primary" onClick={()=>void download()}>{tr('下载更新')}</button>}{s.phase==='ready'&&<button className="primary" onClick={()=>void install()}>{tr('保存并重启更新')}</button>}</div></section></div>;}
