import {beforeEach,afterEach,it,expect,vi} from 'vitest';
import {render,screen,fireEvent,cleanup,act} from '@testing-library/react';
import Navigator from '@/components/Navigator';
import SourceEditor from '@/editors/source/SourceEditor';
import {useEditorStore} from '@/stores/editor';
beforeEach(()=>{
 Element.prototype.scrollIntoView=vi.fn();
 const blocks=[{id:'s1',block_type:'scene_heading',text:'INT. 办公室 - 日'},{id:'c1',block_type:'character',text:'小明',character_id:'char'},{id:'d1',block_type:'dialogue',text:'你好',character_id:'char'},{id:'s2',block_type:'scene_heading',text:'INT. 办公室 - 夜'},{id:'a2',block_type:'action',text:'关灯'},{id:'c2',block_type:'character',text:'小明',character_id:'char'},{id:'d2',block_type:'dialogue',text:'再见',character_id:'char'}];
 useEditorStore.setState({document:{id:'p',blocks,scenes:[{id:'s1',location:'办公室'},{id:'s2',location:'办公室'}],characters:[{id:'char',name:'小明',aliases:[]}],locations:[{id:'loc',name:'办公室',count:2}],settings:{font_size:12}} as never,activeBlockId:'a2',navigatorTab:'scenes',viewMode:'left'});
});
afterEach(cleanup);
it('a paragraph highlights its containing scene, location and exact outline row',()=>{
 const view=render(<Navigator/>);expect(view.container.querySelector('[data-scene-id="s2"]')?.className).toBe('active');fireEvent.click(screen.getByText('地点'));expect(view.container.querySelector('.navigator-entity.active')?.textContent).toContain('办公室');fireEvent.click(screen.getByText('大纲'));expect(view.container.querySelector('.outline li.active')?.textContent).toContain('关灯');
});
it('characters and locations jump to their nearest occurrence and expand all positions',()=>{
 render(<Navigator/>);fireEvent.click(screen.getByText('人物'));fireEvent.click(screen.getByRole('button',{name:/小明.*2 处/}));expect(useEditorStore.getState().activeBlockId).toBe('c2');fireEvent.click(screen.getByLabelText('查看小明的出现位置'));fireEvent.click(screen.getByRole('button',{name:/第1场.*你好/}));expect(useEditorStore.getState().activeBlockId).toBe('c1');fireEvent.click(screen.getByText('地点'));fireEvent.click(screen.getByLabelText('查看办公室的出现位置'));fireEvent.click(screen.getByRole('button',{name:/第2场/}));expect(useEditorStore.getState().activeBlockId).toBe('s2');
});
it('free-writing selection updates the shared active paragraph even outside split mode',async()=>{
 useEditorStore.setState({sourceText:'甲\n乙',viewMode:'right',document:{id:'p',blocks:[{id:'one',source:{from:0,to:3}},{id:'two',source:{from:4,to:7}}],settings:{font_size:12}} as never});render(<SourceEditor/>);const text=screen.getByLabelText('自由剧本文本') as HTMLTextAreaElement;text.setSelectionRange(2,2);fireEvent.select(text);expect(useEditorStore.getState().activeBlockId).toBe('two');
});
