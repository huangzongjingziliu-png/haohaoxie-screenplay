import { act, cleanup, fireEvent, render, waitFor, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import SceneBoard from '@/components/SceneBoard';
import Navigator from '@/components/Navigator';
import { scenesOf, movedOrder, defaultBoard, updateBoard } from '@/utils/scenes';
import { useEditorStore } from '@/stores/editor';
import { mockBackend } from './setup';
const blocks = [{id:'pre',block_type:'general',text:'前言'},{id:'a',block_type:'scene_heading',text:'INT. A - DAY'},{id:'aa',block_type:'action',text:'A动作'},{id:'b',block_type:'scene_heading',text:'EXT. B - NIGHT'},{id:'bb',block_type:'dialogue',text:'B对白'}];
const doc = () => ({id:'board-qa',revision:1,blocks,settings:{scene_board:defaultBoard()},characters:[],locations:[]}) as never;
beforeEach(()=>{mockBackend.reset();useEditorStore.setState({document:doc(),navigatorTab:'scenes',activeBlockId:null,applyResult:vi.fn(async r=>{useEditorStore.setState({document:r.document});})});mockBackend.on('agent_get_config',()=>({providerKind:'echo',model:'',baseUrl:''}));});
afterEach(cleanup);
it('场景内容用于概括，列表和白板从相同正文派生，移动到目标位置',()=>{
  expect(scenesOf(doc()).map(s=>s.source)).toEqual(['INT. A - DAY\nA动作','EXT. B - NIGHT\nB对白']);
  expect(movedOrder(['a','b','c'],'a','c')).toEqual(['b','c','a']);
  expect(movedOrder(['a','b','c'],'c','a')).toEqual(['c','a','b']);
});
it('未开启模型时白板仅展示标题，没有请求；白板和导航拖动调用同一排序命令',async()=>{
  mockBackend.on('reorder_scenes',args=>({document:{...doc() as object,revision:2,blocks:[blocks[0],blocks[3],blocks[4],blocks[1],blocks[2]]},revision:2}));
  const {container,getByLabelText}=render(<SceneBoard onClose={()=>{}} onConfigure={()=>{}}/>);
  await waitFor(()=>expect(getByLabelText('默认便签颜色色板')).toBeTruthy());
  expect(container.querySelectorAll('.scene-card')).toHaveLength(2);
  expect(container.querySelector('.scene-summary')).toBeNull();
  expect(mockBackend.callsTo('agent_summarize_scene')).toHaveLength(0);
  const transfer={setData:vi.fn(),getData:()=> 'a',effectAllowed:''};
  fireEvent.dragStart(container.querySelector('[data-scene-id="a"] header')!,{dataTransfer:transfer});
  fireEvent.drop(container.querySelector('[data-scene-id="b"]')!,{dataTransfer:transfer});
  await waitFor(()=>expect(mockBackend.callsTo('reorder_scenes')[0].args?.sceneIds).toEqual(['b','a']));
  await waitFor(()=>expect(container.querySelector('.scene-card')?.getAttribute('data-scene-id')).toBe('b'));
  cleanup();
  const nav=render(<Navigator/>);
  const rows=nav.container.querySelectorAll('li');
  const next={setData:vi.fn(),getData:()=> 'b',effectAllowed:''};
  fireEvent.dragStart(rows[0],{dataTransfer:next});fireEvent.drop(rows[1],{dataTransfer:next});
  await waitFor(()=>expect(mockBackend.callsTo('reorder_scenes')[1].args?.sceneIds).toEqual(['a','b']));
});
it('便签改色保存到工程，不同工程的排队修改不会串写',async()=>{
  mockBackend.on('update_scene_board',args=>({document:{...doc() as object,revision:2,settings:{scene_board:args.board}},revision:2}));
  await updateBoard('board-qa',b=>({...b,color:'#AABBCC'}));
  expect(mockBackend.callsTo('update_scene_board')[0].args?.expectedRevision).toBe(1);
  expect(useEditorStore.getState().document?.settings.scene_board.color).toBe('#AABBCC');
  await updateBoard('other-project',b=>({...b,color:'#000000'}));
  expect(mockBackend.callsTo('update_scene_board')).toHaveLength(1);
});

it('开启后只概括未缓存场景，结果只保存到便签，不改正文',async()=>{
  const board=defaultBoard();board.summaries_enabled=true;board.cards.a={color:'',width:0,height:0,comments:[],summary:'缓存A',summary_source:'INT. A - DAY\nA动作'};
  useEditorStore.setState({document:{...doc() as object,settings:{scene_board:board}} as never});
  mockBackend.on('agent_get_config',()=>({providerKind:'ollama',model:'local',baseUrl:''}));
  mockBackend.on('agent_summarize_scene',()=>({summary:'B概括',source:'EXT. B - NIGHT\nB对白',projectId:'board-qa'}));
  mockBackend.on('update_scene_board',args=>({document:{...doc() as object,revision:2,settings:{scene_board:args.board}},revision:2}));
  const view=render(<SceneBoard onClose={()=>{}} onConfigure={()=>{}}/>);
  await waitFor(()=>expect(view.getByText('B概括')).toBeTruthy(),{timeout:2000});
  expect(mockBackend.callsTo('agent_summarize_scene')).toHaveLength(1);
  expect(mockBackend.callsTo('agent_summarize_scene')[0].args?.sceneId).toBe('b');
  expect(useEditorStore.getState().document?.blocks).toEqual(blocks);
  expect(view.getByText('缓存A')).toBeTruthy();
});
it('模型返回期间正文变化，过期概括不会覆盖当前场景',async()=>{
  const board=defaultBoard();board.summaries_enabled=true;
  useEditorStore.setState({document:{...doc() as object,settings:{scene_board:board}} as never});
  mockBackend.on('agent_get_config',()=>({providerKind:'ollama',model:'local',baseUrl:''}));
  let resolve!:(value:unknown)=>void;
  mockBackend.on('agent_summarize_scene',()=>new Promise(r=>{resolve=r;}));
  render(<SceneBoard onClose={()=>{}} onConfigure={()=>{}}/>);
  await waitFor(()=>expect(mockBackend.callsTo('agent_summarize_scene')).toHaveLength(1),{timeout:2000});
  expect(screen.getByRole('dialog',{name:'Agent工作状态'})).toBeTruthy();
  expect(screen.getByRole('button',{name:'停止后续概括'})).toBeTruthy();
  act(()=>useEditorStore.setState({document:{...doc() as object,revision:2,blocks:blocks.map(b=>b.id==='aa'?{...b,text:'新的动作'}:b),settings:{scene_board:board}} as never}));
  await act(async()=>resolve({summary:'旧概括',source:'INT. A - DAY\nA动作',projectId:'board-qa'}));
  expect(mockBackend.callsTo('update_scene_board')).toHaveLength(0);
  expect(useEditorStore.getState().document?.blocks[2].text).toBe('新的动作');
});
it('鼠标直接拖动便签的正文区域也能排序，轻点不排序',async()=>{
  vi.stubGlobal('PointerEvent',MouseEvent);
  const original=document.elementFromPoint;
  try {
    mockBackend.on('reorder_scenes',()=>({document:{...doc() as object,revision:2},revision:2}));
    const {container}=render(<SceneBoard onClose={()=>{}} onConfigure={()=>{}}/>);
    const a=container.querySelector('[data-scene-id="a"]')!, b=container.querySelector('[data-scene-id="b"]')!;
    document.elementFromPoint=vi.fn(()=>b);
    fireEvent.pointerDown(a,{button:0,clientX:10,clientY:10});fireEvent.pointerUp(window,{clientX:10,clientY:10});
    expect(mockBackend.callsTo('reorder_scenes')).toHaveLength(0);
    fireEvent.pointerDown(a,{button:0,clientX:10,clientY:10});fireEvent.pointerMove(window,{clientX:100,clientY:10});fireEvent.pointerUp(window,{clientX:100,clientY:10});
    await waitFor(()=>expect(mockBackend.callsTo('reorder_scenes')[0].args?.sceneIds).toEqual(['b','a']));
  } finally { document.elementFromPoint=original;vi.unstubAllGlobals(); }
});
it('颜色选择器即时显示新色并将选择时的颜色保存，而不是稍后读取重置值',async()=>{
  let finish!:(r:unknown)=>void;
  mockBackend.on('update_scene_board',args=>new Promise(r=>{finish=()=>r({document:{...doc() as object,revision:2,settings:{scene_board:args.board}},revision:2});}));
  const view=render(<SceneBoard onClose={()=>{}} onConfigure={()=>{}}/>);
  fireEvent.click(view.getByLabelText('默认便签颜色色板'));
  const input=view.getByLabelText('默认便签颜色') as HTMLInputElement;
  fireEvent.change(input,{target:{value:'#123456'}});
  expect(input.value).toBe('#123456');
  await waitFor(()=>expect(mockBackend.callsTo('update_scene_board')).toHaveLength(1));
  expect((mockBackend.callsTo('update_scene_board')[0].args?.board as {color:string}).color).toBe('#123456');
  await act(async()=>finish(undefined));
  expect(input.value).toBe('#123456');
});
it('缩放按钮、触控板缩放及右键滚轮生效，便签提供标准剧本跳转',async()=>{
  const jump=vi.fn();
  const view=render(<SceneBoard onClose={()=>{}} onConfigure={()=>{}} onJump={jump}/>);
  fireEvent.click(view.getByLabelText('放大白板'));
  expect(view.getByLabelText('白板缩放比例').textContent).toBe('110%');
  fireEvent.click(view.getByText('重置缩放'));
  const canvas=view.container.querySelector('.board-canvas')!;
  fireEvent.wheel(canvas,{deltaY:-100,ctrlKey:true});
  expect(view.getByLabelText('白板缩放比例').textContent).not.toBe('100%');
  fireEvent.click(view.getByText('重置缩放'));
  vi.stubGlobal('PointerEvent',MouseEvent);
  fireEvent.pointerDown(canvas,{button:2});fireEvent.wheel(canvas,{deltaY:100});fireEvent.pointerUp(window);
  expect(view.getByLabelText('白板缩放比例').textContent).not.toBe('100%');
  vi.unstubAllGlobals();
  fireEvent.click(view.getByLabelText('跳转到场景 2 的标准剧本'));
  expect(jump).toHaveBeenCalledWith('b');
});
it('拖动的插入线与放置前后语义一致',async()=>{
  expect(movedOrder(['a','b','c'],'a','b','before')).toEqual(['a','b','c']);
  expect(movedOrder(['a','b','c'],'a','b','after')).toEqual(['b','a','c']);
  vi.stubGlobal('PointerEvent',MouseEvent);const original=document.elementFromPoint;
  try {
    mockBackend.on('reorder_scenes',()=>({document:doc(),revision:2}));
    const view=render(<SceneBoard onClose={()=>{}} onConfigure={()=>{}}/>);
    const a=view.container.querySelector('[data-scene-id="a"]')!,b=view.container.querySelector('[data-scene-id="b"]')!;
    vi.spyOn(b,'getBoundingClientRect').mockReturnValue({left:100,right:300,top:0,bottom:240} as DOMRect);document.elementFromPoint=vi.fn(()=>b);
    fireEvent.pointerDown(a,{button:0,clientX:10,clientY:20});fireEvent.pointerMove(window,{clientX:250,clientY:20});
    expect(b.getAttribute('data-insertion')).toBe('after');
    fireEvent.pointerUp(window,{clientX:250,clientY:20});
    expect(b.hasAttribute('data-insertion')).toBe(false);
    await waitFor(()=>expect(mockBackend.callsTo('reorder_scenes')[0].args?.sceneIds).toEqual(['b','a']));
  } finally {document.elementFromPoint=original;vi.unstubAllGlobals();}
});

it('组成组是连续场景并可整体移动，评论和连线保存在工程',async()=>{
 mockBackend.on('update_scene_board',args=>({document:{...useEditorStore.getState().document,revision:(useEditorStore.getState().document?.revision??0)+1,settings:{scene_board:args.board}},revision:2}));
 const view=render(<SceneBoard onClose={()=>{}} onConfigure={()=>{}}/>);
 fireEvent.click(view.getByLabelText('场景 1：INT. A - DAY'));fireEvent.click(view.getByLabelText('场景 2：EXT. B - NIGHT'),{metaKey:true});fireEvent.change(view.getByLabelText('组名'),{target:{value:'第一幕'}});fireEvent.click(view.getByRole('button',{name:'成组'}));
 await waitFor(()=>expect(useEditorStore.getState().document?.settings.scene_board.groups[0].scene_ids).toEqual(['a','b']));
 expect(view.container.querySelector('.board-group')).toBeTruthy();
 fireEvent.click(view.getByLabelText('场景 1 评论'));fireEvent.change(view.getByLabelText('新增场景评论'),{target:{value:'这里增加伏笔'}});fireEvent.click(view.getByText('添加评论'));
 await waitFor(()=>expect(useEditorStore.getState().document?.settings.scene_board.cards.a?.comments).toEqual(['这里增加伏笔']));fireEvent.click(view.getByText('完成'));
 fireEvent.click(view.getByLabelText('场景 1：INT. A - DAY'));fireEvent.click(view.getByText('从所选便签或组连线'));fireEvent.click(view.getByLabelText('场景 2：EXT. B - NIGHT'));
 await waitFor(()=>expect(useEditorStore.getState().document?.settings.scene_board.links[0]).toMatchObject({from:'a',to:'b',arrow:'end'}));
 fireEvent.click(view.getByText('隐藏连线'));await waitFor(()=>expect(useEditorStore.getState().document?.settings.scene_board.hide_links).toBe(true));
});

it('移动组不打散成员，拖动便签进入组时调整连续顺序',async()=>{
 const {moveBoardUnit}=await import('@/utils/scenes');
 const extra=[{id:'c',block_type:'scene_heading',text:'INT. C - DAY'},{id:'cc',block_type:'action',text:'C动作'}];
 const board=defaultBoard();board.groups=[{id:'g',name:'一组',scene_ids:['a','b']}];
 useEditorStore.setState({document:{...doc() as object,blocks:[...blocks,...extra],settings:{scene_board:board}} as never});
 mockBackend.on('update_scene_board',args=>({document:{...useEditorStore.getState().document,settings:{scene_board:args.board}},revision:2}));
 await moveBoardUnit('board-qa','group:g','a','after');expect(mockBackend.callsTo('update_scene_board')).toHaveLength(0);
 await moveBoardUnit('board-qa','group:g','c','after');expect(mockBackend.callsTo('update_scene_board')[0].args?.sceneIds).toEqual(['c','a','b']);
 await moveBoardUnit('board-qa','c','group:g','after');const last=mockBackend.callsTo('update_scene_board')[1].args;expect(last?.sceneIds).toEqual(['a','b','c']);expect((last?.board as typeof board).groups[0].scene_ids).toEqual(['a','b','c']);
});

it('框选两个便签显示成组和连线，不再显示逐便签复选框',async()=>{
 mockBackend.on('update_scene_board',args=>({document:{...useEditorStore.getState().document,revision:2,settings:{scene_board:args.board}},revision:2}));
 const view=render(<SceneBoard onClose={()=>{}} onConfigure={()=>{}}/>);
 const cards=view.container.querySelectorAll<HTMLElement>('.scene-card');
 cards.forEach((card,i)=>vi.spyOn(card,'getBoundingClientRect').mockReturnValue({left:10+i*100,top:10,right:90+i*100,bottom:90,width:80,height:80,x:10+i*100,y:10,toJSON:()=>({})}));
 expect(view.queryByRole('button',{name:'成组'})).toBeNull();
 expect(view.container.querySelector('.card-extras input')).toBeNull();
 vi.stubGlobal('PointerEvent',MouseEvent);
 fireEvent.pointerDown(view.container.querySelector('.board-canvas')!,{button:0,clientX:0,clientY:0});
 fireEvent.pointerMove(window,{clientX:200,clientY:100});
 fireEvent.pointerUp(window,{clientX:200,clientY:100});
 expect(view.getByRole('button',{name:'成组'})).toBeTruthy();
 fireEvent.click(view.getByRole('button',{name:'连线'}));
 await waitFor(()=>expect(useEditorStore.getState().document?.settings.scene_board.links[0]).toMatchObject({from:'a',to:'b'}));
});
