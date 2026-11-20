import {expect,it} from 'vitest';
import {shouldBypassUndoShortcut} from '@/utils/shortcuts';

const mount=(html:string)=>{document.body.innerHTML=html;return document.body.querySelector('*')!;};

it('搜索框等文本输入里 ⌘Z 交给浏览器原生撤销',()=>{
  const input=mount('<div><input value="搜索" /></div>').querySelector('input')!;
  expect(shouldBypassUndoShortcut(input)).toBe(true);
});

it('白板便签的输入框同样不被全局撤销拦截',()=>{
  const root=document.createElement('div');
  root.innerHTML='<div class="board-card"><textarea></textarea></div>';
  const textarea=root.querySelector('textarea')!;
  expect(shouldBypassUndoShortcut(textarea)).toBe(true);
});

it('剧本正文块仍走应用级撤销',()=>{
  const root=document.createElement('div');
  root.innerHTML='<div class="screenplay-block" contenteditable="true"></div>';
  expect(shouldBypassUndoShortcut(root.querySelector('.screenplay-block'))).toBe(false);
});

it('普通区域不受影响',()=>{
  const div=document.createElement('div');
  expect(shouldBypassUndoShortcut(div)).toBe(false);
  expect(shouldBypassUndoShortcut(null)).toBe(false);
});
