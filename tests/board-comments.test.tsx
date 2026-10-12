import {afterEach,it,expect,vi} from 'vitest';
import {cleanup,render,screen,fireEvent,waitFor} from '@testing-library/react';
import BoardNotes from '@/components/BoardNotes';
import {defaultBoard} from '@/utils/scenes';
afterEach(cleanup);
it('评论保存失败保留草稿，成功后才清空',async()=>{
 const save=vi.fn().mockRejectedValueOnce(new Error('无法保存')).mockResolvedValue(undefined);
 render(<BoardNotes id="a" title="场景" board={defaultBoard()} onSave={save} onClose={()=>{}}/>);
 const input=screen.getByRole('textbox');fireEvent.change(input,{target:{value:'重要意见'}});fireEvent.click(screen.getByText('添加评论'));
 await waitFor(()=>expect(screen.getByRole('alert').textContent).toContain('无法保存'));
 expect((input as HTMLTextAreaElement).value).toBe('重要意见');
 fireEvent.click(screen.getByText('添加评论'));await waitFor(()=>expect((input as HTMLTextAreaElement).value).toBe(''));
 expect(save.mock.calls[0]).toEqual([['重要意见'],[]]);
});
