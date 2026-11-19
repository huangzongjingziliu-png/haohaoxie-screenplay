import {fmt, tr} from '@/i18n';
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import { ipc } from '@/ipc/client';
import { useEditorStore } from '@/stores/editor';
import ModelPicker from './ModelPicker';
import SkillCreator from './SkillCreator';
import {flushPendingEdits} from '@/utils/pendingEdits';
import AgentWorkStatus from './AgentWorkStatus';

type ExpertInfo={id:string;name:string;description:string;dimensions:string[];builtin:boolean;enabled:boolean};
type ExpertReply={expertId:string;name:string;ok:boolean;text:string;error?:string|null};
type ExpertPanelResult={reviews:ExpertReply[];synthesis?:string|null;synthesisError?:string|null};

const PANEL_WIDTH_KEY='hhx.agentPanelWidth';
const PANEL_MIN=300,PANEL_MAX=480,PANEL_DEFAULT=340;
const clampPanelWidth=(w:number)=>Math.min(PANEL_MAX,Math.max(PANEL_MIN,w));
const loadPanelWidth=()=>{try{const raw=window.localStorage.getItem(PANEL_WIDTH_KEY);const n=raw?Number.parseInt(raw,10):Number.NaN;return Number.isFinite(n)?clampPanelWidth(n):PANEL_DEFAULT;}catch{return PANEL_DEFAULT;}};

