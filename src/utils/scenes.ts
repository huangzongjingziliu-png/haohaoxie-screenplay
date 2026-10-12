import type { ScreenplayDocument } from '@/ipc/types';
import type { SceneBoard } from '@/ipc/types/SceneBoard';
import type { SceneCard } from '@/ipc/types/SceneCard';
import { useEditorStore } from '@/stores/editor';
import { ipc } from '@/ipc/client';
import { flushPendingEdits } from '@/utils/pendingEdits';
export const defaultBoard = (): SceneBoard => ({color:'#FFF2A8', summaries_enabled:false, cards:{}, groups:[], links:[], hide_links:false});
export const defaultCard = (): SceneCard => ({color:'',width:0,height:0,summary:'',summary_source:'',summary_manual:false,comments:[]});
export function scenesOf(doc: ScreenplayDocument | null) {
  const blocks = doc?.blocks ?? [];
  const scenes: {id:string;title:string;source:string}[] = [];
  let texts: string[] = [];
  for (const block of blocks) {
    if (block.block_type === 'scene_heading') {
      if (scenes.length) scenes[scenes.length-1].source = texts.join('\n');
      scenes.push({id:block.id,title:block.text,source:''});texts=[];
    }
    if (scenes.length) texts.push(block.text);
  }
  if (scenes.length) scenes[scenes.length-1].source = texts.join('\n');
  return scenes;
}
export function movedOrder(ids: string[], from: string, to: string, side?: 'before' | 'after'): string[] {
  if (from === to || !ids.includes(from) || !ids.includes(to)) return ids;
  const result = ids.filter(id=>id!==from);
  result.splice(side ? result.indexOf(to) + (side === 'after' ? 1 : 0) : ids.indexOf(to),0,from); return result;
}
export async function moveScene(from: string, to: string, side?: 'before' | 'after') {
  await flushPendingEdits();
  const store = useEditorStore.getState(), doc = store.document;
  if (!doc) return;
  const ids = scenesOf(doc).map(s=>s.id), next = movedOrder(ids,from,to,side);
  if (next === ids) return;
  await store.applyResult(await ipc.reorderScenes(next,doc.revision));
}
// Queue presentation updates and merge each edit with the latest authoritative document.
let boardQueue: Promise<unknown> = Promise.resolve();
export async function updateBoard(projectId: string, change: (board: SceneBoard) => SceneBoard, order?: (ids:string[],board:SceneBoard)=>string[], flushDrafts=true) {
  // Flush before taking the board queue slot: a pending card may enqueue its own save.
  // Holding the slot while waiting for that card would deadlock both operations.
  if(flushDrafts)await flushPendingEdits();
  const task = boardQueue.catch(()=>undefined).then(async () => {
    const store = useEditorStore.getState(), doc = store.document;
    if (!doc || doc.id !== projectId) return;
    const board = change(structuredClone(doc.settings.scene_board ?? defaultBoard()));
    if (!order && JSON.stringify(board) === JSON.stringify(doc.settings.scene_board ?? defaultBoard())) return;
    await store.applyResult(await ipc.updateSceneBoard(board,doc.revision,order?.(scenesOf(doc).map(s=>s.id),board)));
  });
  boardQueue = task; return task;
}

export function moveBoardUnit(projectId:string,from:string,to:string,side:'before'|'after') {
 const initial=useEditorStore.getState().document?.settings.scene_board;
 const own=from.startsWith('group:')?initial?.groups.find(g=>g.id===from.slice(6)):undefined;
 if(from===to||own?.scene_ids.includes(to))return Promise.resolve();
 let moving:string[]=[];
 return updateBoard(projectId,b=>{
  const ids=scenesOf(useEditorStore.getState().document).map(s=>s.id);
  const fromGroup=from.startsWith('group:')?b.groups.find(g=>g.id===from.slice(6)):undefined;
  moving=fromGroup?ids.filter(id=>fromGroup.scene_ids.includes(id)):[from];
  if(!moving.length||moving.includes(to)||from===to)return b;
  const targetGroup=to.startsWith('group:')?b.groups.find(g=>g.id===to.slice(6)):b.groups.find(g=>g.scene_ids.includes(to));
  if(!fromGroup){b.groups=b.groups.map(g=>({...g,scene_ids:g.scene_ids.filter(id=>id!==from)}));if(targetGroup){const g=b.groups.find(g=>g.id===targetGroup.id);if(g)g.scene_ids.push(from);}}
  return b;
 },(ids,b)=>{
  const rest=ids.filter(id=>!moving.includes(id));
  const group=to.startsWith('group:')?b.groups.find(g=>g.id===to.slice(6)):from.startsWith('group:')?b.groups.find(g=>g.scene_ids.includes(to)):undefined;
  const anchor=group?rest.filter(id=>group.scene_ids.includes(id)): [to];
  let at=group?(side==='before'?rest.indexOf(anchor[0]):rest.indexOf(anchor[anchor.length-1])+1):rest.indexOf(to)+(side==='after'?1:0);
  if(at<0)at=rest.length;rest.splice(at,0,...moving);return rest;
 });
}
