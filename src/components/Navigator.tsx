import {tr,fmt} from '@/i18n';
import {sourceSegments} from '@/utils/sourcePositions';
import { useSceneDrag } from '@/utils/useSceneDrag';
import { moveScene } from '@/utils/scenes';
import { useMemo, useState, useEffect, useRef } from 'react';
import { useEditorStore } from '@/stores/editor';

/** 大纲导航：场景 / 人物 / 地点 / 结构。数据全部来自后端 AST。 */
export default function Navigator() {
  const {startDrag,pointerTarget,insertion} = useSceneDrag('vertical');
  const [dragged, setDragged] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const doc = useEditorStore((s) => s.document);
  const tab = useEditorStore((s) => s.navigatorTab);
  const setTab = useEditorStore((s) => s.setNavigatorTab);
  const setActiveBlock = useEditorStore((s) => s.setActiveBlock);
  const activeBlockId = useEditorStore((s) => s.activeBlockId);

  const [expanded,setExpanded]=useState<string|null>(null);
  const body=useRef<HTMLDivElement>(null);
  const blocks=doc?.blocks??[];
  const activeIndex=blocks.findIndex(b=>b.id===activeBlockId);
  const currentScene=[...blocks.slice(0,activeIndex+1)].reverse().find(b=>b.block_type==='scene_heading');
  const currentLocation=doc?.scenes?.find(s=>s.id===currentScene?.id)?.location;
  useEffect(()=>setExpanded(null),[doc?.id,tab]);
  useEffect(()=>{const row=body.current?.querySelector<HTMLElement>('[data-current="true"]');if(row&&body.current){const a=row.getBoundingClientRect(),b=body.current.getBoundingClientRect();if(a.top<b.top||a.bottom>b.bottom)row.scrollIntoView({block:'nearest'});}},[activeBlockId,tab]);

  const scenes = useMemo(
    () =>
      (doc?.blocks ?? [])
        .filter((b) => b.block_type === 'scene_heading')
        .map((b, i) => ({ id: b.id, label: b.text, no: i + 1 })),
    [doc]
  );

  const characters = useMemo(() => doc?.characters ?? [], [doc]);
  const locations = useMemo(() => doc?.locations ?? [], [doc]);

  const outline = useMemo(
    () =>
      (doc?.blocks ?? []).map((b, i) => ({
        id: b.id,
        index: i,
        type: b.block_type,
        text: b.text,
      })),
    [doc]
  );

  const jump = (id: string) => {
    setActiveBlock(id);
    requestAnimationFrame(() => {
      const state=useEditorStore.getState();
      const source=document.querySelector<HTMLTextAreaElement>('.source-editor textarea');
      if(state.viewMode==='right'&&source){
        const range=sourceSegments(state.sourceText,state.document?.blocks??[]).find(r=>r.ids.includes(id));
        if(range){source.focus();source.setSelectionRange(range.from,range.from);const line=[...document.querySelectorAll<HTMLElement>('[data-source-blocks]')].find(el=>(el.dataset.sourceBlocks??'').split(' ').includes(id));if(line)source.scrollTop+=line.getBoundingClientRect().top-source.getBoundingClientRect().top-source.clientHeight*.3;}
      }else{
        const row=[...document.querySelectorAll<HTMLElement>('.screenplay-editor [data-block-id]')].find(el=>el.dataset.blockId===id);
        row?.scrollIntoView({behavior:'smooth',block:'center'});
        row?.querySelector<HTMLElement>('.screenplay-block')?.focus();
      }
    });
  };
  const occurrenceMaps=useMemo(()=>{
    type Hit={id:string;index:number;sceneId?:string;label:string;no?:number;dialogue:string};
    const characters=new Map<string,Hit[]>(),locations=new Map<string,Hit[]>();
    const locationByScene=new Map((doc?.scenes??[]).map(s=>[s.id,s.location]));
    let scene:typeof blocks[number]|undefined,no=0;
    blocks.forEach((b,index)=>{
      if(b.block_type==='scene_heading'){scene=b;no++;const name=locationByScene.get(b.id);if(name){const list=locations.get(name)??[];list.push({id:b.id,index,sceneId:b.id,label:b.text,no,dialogue:''});locations.set(name,list);}}
      if(b.block_type==='character'&&b.character_id){const list=characters.get(b.character_id)??[];list.push({id:b.id,index,sceneId:scene?.id,label:scene?.text??tr('正文开始'),no:no||undefined,dialogue:blocks.slice(index+1,index+3).filter(b=>b.block_type==='dialogue'||b.block_type==='parenthetical').map(b=>b.text).join(' ')});characters.set(b.character_id,list);}
    });
    return {characters,locations};
  },[doc]);
  const entityRow=(kind:'characters'|'locations',id:string,name:string)=>{
    const hits=occurrenceMaps[kind].get(kind==='characters'?id:name)??[],key=kind+id;
    const highlighted=kind==='locations'?currentLocation===name:blocks[activeIndex]?.character_id===id;
    const nearest=[...hits].sort((a,b)=>Math.abs(a.index-Math.max(activeIndex,0))-Math.abs(b.index-Math.max(activeIndex,0)))[0];
    return <li key={id} className={`navigator-entity ${highlighted?'active':''}`} data-current={highlighted?'true':undefined}>
      <div className="navigator-entity-heading"><button className="navigator-entity-name" disabled={!nearest} onClick={()=>{if(nearest)jump(nearest.id);}} title={tr('跳转到最近出现的位置')}><span>{name}</span><small>{fmt('{0} 处',hits.length)}</small></button>{hits.length>1&&<button className="navigator-occurrence-toggle" aria-label={fmt('查看{0}的出现位置',name)} aria-expanded={expanded===key} onClick={()=>setExpanded(expanded===key?null:key)}>{expanded===key?'⌃':'⌄'}</button>}</div>
      {expanded===key&&<div className="navigator-occurrences">{hits.map((hit,i)=><button key={hit.id} className={activeBlockId===hit.id?'active':''} onClick={()=>jump(hit.id)} title={hit.dialogue||hit.label}><strong>{hit.no?fmt('第{0}场',hit.no):tr('正文开始')} · {i+1}/{hits.length}</strong><span>{hit.label}</span>{hit.dialogue&&<small>{hit.dialogue.slice(0,60)}</small>}</button>)}</div>}
    </li>;
  };

  return (
    <div className="navigator">
      <div className="navigator-tabs">
        {(['scenes', 'characters', 'locations', 'outline'] as const).map((t) => (
          <button key={t} className={tab === t ? 'active' : ''} onClick={() => setTab(t)}>
            {tr({ scenes: '场景', characters: '人物', locations: '地点', outline: '大纲' }[t])}
          </button>
        ))}
      </div>

      <div className="navigator-body" ref={body}>
        {tab === 'scenes' && (
          <ul>
            {scenes.length === 0 && <li className="empty">{tr("暂无场景")}</li>}
            {scenes.map((s) => (
              <li
                key={s.id}
                draggable
                data-scene-id={s.id}
                tabIndex={0}
                onPointerDown={e => startDrag(e,s.id)}
                title={tr("拖动以调整场景顺序")}
                onDragStart={e => { setDragged(s.id); e.dataTransfer.setData('application/x-muben-scene', s.id); e.dataTransfer.effectAllowed = 'move'; }}
                onDragOver={e => { if (dragged) { e.preventDefault(); setDropTarget(s.id); } }}
                onDragEnd={() => { setDragged(null); setDropTarget(null); }}
                onDrop={e => { e.preventDefault(); const id = e.dataTransfer.getData('application/x-muben-scene') || dragged; setDragged(null); setDropTarget(null); if (id) void moveScene(id,s.id).catch(error => useEditorStore.getState().pushToast('error', String(error))); }}
                data-insertion={insertion?.id === s.id ? insertion.side : undefined}
                data-drop-target={dropTarget === s.id || pointerTarget === s.id}
                data-current={currentScene?.id===s.id?'true':undefined}
                className={currentScene?.id === s.id ? 'active' : ''}
                onKeyDown={e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();jump(s.id);}}}
                onClick={() => jump(s.id)}
              >
                <span className="num">{s.no}</span>
                <span className="label">{s.label}</span>
              </li>
            ))}
          </ul>
        )}

        {tab === 'characters' && (
          <ul>
            {characters.length === 0 && <li className="empty">{tr("暂无人物")}</li>}
            {characters.map(c=>entityRow('characters',c.id,c.name))}
          </ul>
        )}

        {tab === 'locations' && (
          <ul>
            {locations.length === 0 && <li className="empty">{tr("暂无地点")}</li>}
            {locations.map(l=>entityRow('locations',l.id,l.name))}
          </ul>
        )}

        {tab === 'outline' && (
          <ul className="outline">
            {outline.map((o) => (
              <li
                key={o.id}
                data-current={activeBlockId===o.id?'true':undefined}
                tabIndex={0}
                onKeyDown={e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();jump(o.id);}}}
                className={`${activeBlockId === o.id ? 'active' : ''} type-${o.type}`}
                onClick={() => jump(o.id)}
              >
                <span className="idx">{o.index + 1}</span>
                <span className="type">{o.type}</span>
                <span className="text">{o.text.slice(0, 40)}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}