export default function AgentPanel({onClose}:{onClose:()=>void}) {
 const [creatingSkill,setCreatingSkill]=useState(false);
 const [panelWidth,setPanelWidth]=useState(loadPanelWidth);
 useEffect(()=>{try{window.localStorage.setItem(PANEL_WIDTH_KEY,String(panelWidth));}catch{/* 隐私模式下忽略 */}},[panelWidth]);
 const startResize=(event:ReactPointerEvent<HTMLDivElement>)=>{
  if(event.button>0)return;
  event.preventDefault();
  const startX=event.clientX,startWidth=panelWidth;
  const move=(e:PointerEvent)=>setPanelWidth(clampPanelWidth(startWidth-(e.clientX-startX)));
  const stop=()=>{document.removeEventListener('pointermove',move);document.removeEventListener('pointerup',stop);document.body.classList.remove('resizing-agent-panel');};
  document.body.classList.add('resizing-agent-panel');
  document.addEventListener('pointermove',move);
  document.addEventListener('pointerup',stop);
 };
 const doc=useEditorStore(s=>s.document),active=useEditorStore(s=>s.activeBlockId),toast=useEditorStore(s=>s.pushToast);
 const [turns,setTurns]=useState<{role:string;content:string}[]>([]),[text,setText]=useState(''),[busy,setBusy]=useState(false),[skill,setSkill]=useState('');
 const [skills,setSkills]=useState<{id:string;name:string}[]>([]);const bottom=useRef<HTMLDivElement>(null);
 const [experts,setExperts]=useState<ExpertInfo[]>([]),[expertIssues,setExpertIssues]=useState<string[]>([]),[expertForChat,setExpertForChat]=useState('');
 const [showExperts,setShowExperts]=useState(true),[picked,setPicked]=useState<string[]>([]);
 const [reviewMode,setReviewMode]=useState<'review'|'rewrite'>('review'),[scope,setScope]=useState<'scene'|'whole'>('scene');
 const [reviewing,setReviewing]=useState(false),[report,setReport]=useState<ExpertPanelResult|null>(null);
 const [lastRun,setLastRun]=useState<{picked:string[];mode:'review'|'rewrite';scope:'scene'|'whole'}|null>(null);
 const [savingExpert,setSavingExpert]=useState(false),[saveName,setSaveName]=useState('');
 useEffect(()=>{setTurns([]);setText('');setReport(null);},[doc?.id]);
 useEffect(()=>{const reload=()=>{void ipc.agentListSkills().then(setSkills).catch(e=>toast('error',String(e)));};reload();window.addEventListener('skills-changed',reload);return()=>window.removeEventListener('skills-changed',reload);},[]);
 useEffect(()=>{const reloadExperts=()=>{void ipc.agentListExperts().then(r=>{setExperts(r.experts);setExpertIssues(r.issues);}).catch(e=>toast('error',String(e)));};reloadExperts();window.addEventListener('experts-changed',reloadExperts);return()=>window.removeEventListener('experts-changed',reloadExperts);},[]);
 useEffect(()=>{bottom.current?.scrollIntoView({block:'nearest'});},[turns,busy,report]);
 const send=async()=>{if(!doc||busy||!text.trim())return;const request=text.trim(),id=doc.id,history=turns.slice(-12);setText('');setTurns(t=>[...t,{role:'user',content:request}]);setBusy(true);try {await flushPendingEdits();const answer=await ipc.agentChat(id,request,history,skill||undefined,active||undefined,expertForChat||undefined);if(useEditorStore.getState().document?.id===id){setTurns(t=>[...t,{role:'assistant',content:answer}]);window.dispatchEvent(new CustomEvent('project-memory-discussion',{detail:'作者：'+request+'\n助理建议（未确认）：'+answer}));}}catch(e){if(useEditorStore.getState().document?.id===id){setTurns(t=>[...t,{role:'assistant',content:tr("本次未完成：")+String(e)}]);setText(request);}}finally{setBusy(false);}};
 const toggleExpert=(id:string)=>setPicked(list=>list.includes(id)?list.filter(x=>x!==id):list.length>=4?list:[...list,id]);
 const runPanel=async()=>{if(!doc||reviewing||!picked.length)return;if(scope==='scene'&&!active){toast('info',tr("请先在剧本中选择一个段落，再按场景会审。"));return;}const runPicked=[...picked],runMode=reviewMode,runScope=scope;setReviewing(true);setReport(null);setLastRun(null);setSavingExpert(false);try{await flushPendingEdits();const result=await ipc.agentExpertPanel(doc.id,runPicked,runMode,runScope,runScope==='scene'?active||undefined:undefined);if(useEditorStore.getState().document?.id===doc.id){setReport(result);setLastRun({picked:runPicked,mode:runMode,scope:runScope});}}catch(e){toast('error',String(e));}finally{setReviewing(false);}};
 const savePanelExpert=async()=>{if(!lastRun)return;const name=saveName.trim();if(!name){toast('info',tr("请先给这位专家起个名字"));return;}try{await ipc.agentSavePanelExpert(name,lastRun.picked,lastRun.mode,lastRun.scope);void ipc.agentListExperts().then(r=>{setExperts(r.experts);setExpertIssues(r.issues);}).catch(()=>{});setSavingExpert(false);toast('success',tr("已存入专家库，可在 设置 → 编剧助理 中查看"));}catch(e){toast('error',String(e));}};
 const preview=async()=>{await flushPendingEdits();const current=useEditorStore.getState().document;if(!current)return;const block=current.blocks.find(b=>b.id===active);if(!block){toast('info',tr("请先在剧本中选择要修改的段落，或右键发送整个场景。"));return;}window.dispatchEvent(new CustomEvent('agent-selection',{detail:{label:'当前段落',instruction:[...turns].reverse().find(t=>t.role==='user')?.content,text:block.text,fullText:block.text,start:0,end:block.text.length,blockId:block.id,projectId:current.id,revision:current.revision}}));};
 const attach=async()=>{setBusy(true);try{const {open}=await import('@tauri-apps/plugin-dialog');const path=await open({multiple:false,filters:[{name:'剧本附件',extensions:['pdf','png','jpg','jpeg','heic','tiff','docx','txt','md','fountain']}]});if(typeof path!=='string')return;const attachment=await ipc.agentAttachmentText(path);const st=useEditorStore.getState();if(!st.document)return;window.dispatchEvent(new CustomEvent('agent-selection',{detail:{label:path.split('/').pop(),source:path,text:attachment,fullText:st.sourceText,start:0,end:st.sourceText.length,projectId:st.document.id,revision:st.document.revision}}));}catch(e){toast('error',String(e));}finally{setBusy(false);}};
 const availableExperts=experts.filter(e=>e.enabled);
 const pickedExperts=availableExperts.filter(e=>picked.includes(e.id));
 return <aside className="agent-panel harness-panel" style={{width:panelWidth,flexBasis:panelWidth}}>
 <div className="agent-panel-resizer" role="separator" aria-orientation="vertical" aria-label={tr("拖动调整面板宽度")} title={tr("拖动调整宽度（300–480）")} onPointerDown={startResize}/>
 <header><strong>{tr("编剧助理")}</strong><button aria-label={tr("关闭编剧助理")} onClick={onClose}>×</button></header>
 <div className="harness-conversation" aria-live="polite">{!turns.length&&!report&&<div className="harness-welcome"><h3>{tr("一起把故事写好")}</h3><p>{tr("讨论人物、梳理场景，或选中正文后预览改稿。")}</p><p>{tr("需要多人审稿时，用下方的「专家会审」一次请多位专家。")}</p><p>{tr("项目记忆会自动整理；写作偏好请在设置的“个性化”中填写。")}</p></div>}{turns.map((t,i)=><div key={i} className={'chat-turn '+t.role}>{t.content}</div>)}{busy&&<p role="status">{tr("正在阅读上下文并思考…")}</p>}{report&&<section className="expert-report" aria-label={tr("专家会审结果")}><div className="expert-report-head"><strong>{tr("专家会审")}</strong><span className="expert-report-actions">{lastRun&&<button disabled={savingExpert} onClick={()=>{setSavingExpert(true);setSaveName(tr("我的会审专家"));}}>{tr("存成我的专家")}</button>}<button onClick={()=>setReport(null)}>{tr("清空")}</button></span></div>{savingExpert&&<div className="expert-save-form"><input aria-label={tr("专家名称")} value={saveName} maxLength={60} placeholder={tr("给这位专家起个名字")} onChange={e=>setSaveName(e.target.value)} onKeyDown={e=>{if(e.key==='Enter')void savePanelExpert();if(e.key==='Escape')setSavingExpert(false);}}/><button onClick={()=>void savePanelExpert()}>{tr("保存")}</button><button onClick={()=>setSavingExpert(false)}>{tr("取消")}</button></div>}{report.reviews.map(r=><article key={r.expertId} className={'expert-card'+(r.ok?'':' failed')}><h4>{r.name}</h4>{r.ok?<div className="expert-body">{r.text}</div>:<p className="expert-error">{tr("本次未完成：")}{r.error||tr("未知错误")}</p>}</article>)}{report.synthesis&&<article className="expert-card synthesis"><h4>{tr("综合意见")}</h4><div className="expert-body">{report.synthesis}</div></article>}{report.synthesisError&&<p className="expert-error">{tr("汇总未完成：")}{report.synthesisError}</p>}</section>}{reviewing&&<p role="status">{tr("专家正在依次审阅，全部完成后显示结果…")}</p>}<div ref={bottom}/></div>
 <div className="expert-setup">
  <button className="expert-setup-toggle" aria-expanded={showExperts} onClick={()=>setShowExperts(v=>!v)}>{tr("专家会审")}{picked.length?` · ${picked.length}`:''}{showExperts?' ▴':' ▾'}</button>
  {showExperts&&<div className="expert-setup-body">
   <div className="expert-grid">{availableExperts.map(e=><button key={e.id} className={'expert-chip'+(picked.includes(e.id)?' on':'')} aria-pressed={picked.includes(e.id)} title={e.description} disabled={reviewing||(!picked.includes(e.id)&&picked.length>=4)} onClick={()=>toggleExpert(e.id)}>{e.name}</button>)}</div>
   {!!pickedExperts.length&&<p className="expert-dims">{pickedExperts.map(e=>e.name+'：'+e.dimensions.join('、')).join('；')}</p>}
   <div className="expert-options">
    <label>{tr("范围")}<select value={scope} disabled={reviewing} onChange={e=>setScope(e.target.value as 'scene'|'whole')}><option value="scene">{tr("当前场景")}</option><option value="whole">{tr("整稿")}</option></select></label>
    <label>{tr("方式")}<select value={reviewMode} disabled={reviewing} onChange={e=>setReviewMode(e.target.value as 'review'|'rewrite')}><option value="review">{tr("评审")}</option><option value="rewrite">{tr("改写建议")}</option></select></label>
   </div>
   <div className="expert-actions"><button className="expert-run" disabled={reviewing||!picked.length||!doc} onClick={()=>void runPanel()}>{reviewing?tr("会审中…"):tr("开始会审")}</button><small>{picked.length?fmt("{0} 位专家依次执行，再做一次汇总",picked.length):tr("最多四位；整稿会审耗时更长")}</small></div>
   <p className="expert-hint">{tr("专家可在「设置 → 编剧助理」中启用、停用、改名或导入导出自定义 EXPERT.md。")}</p>
   {!!expertIssues.length&&<div className="expert-error"><p>{tr("以下自定义专家未能载入：")}</p>{expertIssues.map(issue=><p key={issue}>{issue}</p>)}</div>}
  </div>}
 </div>
 <div className="harness-composer"><div className="harness-tools"><button disabled={busy} onClick={()=>setCreatingSkill(true)}>{tr("创建技能")}</button><button disabled={busy} onClick={()=>void attach()}>{tr("＋ 附件")}</button><button disabled={busy} onClick={()=>void preview().catch(e=>toast('error',String(e)))}>{tr("预览改稿")}</button><button onClick={()=>window.dispatchEvent(new Event('open-settings'))}>{tr("设置")}</button></div><textarea aria-label={tr("与编剧助理对话")} placeholder={tr("输入想法或要求…（⌘ Enter 发送）")} value={text} disabled={busy} onChange={e=>setText(e.target.value)} onKeyDown={e=>{if((e.metaKey||e.ctrlKey)&&e.key==='Enter'){e.preventDefault();void send();}}}/><div className="harness-send-row"><select aria-label={tr("选择专家")} value={expertForChat} onChange={e=>setExpertForChat(e.target.value)}><option value="">{tr("专家：通用")}</option>{availableExperts.map(e=><option key={e.id} value={e.id}>{e.name}</option>)}</select><select aria-label={tr("选择技能")} value={skill} onChange={e=>setSkill(e.target.value)}><option value="">{tr("通用编剧")}</option>{skills.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}</select><ModelPicker/><button className="harness-send" aria-label={tr("发送")} title={tr("发送 · ⌘ Enter")} disabled={busy||!text.trim()||!doc} onClick={()=>void send()}><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="m3 3 18 9-18 9 4-9-4-9Z M7 12h14"/></svg></button></div></div>
 {creatingSkill&&<SkillCreator onClose={()=>setCreatingSkill(false)}/>}
 {busy&&<AgentWorkStatus label={tr("编剧助理正在处理，请稍候")}/>}</aside>;
}
