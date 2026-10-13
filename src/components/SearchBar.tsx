import ReplaceDialog from './ReplaceDialog';
import {tr,fmt} from '@/i18n';
import {useEffect,useRef,useState} from 'react';
import {ipc,type SearchHit} from '@/ipc/client';
import {useEditorStore} from '@/stores/editor';
export default function SearchBar({onClose}:{onClose:()=>void}) {
 const [replaceOpen,setReplaceOpen]=useState(false);
 const [query,setQuery]=useState(''),[hits,setHits]=useState<SearchHit[]>([]),[index,setIndex]=useState(-1),[error,setError]=useState('');const input=useRef<HTMLInputElement>(null);const revision=useEditorStore(s=>s.document?.revision);
 useEffect(()=>{input.current?.focus();},[]);
 useEffect(()=>{let live=true;setIndex(-1);setHits([]);setError('');const timer=setTimeout(()=>{if(query.trim())void ipc.search(query).then(h=>{if(live)setHits(h);}).catch(e=>{if(live)setError(String(e));});},150);return()=>{live=false;clearTimeout(timer);};},[query,revision]);
 const jump=(i:number)=>{if(!hits.length)return;const n=(i+hits.length)%hits.length;setIndex(n);const id=hits[n].blockId;const st=useEditorStore.getState();st.setViewMode('left');st.setActiveBlock(id);let tries=0;const seek=()=>{const el=document.querySelector<HTMLElement>(`.screenplay-editor [data-block-id="${id}"] .screenplay-block`);if(el){el.scrollIntoView({block:'center',behavior:'smooth'});el.classList.add('search-hit');setTimeout(()=>el.classList.remove('search-hit'),1800);}else if(tries++<40)requestAnimationFrame(seek);};requestAnimationFrame(seek);};
 return <><div className="screenplay-search" role="search"><input ref={input} aria-label={tr("搜索剧本")} placeholder={tr("搜索剧本中的文字…")} value={query} onChange={e=>setQuery(e.target.value)} onKeyDown={e=>{if(e.key==='Escape')onClose();if(e.key==='Enter'){e.preventDefault();jump(e.shiftKey?(index<0?hits.length-1:index-1):index+1);}}}/><span>{error||(!query?'⌘F':hits.length?fmt("{0} / {1} 个段落",Math.max(index,0)+1,hits.length):tr("没有匹配结果"))}</span><button disabled={!hits.length} onClick={()=>jump(index<0?hits.length-1:index-1)} aria-label={tr("上一个结果")}>↑</button><button disabled={!hits.length} onClick={()=>jump(index+1)} aria-label={tr("下一个结果")}>↓</button><button onClick={()=>setReplaceOpen(true)}>{tr("查找与替换")}</button><button onClick={onClose} aria-label={tr("关闭搜索")}>×</button>{hits.length>0&&<button className="search-preview" onClick={()=>jump(Math.max(index,0))}>{hits[Math.max(index,0)]?.text.slice(0,100)}</button>}</div>{replaceOpen&&<ReplaceDialog initial={query} onClose={()=>setReplaceOpen(false)}/>}</>;
}
