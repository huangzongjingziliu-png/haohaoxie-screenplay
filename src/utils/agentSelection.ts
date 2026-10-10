import {useEditorStore} from '@/stores/editor';
import {flushPendingEdits} from '@/utils/pendingEdits';
export type AgentTarget={label:string;text:string;instruction?:string;revision:number;projectId:string;blockIds?:string[];blockId?:string;fullText?:string;start?:number;end?:number;source?:string};
export async function sendSceneToAgent(id:string){
 await flushPendingEdits();const {document:doc}=useEditorStore.getState();if(!doc)return;
 const start=doc.blocks.findIndex(b=>b.id===id);if(start<0)return;
 let end=start+1;while(end<doc.blocks.length&&doc.blocks[end].block_type!=='scene_heading')end++;
 const blocks=doc.blocks.slice(start,end);
 const text=blocks.map(b=> b.block_type==='scene_heading'?'.'+b.text:b.block_type==='character'?'@'+b.text:b.block_type==='transition'?'>'+b.text:b.block_type==='parenthetical'?`（${b.text.replace(/^[（(]|[）)]$/g,'')}）`:b.block_type==='action'||b.block_type==='shot'?'!'+b.text:b.text).join('\n\n');
 window.dispatchEvent(new CustomEvent('agent-selection',{detail:{label:doc.blocks[start].text,text,revision:doc.revision,projectId:doc.id,blockIds:blocks.map(b=>b.id)} satisfies AgentTarget}));
}
