import {act,cleanup,fireEvent,render,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,expect,it} from 'vitest';
import {useSplitSync} from '@/hooks/useSplitSync';
import SourceEditor from '@/editors/source/SourceEditor';
import {useEditorStore} from '@/stores/editor';
import {sourceSegments,sourceBlockAt,correspondingScroll} from '@/utils/sourcePositions';
const blocks=[{id:'a',text:'a',source:{from:0,to:1}},{id:'b',text:'b',source:{from:3,to:4}}] as never;
beforeEach(()=>useEditorStore.setState({viewMode:'split',sourceText:'a\n\nb',document:{id:'sync',revision:1,blocks} as never,activeBlockId:null}));
afterEach(cleanup);
it('maps UTF-8 backend ranges to Chinese and emoji textarea offsets, including repeated text',()=>{
 const text='轻声🙂\n轻声';
 const ranges=sourceSegments(text,[{id:'first',source:{from:0,to:10}},{id:'second',source:{from:11,to:17}}] as never);
 expect(ranges).toEqual([{from:0,to:4,ids:['first']},{from:5,to:7,ids:['second']}]);
 expect(sourceBlockAt(ranges,6)).toBe('second');
 expect(sourceSegments(text,[{id:'bad',source:{from:1,to:4}}] as never)).toEqual([]);
});
it('uses paragraph anchors instead of whole-document percentages and supports merged inline ranges',()=>{
 expect(correspondingScroll([{id:'b',top:100,height:20}],[{id:'b',top:400,height:40}],100,100,100)).toBe(415);
 expect(correspondingScroll([{id:'missing',top:0,height:10}],[],0,100,100)).toBeNull();
 expect(sourceSegments('abc',[{id:'a',source:{from:0,to:2}},{id:'b',source:{from:1,to:3}}] as never)).toEqual([{from:0,to:3,ids:['a','b']}]);
});
function Harness(){useSplitSync();return <div className="editors"><SourceEditor/><div className="paper-workspace"><div className="screenplay-editor" tabIndex={0}><div data-block-id="a"/><div data-block-id="b"/></div></div></div>;}
function rect(top:number,height:number){return {top,height,width:500,left:0,bottom:top+height,right:500,x:0,y:top,toJSON:()=>({})};}
function geometry(container:HTMLElement){
 const source=container.querySelector('textarea')!,paper=container.querySelector<HTMLElement>('.screenplay-editor')!;
 for(const element of [source,paper]){Object.defineProperty(element,'clientHeight',{configurable:true,value:100});element.getBoundingClientRect=()=>rect(0,100);}
 container.querySelectorAll<HTMLElement>('[data-source-blocks]').forEach((el,i)=>el.getBoundingClientRect=()=>rect(i*100-source.scrollTop,20));
 paper.querySelectorAll<HTMLElement>('[data-block-id]').forEach((el,i)=>el.getBoundingClientRect=()=>rect(i*400-paper.scrollTop,20));
 return {source,paper};
}
it('synchronizes both directions and ignores the resulting programmatic scroll event',async()=>{
 const {container}=render(<Harness/>);const {source,paper}=geometry(container);
 source.scrollTop=100;fireEvent.scroll(source);await waitFor(()=>expect(paper.scrollTop).toBe(395));
 fireEvent.scroll(paper);expect(source.scrollTop).toBe(100);
 paper.scrollTop=390;fireEvent.scroll(paper);await waitFor(()=>expect(source.scrollTop).toBe(90));
 fireEvent.scroll(source);expect(paper.scrollTop).toBe(390);
});
it('marks a source selection in both views without moving the other caret or changing text',async()=>{
 const {container}=render(<Harness/>);const {source,paper}=geometry(container);
 source.focus();source.setSelectionRange(3,3);fireEvent.select(source);
 await waitFor(()=>expect(useEditorStore.getState().activeBlockId).toBe('b'));
 expect(container.querySelector('[data-source-blocks="b"]')?.classList.contains('active')).toBe(true);
 const before=useEditorStore.getState().sourceText;
 paper.focus();act(()=>useEditorStore.getState().setActiveBlock('a'));
 expect(source.selectionStart).toBe(3);expect(useEditorStore.getState().sourceText).toBe(before);
 expect(container.querySelector('[data-source-blocks="a"]')?.classList.contains('active')).toBe(true);
});
it('leaves single-view scrolling alone',async()=>{
 useEditorStore.setState({viewMode:'right'});const {container}=render(<Harness/>);const {source,paper}=geometry(container);
 source.scrollTop=100;fireEvent.scroll(source);expect(paper.scrollTop).toBe(0);
});

it('does not align a draft using outdated backend ranges',()=>{
 const {container}=render(<Harness/>);const {source,paper}=geometry(container);
 source.value='new uncommitted paragraph';source.scrollTop=100;fireEvent.scroll(source);
 expect(paper.scrollTop).toBe(0);
});
