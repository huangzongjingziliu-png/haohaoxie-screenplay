import ModalBackdrop from './ModalBackdrop';
import {useState} from 'react';
import {createPortal} from 'react-dom';
import {tr} from '@/i18n';
import {ipc} from '@/ipc/client';
import ModelPicker from './ModelPicker';
export default function SkillCreator({onClose}:{onClose:()=>void}) {
 const [prompt,setPrompt]=useState(''),[draft,setDraft]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const generate=async()=>{setBusy(true);setError('');try{setDraft(await ipc.agentGenerateSkill(prompt));}catch(e){setError(String(e));}finally{setBusy(false);}};
 const save=async()=>{setBusy(true);setError('');try{await ipc.agentCreateSkill(draft);window.dispatchEvent(new Event('skills-changed'));onClose();}catch(e){setError(String(e));}finally{setBusy(false);}};
 return createPortal(<ModalBackdrop onDismiss={onClose} disabled={busy} dirty={!!prompt.trim()||!!draft.trim()} message={tr("放弃技能需求和未安装的技能草稿？")} style={{zIndex:1500}}><section className="modal skill-creator" role="dialog" aria-label={tr("创建技能")}><h2>{tr("创建技能")}</h2><p>{tr("描述要反复使用的写作方法，让模型起草，再检查、编辑并安装。只生成技能，不修改剧本。")}</p><textarea aria-label={tr("技能需求")} placeholder={tr("例如：检查对白是否符合人物性格，保留原作语气，逐条给出修改建议。") } value={prompt} disabled={busy} maxLength={10000} onChange={e=>setPrompt(e.target.value)}/><div className="skill-create-tools"><ModelPicker/><button disabled={busy||!prompt.trim()} onClick={()=>void generate()}>{busy?tr("处理中…"):tr("生成技能草稿")}</button></div>{busy&&<p role="status">{tr("正在处理技能，请稍候…")}</p>}{draft&&<><label>{tr("技能草稿 · 可编辑")}<textarea className="skill-draft" aria-label="SKILL.md" value={draft} disabled={busy} onChange={e=>setDraft(e.target.value)}/></label><p className="modal-hint">{tr("保存到本机技能库；id需唯一，不能覆盖已有技能。")}</p></>}{error&&<p role="alert">{error}</p>}<div className="modal-actions"><button disabled={busy} onClick={onClose}>{tr("取消")}</button><button disabled={busy||!draft.trim()} onClick={()=>void save()}>{tr("保存并安装技能")}</button></div></section></ModalBackdrop>,document.body);
}
