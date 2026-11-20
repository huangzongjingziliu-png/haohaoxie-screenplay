import {UpdateSettings} from './AppUpdates';
import ProjectMemoryOverview from './ProjectMemoryOverview';
import ModalBackdrop from './ModalBackdrop';
import {tr,useUiLanguage,setUiLanguage,type UiLanguage} from '@/i18n';
import {useEffect,useState,useRef} from 'react';
import {TypographySettings} from './SettingsPanel';
import ModelPicker from './ModelPicker';
import SkillSettings from './SkillSettings';
import {ipc} from '@/ipc/client';
import {useEditorStore} from '@/stores/editor';
import {readTheme,saveTheme,type ThemeMode} from '@/utils/theme';
export default function UnifiedSettings({onClose,initial='text'}:{onClose:()=>void;initial?:string}) {
 const saveTypography=useRef<(()=>Promise<boolean>)|null>(null);
 const closing=useRef(false);
 const [creatingSkill,setCreatingSkill]=useState(false);
 const loadedMemory=useRef<string|null>(null);
 const uiLanguage=useUiLanguage();
 const [tab,setTab]=useState(initial),[memory,setMemory]=useState(''),[busy,setBusy]=useState(false),[cloud,setCloud]=useState(false);
 const [appearance,setAppearance]=useState<ThemeMode>(readTheme);
 const doc=useEditorStore(s=>s.document),toast=useEditorStore(s=>s.pushToast);const id=doc?.id??'';
 useEffect(()=>{void ipc.agentGetConfig().then(c=>setCloud(c.cloudAiEnabled)).catch(e=>toast('error',String(e)));},[]);
 useEffect(()=>{let live=true;setMemory('');loadedMemory.current=null;if(tab==='system'||(tab==='project'&&id)){setBusy(true);void ipc.agentMemory(tab,id).then(t=>{if(live){setMemory(t);loadedMemory.current=t;}}).catch(e=>toast('error',String(e))).finally(()=>{if(live)setBusy(false);});}return()=>{live=false;};},[tab,id]);
 const save=async()=>{setBusy(true);try{await ipc.agentSaveMemory(tab,id,memory);loadedMemory.current=memory;toast('success',tr("记忆已保存"));}catch(e){toast('error',String(e));}finally{setBusy(false);}};
 const saveCurrent=async()=>{
  if(tab==='text')return await saveTypography.current?.()??true;
  if((tab==='system'||(tab==='project'&&id))&&loadedMemory.current!==null&&memory!==loadedMemory.current){await ipc.agentSaveMemory(tab,id,memory);loadedMemory.current=memory;}
  return true;
 };
 const leave=async(next?:string)=>{if(busy||closing.current||creatingSkill)return;closing.current=true;setBusy(true);try{if(await saveCurrent()){if(next)setTab(next);else onClose();}}catch(e){toast('error',String(e));}finally{closing.current=false;setBusy(false);}};
 return <ModalBackdrop className="settings-shell modal-backdrop" onDismiss={edited=>{if(edited)void leave();else onClose();}} disabled={busy||creatingSkill} dirty={tab==='text'?undefined:(tab==='system'||tab==='project')&&loadedMemory.current!==null&&memory!==loadedMemory.current} message={tr("保存当前设置或记忆的修改并关闭？")}><section className="modal unified-settings" role="dialog" aria-label={tr("设置")}><h2>{tr("设置")}</h2><div className="settings-layout"><nav className="settings-tabs" aria-label={tr("设置分类")}>{[['general',tr("语言")],['appearance',tr("外观")],['text',tr("文字与排版")],['updates',tr("软件更新")],['models',tr("模型与技能")],['project',tr("项目记忆")],['system',tr("个性化")]].map(([key,label])=><button key={key} className={tab===key?'selected':''} aria-current={tab===key?'page':undefined} disabled={busy} onClick={()=>void leave(key)}>{tr(label)}</button>)}</nav><div className="settings-detail">{tab==='text'?<TypographySettings embedded onClose={onClose} registerSave={save=>{saveTypography.current=save;}}/>:<div className="settings-content">{tab==='appearance'?<><h3>{tr("显示模式")}</h3><p className="modal-hint">{tr("选择应用界面的外观。跟随系统会随 macOS 外观即时切换；导出的 PDF、Word、HTML 始终使用浅色纸面。")}</p><div className="theme-choices" role="radiogroup" aria-label={tr("显示模式")}>{([['light',tr("浅色")],['dark',tr("深色")],['system',tr("跟随系统")]] as [ThemeMode,string][]).map(([mode,label])=><button key={mode} role="radio" aria-checked={appearance===mode} className={appearance===mode?'selected':''} onClick={()=>{setAppearance(mode);saveTheme(mode);}}>{label}</button>)}</div></>:tab==='general'?<><h3>{tr("语言")}</h3><label>{tr("界面语言")}<select aria-label={tr("界面语言")} value={uiLanguage} onChange={e=>setUiLanguage(e.target.value as UiLanguage)}><option value="system">{tr("跟随系统")}</option><option value="zh">简体中文</option><option value="en">English</option></select></label><p className="modal-hint">{tr("只改变软件界面，不会翻译正文或改变剧本排版。")}</p></>:tab==='updates'?<UpdateSettings/>:tab==='models'?<><h3>{tr("自定义模型")}</h3><ModelPicker manage/><label><input type="checkbox" checked={cloud} onChange={e=>{const enabled=e.target.checked;void ipc.agentSetConfig({cloudAiEnabled:enabled}).then(()=>{setCloud(enabled);window.dispatchEvent(new Event('models-changed'));}).catch(e=>toast('error',String(e)));}}/>{tr(" 允许云模型处理剧本（本机 Ollama 无需开启）")}</label><SkillSettings creating={creatingSkill} onCreating={setCreatingSkill}/></>:<><h3>{tab==='project'?tr("项目记忆 · ")+(doc?.metadata.title??tr("未打开项目")):tr("个性化 · 写作偏好")}</h3><p className="modal-hint">{tr("Markdown 格式，本机保存，可自由编辑。")}{tab==='project'?tr("项目记忆随工程保存，自动整理内容与作者确认笔记分开维护。"):tr("由你自行填写，对所有项目生效。模型不会自动修改，不随工程分享。")}</p>{tab==='project'&&<ProjectMemoryOverview onPrepare={saveCurrent}/>}<textarea className="memory-editor" aria-label={tr("Markdown记忆")} value={memory} disabled={busy||(tab==='project'&&!id)} onChange={e=>setMemory(e.target.value)} placeholder={tab==='project'&&!id?tr("请先打开项目"):tr("尚无记忆，可在此编写…")}/><button disabled={busy||(tab==='project'&&!id)} onClick={()=>void save()}>{tr(tab==='project'?"保存作者笔记":"保存个性化")}</button></>}<div className="modal-actions"><button disabled={busy} onClick={()=>void leave()}>{tr("完成")}</button></div></div>}</div></div></section></ModalBackdrop>;
}
