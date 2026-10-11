import type {ScreenplayBlock} from '@/ipc/types/ScreenplayBlock';
export type SourceSegment = {from:number;to:number;ids:string[]};
/** Rust ranges are UTF-8 bytes; textarea selection offsets are UTF-16. */
export function sourceSegments(text:string,blocks:ScreenplayBlock[]):SourceSegment[] {
  const offsets=new Map<number,number>([[0,0]]);let bytes=0,index=0;
  for(const char of text){const point=char.codePointAt(0)!;bytes+=point<=0x7f?1:point<=0x7ff?2:point<=0xffff?3:4;index+=char.length;offsets.set(bytes,index);}
  const ranges=blocks.flatMap(b=>{const from=offsets.get(b.source?.from??-1),to=offsets.get(b.source?.to??-1);return from!==undefined&&to!==undefined&&to>from?[{from,to,ids:[b.id]}]:[];}).sort((a,b)=>a.from-b.from||a.to-b.to);
  const result:SourceSegment[]=[];
  for(const range of ranges){const previous=result[result.length-1];if(previous&&range.from<previous.to){previous.to=Math.max(previous.to,range.to);previous.ids.push(...range.ids);}else result.push(range);}
  return result;
}
export function sourceBlockAt(segments:SourceSegment[],offset:number):string|null {
  const inside=segments.find(s=>offset>=s.from&&offset<s.to);
  if(inside)return inside.ids[0];
  const preceding=segments.filter(s=>s.from<=offset);const previous=preceding[preceding.length-1];
  return (previous??segments[0])?.ids[0]??null;
}
export type ScrollAnchor={id:string;top:number;height:number};
export function correspondingScroll(from:ScrollAnchor[],to:ScrollAnchor[],scroll:number,viewport:number,targetViewport:number):number|null {
  if(!from.length)return null;
  const reference=scroll+viewport*.25;
  const preceding=from.filter(a=>a.top<=reference);const anchor=preceding[preceding.length-1]??from[0];
  const target=to.find(a=>a.id===anchor.id);if(!target)return null;
  const fraction=Math.max(0,Math.min(1,(reference-anchor.top)/Math.max(anchor.height,1)));
  return Math.max(0,target.top+target.height*fraction-targetViewport*.25);
}
