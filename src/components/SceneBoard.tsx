import ModalBackdrop from './ModalBackdrop';
import {tr,fmt} from '@/i18n';
import InlineSceneText from './InlineSceneText';
import {flushPendingEdits} from '@/utils/pendingEdits';
import AgentWorkStatus from './AgentWorkStatus';
import BoardConnections from '@/components/BoardConnections';
import BoardNotes from '@/components/BoardNotes';
import BoardColorPicker from '@/components/BoardColorPicker';
import { useBoardZoom } from '@/utils/useBoardZoom';
import { useSceneDrag } from '@/utils/useSceneDrag';
import { useEffect, useRef, useState } from 'react';
import { useEditorStore } from '@/stores/editor';
import { ipc } from '@/ipc/client';
import { defaultBoard, defaultCard, scenesOf, moveScene, moveBoardUnit, updateBoard, useSceneHeadingDisplays } from '@/utils/scenes';
import type { SceneCard } from '@/ipc/types/SceneCard';
import type { BoardLane } from '@/ipc/types/BoardLane';
import type { SceneGroup } from '@/ipc/types/SceneGroup';
import type { TemplateSummary } from '@/ipc/client';

export default function SceneBoard({onClose, onConfigure, onJump}: {onClose:()=>void; onConfigure:()=>void; onJump?:(id:string)=>void}) {
  const {canvasRef,zoom,changeZoom}=useBoardZoom();
  const colorSequence=useRef(0);
  const closingBoard=useRef(false);
  const [draftColor,setDraftColor]=useState<string | null>(null);
  const {startDrag,pointerTarget,insertion} = useSceneDrag('horizontal',(from,to,side)=>{const current=useEditorStore.getState().document;return current?(current.settings.scene_board.groups.length?moveBoardUnit(current.id,from,to,side):moveScene(from,to,side)):Promise.resolve();});
  const cardsRef=useRef<HTMLDivElement>(null);
  const [checked,setChecked]=useState<string[]>([]);
  const [groupName,setGroupName]=useState('');
  const [adding,setAdding]=useState(false),[addingLane,setAddingLane]=useState(''),[newTitle,setNewTitle]=useState(''),[newSummary,setNewSummary]=useState(''),[creating,setCreating]=useState(false),[createError,setCreateError]=useState('');

  const [selectionBox,setSelectionBox]=useState<{left:number;top:number;width:number;height:number}|null>(null);
  const stopSelection=useRef<(()=>void)|null>(null);
  useEffect(()=>()=>stopSelection.current?.(),[]);
  const [commentId,setCommentId]=useState<string|null>(null);
  const [linkFrom,setLinkFrom]=useState<string|null>(null);
  const [selectedNode,setSelectedNode]=useState<string|null>(null);
  const [editingLink,setEditingLink]=useState<string|null>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const doc = useEditorStore(s=>s.document);
  const [selected, setSelected] = useState<string | null>(null);
  const [draggedUnit, setDraggedUnit] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [dropLane, setDropLane] = useState<string | null>(null);
  const [templates, setTemplates] = useState<TemplateSummary[]>([]);
  const [templateId, setTemplateId] = useState('');
  const [classifying, setClassifying] = useState(false);
  const [laneMessage, setLaneMessage] = useState('');
  const [preview, setPreview] = useState<{id:string;width:number;height:number} | null>(null);
  const [generating, setGenerating] = useState('');
  const [stoppingSummary,setStoppingSummary]=useState(false);
  const [error, setError] = useState('');
  const [tick, setTick] = useState(0);
  const [configured, setConfigured] = useState<boolean | null>(null);
  const paused = useRef(false);
  const running = useRef(false);
  const failed = useRef(new Set<string>());
  const live = useRef(true);
  const stopResize = useRef<(()=>void) | null>(null);
  useEffect(()=> { live.current = true; void ipc.agentGetConfig().then(c=>setConfigured(c.providerKind !== 'echo' && !!c.model && (c.providerKind !== 'compatible' || !!c.baseUrl))).catch(()=>setConfigured(false)); return ()=> {live.current=false;stopResize.current?.();}; }, []);
  useEffect(()=> { const escape=(e:KeyboardEvent)=> {if(e.key==='Escape'&&!adding) void closeBoard();};window.addEventListener('keydown',escape);return ()=>window.removeEventListener('keydown',escape); }, [onClose,adding]);
  useEffect(()=> {
    const previous = document.activeElement as HTMLElement | null;
    dialogRef.current?.focus();
    const trap = (e:KeyboardEvent)=> {
      if (e.key !== 'Tab') return;
      const items = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),textarea:not(:disabled),[tabindex="0"]') ?? []);
      const first=items[0],last=items[items.length-1];
      if (e.shiftKey && (document.activeElement===first || document.activeElement===dialogRef.current)) {e.preventDefault();last?.focus();}
      else if (!e.shiftKey && document.activeElement===last) {e.preventDefault();first?.focus();}
    };
    document.addEventListener('keydown',trap);
    return ()=>{document.removeEventListener('keydown',trap);if(previous?.isConnected)previous.focus();};
  }, []);
  const scenes = scenesOf(doc), board = doc?.settings.scene_board ?? defaultBoard();
  const headingDisplays = useSceneHeadingDisplays();
  const shownColor=draftColor ?? board.color;
  const changeDefaultColor=(color:string)=> {const sequence=++colorSequence.current;setDraftColor(color);if(doc)void updateBoard(doc.id,b=>({...b,color})).then(()=>{if(sequence===colorSequence.current)setDraftColor(null);}).catch(e=>{if(sequence===colorSequence.current)setDraftColor(null);report(e);});};
  const enabled = board.summaries_enabled;
  const report = (e: unknown) => { if (live.current) setError(String(e)); };
  const closeBoard=async()=>{if(closingBoard.current)return;closingBoard.current=true;try{await flushPendingEdits();onClose();}catch(e){report(e);}finally{closingBoard.current=false;}};
  const edit = (change: Parameters<typeof updateBoard>[1]) => { if (doc) void updateBoard(doc.id, change).catch(report); };
  const cardEdit = (id:string, patch:Partial<SceneCard>) => edit(b=>({...b,cards:{...b.cards,[id]:{...(b.cards[id] ?? defaultCard()),...patch}}}));
  const lanes = board.lanes ?? [];

  useEffect(()=> {
    let disposed = false;
    void ipc.listProjectTemplates().then(payload => { if (!disposed) setTemplates(payload?.templates ?? []); }).catch(() => {});
    return () => { disposed = true; };
  }, []);

  const laneEdit = (id:string, patch:Partial<BoardLane>) => edit(b=>({...b,lanes:b.lanes.map(l=>l.id===id?{...l,...patch}:l)}));
  const addLane = () => { setLaneMessage(''); edit(b=>({...b,lanes:[...b.lanes,{id:crypto.randomUUID(),name:fmt("新分区 {0}",b.lanes.length+1),hint:'',color:'',collapsed:false}]})); };
  const moveLane = (id:string, step:number) => edit(b=> {
    const from = b.lanes.findIndex(l=>l.id===id), to = from+step;
    if (from < 0 || to < 0 || to >= b.lanes.length) return b;
    const next = [...b.lanes];[next[from],next[to]]=[next[to],next[from]];
    return {...b,lanes:next};
  });
  const removeLane = (id:string) => { setLaneMessage(tr("已删除分区，原本的便签留在“未分区”。")); edit(b=>({...b,
    lanes:b.lanes.filter(l=>l.id!==id),
    cards:Object.fromEntries(Object.entries(b.cards).map(([cardId,card])=>[cardId, card?.lane===id ? {...card,lane:''} : card])),
  })); };
  const setLaneForScenes = (ids:string[], lane:string, detach:boolean) => { if (!ids.length) return; edit(b=> {
    const cards = {...b.cards};
    for (const sceneId of ids) cards[sceneId] = {...(cards[sceneId] ?? defaultCard()), lane};
    const groups = detach ? b.groups.map(g=>g.scene_ids.some(s=>ids.includes(s)) ? {...g,scene_ids:g.scene_ids.filter(s=>!ids.includes(s))} : g).filter(g=>g.scene_ids.length>0) : b.groups;
    return {...b,cards,groups};
  }); };
  const groupLane = (group:SceneGroup) => { const first = scenes.find(s=>group.scene_ids.includes(s.id)); return first ? board.cards[first.id]?.lane ?? '' : ''; };
  const dropInto = (lane:string) => {
    const unit = draggedUnit; setDraggedUnit(null); setDropLane(null); setDropTarget(null);
    if (!unit) return;
    if (unit.startsWith('lane:')) {
      const from = unit.slice(5);
      if (!lane || from===lane) return;
      edit(b=> {
        const next = [...b.lanes], fromIndex = next.findIndex(l=>l.id===from), toIndex = next.findIndex(l=>l.id===lane);
        if (fromIndex < 0 || toIndex < 0) return b;
        const [moved] = next.splice(fromIndex,1); next.splice(toIndex,0,moved);
        return {...b,lanes:next};
      });
      return;
    }
    if (unit.startsWith('group:')) { const group = board.groups.find(g=>g.id===unit.slice(6)); if (group && groupLane(group)!==lane) setLaneForScenes(group.scene_ids,lane,false); return; }
    const sceneId = unit.slice(6); if ((board.cards[sceneId]?.lane ?? '')!==lane) setLaneForScenes([sceneId],lane,true);
  };
  const applyTemplate = async () => {
    if (!doc || !templateId) return;
    const template = templates.find(t=>t.id===templateId);
    if (!template) return;
    setLaneMessage('');
    try {
      const next = template.lanes.map(lane=>({id:crypto.randomUUID(),name:lane.name,hint:lane.hint,color:lane.color,collapsed:false}));
      await updateBoard(doc.id,b=>({...b,lanes:next,cards:Object.fromEntries(Object.entries(b.cards).map(([cardId,card])=>[cardId, card ? {...card,lane:''} : card]))}));
      setLaneMessage(fmt("已套用「{0}」{1} 个分区；原有便签保留在“未分区”，可撤销。",template.name,next.length));
    } catch(e) { setLaneMessage(String(e)); }
  };
  const classifyLanes = async () => {
    if (!doc || classifying || !lanes.length) return;
    setClassifying(true); setLaneMessage('');
    try {
      const result = await ipc.classifyBoard(lanes.map(l=>({id:l.id,name:l.name,hint:l.hint})));
      if (useEditorStore.getState().document?.id !== doc.id) return;
      const assignments = new Map(result.assignments.map(a=>[a.sceneId,a.laneId]));
      await updateBoard(doc.id,b=> {
        const cards = {...b.cards};
        for (const [sceneId,laneId] of assignments) {
          if (!b.lanes.some(l=>l.id===laneId)) continue;
          cards[sceneId] = {...(cards[sceneId] ?? defaultCard()), lane:laneId};
        }
        return {...b,cards};
      });
      setLaneMessage(fmt("模型已通读剧本，把 {0} 个场景划分到分区；可以手动拖动调整或撤销。",assignments.size));
    } catch(e) { setLaneMessage(String(e)); }
    finally { setClassifying(false); }
  };

  useEffect(()=> {
    if (!doc || !enabled || running.current || paused.current || configured !== true) return;
    const scene = scenes.find(s=>!board.cards[s.id]?.summary_manual && board.cards[s.id]?.summary_source !== s.source && !failed.current.has(s.id+'\n'+s.source));
    if (!scene) return;
    const projectId = doc.id;
    const timer = window.setTimeout(()=> {
      running.current = true; setGenerating(headingDisplays[scene.id]??scene.title); setError('');
      void ipc.summarizeScene(scene.id).then(async result => {
        const current = useEditorStore.getState().document;
        if (!live.current || current?.id !== projectId || !current.settings.scene_board?.summaries_enabled || result.projectId !== projectId) return;
        await updateBoard(projectId, b=> {
          const fresh = scenesOf(useEditorStore.getState().document).find(s=>s.id===scene.id);
          if (!fresh || fresh.source !== result.source || b.cards[scene.id]?.summary_manual) return b;
          return {...b,cards:{...b.cards,[scene.id]:{...(b.cards[scene.id] ?? defaultCard()), summary:result.summary,summary_source:result.source}}};
        });
      }).catch(e=> { paused.current=true; failed.current.add(scene.id+'\n'+scene.source); report(e); }).finally(()=> { running.current=false; if (live.current) {setGenerating('');setStoppingSummary(false);setTick(t=>t+1);} });
    }, 700);
    return ()=>window.clearTimeout(timer);
  }, [doc, enabled, configured, tick]);
  if (!doc) return null;
  const createScene=async()=>{
    if(creating||!newTitle.trim())return;
    setCreating(true);setCreateError('');
    try {
      await flushPendingEdits();
      const state=useEditorStore.getState(),current=state.document;
      if(current?.id!==doc.id)throw new Error(tr("项目已切换"));
      const result=await ipc.createBoardScene(newTitle.trim(),newSummary,current.revision,addingLane||null);
      if(useEditorStore.getState().document?.id!==doc.id)throw new Error(tr("项目已切换"));
      await state.applyResult(result);
      const id=result.affected[0];setSelected(id);setChecked([id]);setAdding(false);setAddingLane('');setNewTitle('');setNewSummary('');
      requestAnimationFrame(()=>cardsRef.current?.querySelector(`[data-scene-id="${id}"]`)?.scrollIntoView({block:'nearest',inline:'nearest'}));
    }catch(e){setCreateError(String(e));}finally{setCreating(false);}
  };
  const selectedCard = selected ? board.cards[selected] ?? defaultCard() : null;
  const resize = (e: React.PointerEvent, id:string, width:number, height:number) => {
    e.preventDefault(); e.stopPropagation(); setSelected(id); stopResize.current?.();
    const startX=e.clientX, startY=e.clientY;
    let nextWidth=width,nextHeight=height;
    const move=(event:PointerEvent)=> { nextWidth=Math.round(Math.max(160,Math.min(640,width+(event.clientX-startX)/zoom)));nextHeight=Math.round(Math.max(160,Math.min(640,height+(event.clientY-startY)/zoom)));setPreview({id,width:nextWidth,height:nextHeight}); };
    const cleanup=()=> { window.removeEventListener('pointermove',move);window.removeEventListener('pointerup',up);window.removeEventListener('pointercancel',cancel);stopResize.current=null; };
    const up=()=> {cleanup();setPreview(null);cardEdit(id,{width:nextWidth,height:nextHeight});};
    const cancel=()=> {cleanup();setPreview(null);};
    stopResize.current=cleanup;window.addEventListener('pointermove',move);window.addEventListener('pointerup',up);window.addEventListener('pointercancel',cancel);
  };
  const reorder=(from:string,to:string)=>void moveScene(from,to).catch(report);
  const selectNode=(id:string)=>{setSelectedNode(id);if(linkFrom&&linkFrom!==id){edit(b=>({...b,links:[...b.links,{id:crypto.randomUUID(),from:linkFrom,to:id,label:'',arrow:'end'}]}));setLinkFrom(null);}};
  const createGroup=()=>{const ids=scenes.filter(s=>checked.includes(s.id)).map(s=>s.id);if(ids.length<2)return;
    const name=groupName.trim() || fmt("场景组 {0}",board.groups.length+1),id=crypto.randomUUID();void updateBoard(doc.id,b=>({...b,groups:[...b.groups.map(g=>({...g,scene_ids:g.scene_ids.filter(s=>!ids.includes(s))})).filter(g=>g.scene_ids.length),{id,name,scene_ids:ids,collapsed:false}]}),(order)=>{const at=order.indexOf(ids[0]),rest=order.filter(s=>!ids.includes(s));rest.splice(at,0,...ids);return rest;}).catch(report);setChecked([]);setGroupName('');};
  const groupByScene=new Map<string,string>();(board.groups??[]).forEach(g=>g.scene_ids.forEach(id=>groupByScene.set(id,g.id)));
  const units:{kind:'scene'|'group';id:string}[]=[];const emitted=new Set<string>();scenes.forEach(scene=>{const group=groupByScene.get(scene.id);if(group){if(!emitted.has(group)){units.push({kind:'group',id:group});emitted.add(group);}}else units.push({kind:'scene',id:scene.id});});
  const renderCard=(s:typeof scenes[number],i:number)=> { const c=board.cards[s.id] ?? defaultCard(), size=preview?.id===s.id ? preview : {width:c.width||240,height:c.height||240}; return <article onDoubleClick={e=>{if(!(e.target as HTMLElement).closest("header,button,textarea,input"))e.currentTarget.querySelector(".scene-summary-inline")?.dispatchEvent(new MouseEvent("dblclick",{bubbles:true}));}} key={s.id} data-scene-id={s.id} data-board-node={s.id} data-insertion={insertion?.id===s.id?insertion.side:undefined} onPointerDown={e=>startDrag(e,s.id)} draggable onDragStart={e=>{setDraggedUnit('scene:'+s.id);e.dataTransfer.setData('application/x-muben-scene',s.id);e.dataTransfer.effectAllowed='move';}} onDragEnd={()=>{setDraggedUnit(null);setDropTarget(null);setDropLane(null);}} className={`scene-card ${checked.includes(s.id)||selected===s.id?'selected':''} ${(dropTarget===s.id || pointerTarget===s.id)?'drop-target':''}`} style={{background:c.color||shownColor,width:size.width,height:size.height}} tabIndex={0} aria-label={fmt("场景 {0}：{1}",i+1,headingDisplays[s.id]??s.title)} onClick={e=>{setSelected(s.id);selectNode(s.id);setChecked(ids=>e.metaKey||e.ctrlKey ? ids.includes(s.id)?ids.filter(id=>id!==s.id):[...ids,s.id] : [s.id]);}} onFocus={()=>setSelected(s.id)} onKeyDown={e=> {if(e.altKey && ['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(e.key)){e.preventDefault();const next=scenes[i+(['ArrowLeft','ArrowUp'].includes(e.key)?-1:1)];if(next)reorder(s.id,next.id);}}} onDragOver={e=>{if(draggedUnit?.startsWith('scene:')){e.preventDefault();e.stopPropagation();setDropTarget(s.id);setDropLane(null);}}} onDrop={e=>{if(!draggedUnit?.startsWith('scene:'))return;e.preventDefault();e.stopPropagation();const from=e.dataTransfer.getData('application/x-muben-scene')||draggedUnit.slice(6);setDraggedUnit(null);setDropTarget(null);if(!from)return;const targetLane=laneOfScene(s.id);if(targetLane!==laneOfScene(from))setLaneForScenes([from],targetLane,true);else reorder(from,s.id);}}>
        <header draggable title={tr("拖动场景标题以调整顺序；Option + 方向键也可排序")} onDragStart={e=>{setDraggedUnit('scene:'+s.id);e.dataTransfer.setData('application/x-muben-scene',s.id);e.dataTransfer.effectAllowed='move';}} onDragEnd={()=>{setDraggedUnit(null);setDropTarget(null);setDropLane(null);}}><span className="scene-card-number">{i+1}</span><InlineSceneText title value={headingDisplays[s.id]??s.title} baseline={s.title} label={fmt("编辑场景 {0} 标题",i+1)} onSave={async(text,original)=>{const state=useEditorStore.getState(),current=state.document;if(current?.id!==doc.id)throw new Error(tr("项目已切换"));const block=current.blocks.find(b=>b.id===s.id);if(!block)throw new Error(tr("场景已删除"));if(block.text!==original)throw new Error(tr("场景标题已被修改，请重新打开编辑"));await state.applyResult(await ipc.replaceBlock(s.id,text,current.revision));}}/></header>

        <button className="card-jump" title={tr("跳转到此场景的标准剧本")} aria-label={fmt("跳转到场景 {0} 的标准剧本",i+1)} onClick={e=>{e.stopPropagation();onJump?.(s.id);}}>↗</button>
        <InlineSceneText value={c.summary_manual||c.summary_source===s.source?c.summary:''} label={fmt("编辑场景 {0} 概括",i+1)} onSave={async(text)=>{if(useEditorStore.getState().document?.id!==doc.id)throw new Error(tr("项目已切换"));await updateBoard(doc.id,b=>{const scene=scenesOf(useEditorStore.getState().document).find(scene=>scene.id===s.id);if(!scene)throw new Error(tr("场景已删除"));return {...b,cards:{...b.cards,[s.id]:{...(b.cards[s.id]??defaultCard()),summary:text,summary_source:scene.source,summary_manual:!!text.trim()}}};},undefined,false);}}/>
        <button className="card-comment" draggable={false} aria-label={fmt("场景 {0} 评论",i+1)} title={fmt("评论 · {0} 条",c.comments?.length||0)} onClick={e=>{e.stopPropagation();setCommentId(s.id);}}><svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M21 11.5a8.4 8.4 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.4 8.4 0 0 1-3.8-.9L3 21l1.9-5.7a8.4 8.4 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.4 8.4 0 0 1 3.8-.9h.5a8.5 8.5 0 0 1 8 8v.5Z"/></svg>{!!c.comments?.length&&<span className="card-comment-dot" aria-hidden="true"/>}</button>
        <button draggable={false} className="card-resize" title={tr("拖动调整便签宽高")} aria-label={fmt("调整场景 {0} 便签大小",i+1)} onPointerDown={e=>resize(e,s.id,size.width,size.height)}>◢</button>
      </article>;}

  const laneOfScene=(id:string)=>board.cards[id]?.lane ?? '';
  const unitsInLane=(lane:string)=>units.filter(unit=> unit.kind==='scene' ? laneOfScene(unit.id)===lane : groupLane(board.groups.find(g=>g.id===unit.id)! )===lane);
  const renderUnit=(unit:{kind:'scene'|'group';id:string})=> {
    if (unit.kind==='scene') return renderCard(scenes.find(s=>s.id===unit.id)!,scenes.findIndex(s=>s.id===unit.id));
    const group=board.groups.find(g=>g.id===unit.id)!;const id='group:'+group.id;
    return <section className={`board-group ${selectedNode===id?'selected':''} ${group.collapsed?'folded':''}`} key={id} data-board-node={id} data-scene-id={id} data-insertion={insertion?.id===id?insertion.side:undefined}>
      <header draggable onDragStart={e=>{setDraggedUnit(id);e.dataTransfer.setData('application/x-muben-group',group.id);e.dataTransfer.effectAllowed='move';}} onDragEnd={()=>{setDraggedUnit(null);setDropLane(null);setDropTarget(null);}} onPointerDown={e=>{e.stopPropagation();startDrag(e,id);}} onClick={()=>selectNode(id)} tabIndex={0}>
        <button className="lane-toggle" aria-label={group.collapsed?fmt("展开场景组 {0}",group.name):fmt("收起场景组 {0}",group.name)} title={group.collapsed?tr("展开"):tr("收起")} onClick={e=>{e.stopPropagation();edit(b=>({...b,groups:b.groups.map(g=>g.id===group.id?{...g,collapsed:!g.collapsed}:g)}));}}>{group.collapsed?'▸':'▾'}</button>
        <strong>{group.name}</strong><span>{group.scene_ids.length}{tr(" 场")}</span><input aria-label={fmt("组名 {0}",group.name)} value={group.name} maxLength={200} onChange={e=>{const name=e.currentTarget.value;edit(b=>({...b,groups:b.groups.map(g=>g.id===group.id?{...g,name}:g)}));}}/><button onClick={()=>{edit(b=>({...b,groups:b.groups.filter(g=>g.id!==group.id),links:b.links.filter(l=>l.from!==id&&l.to!==id)}));}}>{tr("取消成组")}</button>
      </header>
      {!group.collapsed && <div className="board-group-cards">{scenes.filter(s=>group.scene_ids.includes(s.id)).map(s=>renderCard(s,scenes.findIndex(x=>x.id===s.id)))}</div>}
    </section>;
  };
  const renderLane=(lane:BoardLane|null)=> {
    const laneId=lane?.id ?? '', key=lane?lane.id:'__unassigned__', collapsed=lane?.collapsed ?? false, items=unitsInLane(laneId);
    return <section key={key} className={`board-lane${lane?'':' board-lane-unassigned'}${dropLane===key?' drop-lane':''}${collapsed?' folded':''}`} style={lane?.color?{borderColor:lane.color,background:`${lane.color}26`}:undefined}
      onDragOver={e=>{if(draggedUnit){e.preventDefault();e.stopPropagation();setDropLane(key);}}}
      onDragLeave={e=>{if(!e.currentTarget.contains(e.relatedTarget as Node))setDropLane(current=>current===key?null:current);}}
      onDrop={e=>{e.preventDefault();e.stopPropagation();dropInto(laneId);}}>
      <header className="board-lane-head" draggable={!!lane} onDragStart={e=>{if(!lane||(e.target as HTMLElement).closest('input,button')){e.preventDefault();return;}setDraggedUnit('lane:'+lane.id);e.dataTransfer.setData('application/x-muben-lane',lane.id);e.dataTransfer.effectAllowed='move';}} onDragEnd={()=>{setDraggedUnit(null);setDropLane(null);}}>
        {lane ? <>
          <button className="lane-toggle" aria-label={collapsed?fmt("展开分区 {0}",lane.name):fmt("收起分区 {0}",lane.name)} title={collapsed?tr("展开"):tr("收起")} onClick={()=>laneEdit(lane.id,{collapsed:!lane.collapsed})}>{collapsed?'▸':'▾'}</button>
          <input className="lane-name" aria-label={tr("分区名称")} value={lane.name} maxLength={200} onChange={e=>laneEdit(lane.id,{name:e.currentTarget.value})}/>
          <span className="lane-count">{items.length}{tr(" 张便签")}</span>
          <button aria-label={fmt("上移分区 {0}",lane.name)} title={tr("上移分区")} onClick={()=>moveLane(lane.id,-1)}>↑</button>
          <button aria-label={fmt("下移分区 {0}",lane.name)} title={tr("下移分区")} onClick={()=>moveLane(lane.id,1)}>↓</button>
          <BoardColorPicker label={tr("分区颜色")} value={lane.color||'#F5F1FB'} onChange={color=>laneEdit(lane.id,{color})}/>
          <button onClick={()=>{setCreateError('');setAddingLane(lane.id);setAdding(true);}}>{tr("＋ 便签")}</button>
          <button onClick={()=>removeLane(lane.id)}>{tr("删除分区")}</button>
        </> : <>
          <strong>{tr("未分区")}</strong>
          <span className="lane-count">{items.length}{tr(" 张便签")}</span>
          <span className="lane-hint-text">{tr("从分区拖出的便签会留在这里")}</span>
        </>}
      </header>
      {lane && !collapsed && <input className="lane-note" aria-label={fmt("分区说明 {0}",lane.name)} placeholder={tr("分区说明：这一幕或这一拍要完成什么")} value={lane.hint} maxLength={2000} onChange={e=>laneEdit(lane.id,{hint:e.currentTarget.value})}/>}
      {collapsed ? <p className="lane-folded">{items.length}{tr(" 项已收起")}</p> : <div className="board-lane-cards">{items.map(renderUnit)}</div>}
    </section>;
  };


  return <ModalBackdrop className="scene-board-backdrop" onDismiss={()=>void closeBoard()} message={tr("关闭白板？已经应用的场景与便签修改会保留，尚未提交的输入会放弃。")}><section ref={dialogRef} tabIndex={-1} className="scene-board" role="dialog" aria-modal="true" aria-label={tr("场景白板")}>
    <header className="board-heading"><div><h2>{tr("场景白板")}</h2><span>{scenes.length}{tr(" 个场景 · 拖动标题调整顺序，与剧本和场景列表同步")}</span></div><button onClick={()=>void closeBoard()}>{tr("返回写作 ×")}</button></header>
    <div className="board-tools">
      <button className="primary" onClick={()=>{setCreateError('');setAddingLane('');setAdding(true);}}>{tr("新增便签")}</button>
      <details className="board-appearance"><summary>{tr("便签外观")} ▾</summary><div className="board-appearance-content">
      <div className="board-color-field">{tr("默认颜色 ")}<BoardColorPicker label={tr("默认便签颜色")} value={shownColor} onChange={changeDefaultColor}/></div>
      <button onClick={()=>edit(b=>({...b,cards:Object.fromEntries(Object.entries(b.cards).filter((entry): entry is [string,SceneCard]=>!!entry[1]).map(([id,c])=>[id,{...c,color:''}]))}))}>{tr("应用到全部便签")}</button>
      <div className="board-color-field">{tr("选中便签 ")}<BoardColorPicker label={tr("选中便签颜色")} disabled={!selected} value={selectedCard?.color || shownColor} onChange={color=>{if(selected)cardEdit(selected,{color});}}/></div>
      <span className="board-swatches" aria-label={tr("常用便签颜色")}>{[['#FFF2A8',tr("黄色")],['#FFD7DE',tr("粉色")],['#CDE9FF',tr("蓝色")],['#D7F0D2',tr("绿色")],['#E7DBFF',tr("紫色")]].map(([color,name])=><button key={color} aria-label={fmt("便签颜色：{0}",name)} title={selected ? fmt("将选中便签设为{0}",name) : fmt("将默认便签设为{0}",name)} style={{background:color}} onClick={()=>{if(selected)cardEdit(selected,{color});else changeDefaultColor(color);}} />)}</span>
      <button disabled={!selected} onClick={()=> {if(selected)cardEdit(selected,{width:240,height:240});}}>{tr("恢复正方形")}</button>
      </div></details>
      <button aria-label={tr("撤销")} title={tr("撤销")} onClick={()=>void useEditorStore.getState().undo()}><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8"><path d="m8 4-5 5 5 5M3 9h10a7 7 0 0 1 0 14"/></svg></button><button aria-label={tr("重做")} title={tr("重做")} onClick={()=>void useEditorStore.getState().redo()}><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8"><path d="m16 4 5 5-5 5M21 9H11a7 7 0 0 0 0 14"/></svg></button>
      <div className="board-zoom-tools" aria-label={tr("白板缩放")}><button aria-label={tr("缩小白板")} onClick={()=>changeZoom(zoom-.1)}>−</button><output aria-label={tr("白板缩放比例")}>{Math.round(zoom*100)}%</output><button aria-label={tr("放大白板")} onClick={()=>changeZoom(zoom+.1)}>＋</button><button onClick={()=>changeZoom(1)}>{tr("重置缩放")}</button></div>
      <label className="board-ai-toggle"><input type="checkbox" checked={enabled} onChange={e=> {const checked=e.currentTarget.checked;if(checked&&configured!==true){window.dispatchEvent(new CustomEvent('model-required',{detail:tr("请先配置模型，才能开启场景概括。")}));return;}paused.current=false;edit(b=>({...b,summaries_enabled:checked}));}}/>{tr("自动概括场景")}</label>
      <button onClick={onConfigure}>{tr("配置模型…")}</button>
    </div>
    <div className="board-lane-bar" role="toolbar" aria-label={tr("构思分区工具")}>
      <strong>{tr("构思分区")}</strong>
      <button onClick={addLane}>{tr("新增分区")}</button>
      <select aria-label={tr("构思模板")} value={templateId} onChange={e=>setTemplateId(e.target.value)}>
        <option value="">{tr("选择构思模板…")}</option>
        {templates.map(t=><option key={t.id} value={t.id}>{t.name}</option>)}
      </select>
      <button disabled={!templateId} onClick={()=>void applyTemplate()}>{tr("套用模板")}</button>
      {configured===true && lanes.length>0 && <button disabled={classifying} onClick={()=>void classifyLanes()}>{classifying?tr("正在通读剧本…"):tr("用模型自动划分")}</button>}
      <span className="lane-hint-text">{tr("把便签拖进分区即自动归属；拖动分区标题可整体移动。")}</span>
    </div>
    {laneMessage&&<p className="board-progress" role="status">{laneMessage}</p>}
    {checked.length>=2 && <div className="board-selection-actions" role="toolbar" aria-label={tr("所选场景操作")}>
      <span>{tr("已选 ")}{checked.length}{tr(" 个场景")}</span>
      <input aria-label={tr("组名")} placeholder={tr("组名（可选）")} value={groupName} maxLength={200} onChange={e=>setGroupName(e.target.value)}/>
      <button onClick={createGroup}>{tr("成组")}</button>
      {checked.length===2&&<button onClick={()=>{edit(b=>({...b,links:[...b.links,{id:crypto.randomUUID(),from:checked[0],to:checked[1],label:'',arrow:'end'}]}));setChecked([]);}}>{tr("连线")}</button>}
      <button onClick={()=>setChecked([])}>{tr("取消选择")}</button>
    </div>}
    <div className="board-structure-tools"><button disabled={!selectedNode} onClick={()=>setLinkFrom(linkFrom?null:selectedNode)}>{linkFrom?tr("取消连线"):tr("从所选便签或组连线")}</button>{linkFrom&&<span>{tr("点击另一便签或组完成连线")}</span>}<select aria-label={tr("编辑连接线")} value={editingLink??''} onChange={e=>setEditingLink(e.target.value||null)}><option value="">{tr("编辑连接线…")}</option>{board.links.map((l,i)=><option key={l.id} value={l.id}>{l.label||fmt("连线 {0}",i+1)}</option>)}</select></div>
    {editingLink&&(()=>{const link=board.links.find(l=>l.id===editingLink);if(!link)return null;const patch=(p:Partial<typeof link>)=>edit(b=>({...b,links:b.links.map(l=>l.id===link.id?{...l,...p}:l)}));return <div className="board-link-editor"><input aria-label={tr("连线注释")} value={link.label} maxLength={1000} onChange={e=>patch({label:e.currentTarget.value})}/><select aria-label={tr("连线箭头")} value={link.arrow} onChange={e=>patch({arrow:e.currentTarget.value})}><option value="end">{tr("终点箭头")}</option><option value="start">{tr("起点箭头")}</option><option value="both">{tr("双向箭头")}</option><option value="none">{tr("无箭头")}</option></select><button onClick={()=>{edit(b=>({...b,links:b.links.filter(l=>l.id!==link.id)}));setEditingLink(null);}}>{tr("删除连线")}</button><button onClick={()=>setEditingLink(null)}>{tr("完成")}</button></div>;})()}
    <p className="board-hint">{tr("⌘ 点击多个便签，或在空白处拖动框选，即可成组；选中两个便签还可连线。点击便签可单独改色；拖动右下角改变宽高。按住右键滚轮或触控板双指捏合可缩放，双指滑动可浏览画布。")}{configured===false ? tr("请先配置模型，才能开启场景概括。") : tr("开启概括后，会将场景正文发送给已配置的模型；概括不修改正文。")}</p>
    {error && <div className="board-error" role="alert">{error}<button onClick={()=>{paused.current=false;failed.current.clear();setError('');setTick(t=>t+1);}}>{tr("重试概括")}</button></div>}
    {generating && <AgentWorkStatus label={fmt("正在概括场景：{0}",generating)} cancelLabel={tr("停止后续概括")} cancelling={stoppingSummary} onCancel={()=>{paused.current=true;setStoppingSummary(true);edit(b=>({...b,summaries_enabled:false}));}}/>}
    {generating && <p className="board-progress" role="status">{tr("正在概括：")}{generating}{tr("（关闭白板或关闭开关可停止后续场景）")}</p>}
    <div className="board-canvas" ref={canvasRef} onPointerDown={e=>{
      if(e.button!==0 || (e.target as HTMLElement).closest('article,header,button,input,select,svg'))return;
      e.preventDefault();stopSelection.current?.();
      const x=e.clientX,y=e.clientY,previous=e.metaKey||e.ctrlKey?checked:[];
      setSelected(null);setChecked(previous);
      const move=(event:PointerEvent)=>{
        const left=Math.min(x,event.clientX),top=Math.min(y,event.clientY),right=Math.max(x,event.clientX),bottom=Math.max(y,event.clientY);
        setSelectionBox({left,top,width:right-left,height:bottom-top});
        const ids=Array.from(cardsRef.current?.querySelectorAll<HTMLElement>('.scene-card')??[]).filter(el=>{
          const r=el.getBoundingClientRect();return r.right>left&&r.left<right&&r.bottom>top&&r.top<bottom;
        }).map(el=>el.dataset.sceneId!);
        setChecked([...new Set([...previous,...ids])]);
      };
      const finish=()=>{window.removeEventListener('pointermove',move);window.removeEventListener('pointerup',finish);window.removeEventListener('pointercancel',finish);setSelectionBox(null);stopSelection.current=null;};
      stopSelection.current=finish;window.addEventListener('pointermove',move);window.addEventListener('pointerup',finish);window.addEventListener('pointercancel',finish);
    }}><div className="board-cards" ref={cardsRef} style={{zoom,width:`${100/zoom}%`}}>
      {!scenes.length && <p>{tr("还没有场景。点击“新增便签”，从第一场戏开始构思。")}</p>}
      <BoardConnections root={cardsRef} board={board} zoom={zoom} onEdit={setEditingLink}/>
      {lanes.length===0 ? units.map(renderUnit) : <>{renderLane(null)}{lanes.map(lane=>renderLane(lane))}</>}
    </div></div>
    {selectionBox&&<div className="board-selection-box" style={selectionBox}/> }
    <button className="board-hide-links" onClick={()=>edit(b=>({...b,hide_links:!b.hide_links}))}>{board.hide_links?tr("显示连线"):tr("隐藏连线")}</button>
    {adding&&<ModalBackdrop disabled={creating} dirty={!!(newTitle||newSummary)} onDismiss={()=>setAdding(false)} message={tr("放弃这张尚未添加的便签？场景标题和梗概不会保存。")}><form className="modal board-create-scene" role="dialog" aria-modal="true" aria-label={tr("新增便签")} onKeyDown={e=>e.stopPropagation()} onSubmit={e=>{e.preventDefault();void createScene();}}>
      <h3>{tr("新增便签")}</h3><p>{tr("便签添加到最后一场，之后可以拖动排序。正文只添加场景标题，梗概留在白板。")}</p>
      <label>{tr("场景标题")}<input autoFocus disabled={creating} aria-label={tr("新便签场景标题")} placeholder={tr("例如：内景。咖啡馆——白天")} maxLength={1000} value={newTitle} onChange={e=>setNewTitle(e.target.value.replace(/[\r\n]+/g,' '))}/></label>
      <label>{tr("场景梗概（可选）")}<textarea disabled={creating} aria-label={tr("新便签场景梗概")} rows={5} maxLength={1000} value={newSummary} onChange={e=>setNewSummary(e.target.value)}/></label>
      {createError&&<p role="alert">{createError}</p>}<div className="modal-actions"><button type="button" disabled={creating} onClick={()=>setAdding(false)}>{tr("取消")}</button><button type="submit" className="primary" disabled={creating||!newTitle.trim()}>{creating?tr("保存中…"):tr("添加场景")}</button></div>
    </form></ModalBackdrop>}
    {commentId&&<BoardNotes id={commentId} title={headingDisplays[commentId]??scenes.find(s=>s.id===commentId)?.title??tr("场景")} board={board} onSave={(comments,expected)=>updateBoard(doc.id,b=>{const current=b.cards[commentId]??defaultCard();if(JSON.stringify(current.comments)!==JSON.stringify(expected))throw new Error(tr("评论已更新，请重新检查后提交"));return {...b,cards:{...b.cards,[commentId]:{...current,comments}}};})} onClose={()=>setCommentId(null)}/>}
  </section></ModalBackdrop>;
}
