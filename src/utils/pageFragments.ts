export function sourceBlockId(id: string): string { return id.replace(/:(?:part|contd):\d+$/, ''); }
export function fragmentContext(source: string, start?: number, end?: number): { prefix: string; suffix: string } {
 if (start === undefined || end === undefined) return {prefix:'',suffix:''};
 if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end < start || end > source.length) throw new Error('分页片段已变化，请结束编辑后重新定位');
 return {prefix:source.slice(0,start),suffix:source.slice(end)};
}
const drafts=new WeakMap<HTMLElement,{prefix:string;suffix:string}>();
export function getFragmentDraft(el:HTMLElement){return drafts.get(el);}
export function rememberFragmentDraft(el:HTMLElement,context:{prefix:string;suffix:string}|null){if(context)drafts.set(el,context);else drafts.delete(el);}
