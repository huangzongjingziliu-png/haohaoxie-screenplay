import ModalBackdrop from './ModalBackdrop';
import {tr} from '@/i18n';
import {useState} from 'react';
import type {SceneBoard} from '@/ipc/types/SceneBoard';
export default function BoardNotes({id,title,board,onSave,onClose}:{id:string;title:string;board:SceneBoard;onSave:(comments:string[],expected:string[])=>Promise<unknown>;onClose:()=>void}){
 const [draft,setDraft]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('');const comments=board.cards[id]?.comments??[];
 const save=async(next:string[],adding=false)=>{if(busy)return;setBusy(true);setError('');try{await onSave(next,comments);if(adding)setDraft('');}catch(e){setError(String(e));}finally{setBusy(false);}};
 return <ModalBackdrop className="board-note-backdrop" onDismiss={onClose} disabled={busy} dirty={!!draft.trim()} message={tr("放弃尚未添加的评论并关闭？")}><section className="board-note-dialog" role="dialog" aria-label={tr("场景评论")}><h3>{title}{tr(" · 评论")}</h3>{comments.map((text,i)=><div className="board-comment" key={i}><p>{text}</p><button disabled={busy} onClick={()=>void save(comments.filter((_,n)=>n!==i))}>{tr("删除评论")}</button></div>)}<textarea disabled={busy} aria-label={tr("新增场景评论")} maxLength={10000} value={draft} onChange={e=>setDraft(e.target.value)} placeholder={tr("记录想法、修改意见或拍摄提示…")}/>{error&&<p role="alert">{error}</p>}<button disabled={busy||!draft.trim()} onClick={()=>void save([...comments,draft.trim()],true)}>{tr("添加评论")}</button><button disabled={busy} onClick={onClose}>{tr("完成")}</button></section></ModalBackdrop>;
}
