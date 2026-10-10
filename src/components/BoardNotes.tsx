import {tr} from '@/i18n';
import {useState} from 'react';
import type {SceneBoard} from '@/ipc/types/SceneBoard';
export default function BoardNotes({id,title,board,onSave,onClose}:{id:string;title:string;board:SceneBoard;onSave:(comments:string[])=>void;onClose:()=>void}){
 const [draft,setDraft]=useState('');const comments=board.cards[id]?.comments??[];
 return <div className="board-note-backdrop" onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>{if(e.key==='Escape'){e.stopPropagation();onClose();}}}><section className="board-note-dialog" role="dialog" aria-label={tr("场景评论")}><h3>{title}{tr(" · 评论")}</h3>{comments.map((text,i)=><div className="board-comment" key={i}><p>{text}</p><button onClick={()=>onSave(comments.filter((_,n)=>n!==i))}>{tr("删除评论")}</button></div>)}<textarea aria-label={tr("新增场景评论")} maxLength={10000} value={draft} onChange={e=>setDraft(e.target.value)} placeholder={tr("记录想法、修改意见或拍摄提示…")}/><button disabled={!draft.trim()} onClick={()=>{onSave([...comments,draft.trim()]);setDraft('');}}>{tr("添加评论")}</button><button onClick={onClose}>{tr("完成")}</button></section></div>;
}
