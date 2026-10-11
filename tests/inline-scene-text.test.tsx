import {useState} from 'react';
import {cleanup,render,screen,fireEvent,waitFor} from '@testing-library/react';
import {afterEach,it,expect,vi} from 'vitest';
import InlineSceneText from '@/components/InlineSceneText';
afterEach(cleanup);
it('preview and editing share the same element and preserve scrolling without opening an input box',async()=>{
 const saved=vi.fn();function Harness(){const [value,setValue]=useState('首行\n末行');return <InlineSceneText value={value} label="概括" onSave={async(text)=>{saved(text);setValue(text);}}/>;}
 const {container}=render(<Harness/>);const preview=screen.getByRole('button',{name:'概括'});preview.scrollTop=90;fireEvent.doubleClick(preview);const editor=screen.getByRole('textbox',{name:'概括'});expect(editor).toBe(preview);expect(editor.scrollTop).toBe(90);expect(editor.getAttribute('contenteditable')).toBe('plaintext-only');expect(container.querySelector('textarea,input')).toBeNull();editor.textContent='自己写的概括';fireEvent.input(editor);fireEvent.pointerDown(document.body);await waitFor(()=>expect(saved).toHaveBeenCalledWith('自己写的概括'));await waitFor(()=>expect(screen.getByRole('button',{name:'概括'})).toBe(preview));expect(preview.textContent).toBe('自己写的概括');
});
it('empty preview placeholder is visual guidance, not content saved to the project',async()=>{const save=vi.fn();render(<InlineSceneText value="" label="概括" onSave={save}/>);const node=screen.getByRole('button',{name:'概括'});expect(node.textContent).toBe('');expect(node.dataset.placeholder).toBe('双击填写场景概括');fireEvent.doubleClick(node);fireEvent.pointerDown(document.body);await waitFor(()=>expect(screen.getByRole('button',{name:'概括'})).toBe(node));expect(save).not.toHaveBeenCalled();});
