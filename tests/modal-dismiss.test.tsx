import {afterEach,it,expect,vi} from 'vitest';
import {cleanup,render,screen,fireEvent} from '@testing-library/react';
import ModalBackdrop from '@/components/ModalBackdrop';
afterEach(cleanup);
it('outside click cancels an untouched dialog, while clicks inside keep it open',()=>{
 const close=vi.fn();const {container}=render(<ModalBackdrop onDismiss={close}><section role="dialog"><input defaultValue="old"/></section></ModalBackdrop>);
 fireEvent.click(screen.getByRole('dialog'));expect(close).not.toHaveBeenCalled();fireEvent.click(container.firstChild!);expect(close).toHaveBeenCalledOnce();
});
it('edited drafts require explicit confirmation and cancellation keeps the draft',()=>{
 const close=vi.fn();const {container}=render(<ModalBackdrop onDismiss={close} message="放弃评论？"><section role="dialog"><input defaultValue="old"/></section></ModalBackdrop>);
 const input=screen.getByRole('textbox');fireEvent.focus(input);fireEvent.change(input,{target:{value:'new'}});fireEvent.click(container.firstChild!);
 expect(screen.getByRole('alertdialog').textContent).toContain('放弃评论？');expect(close).not.toHaveBeenCalled();fireEvent.click(screen.getByText('取消'));expect(input).toHaveProperty('value','new');fireEvent.click(container.firstChild!);fireEvent.click(screen.getByText('确认'));expect(close).toHaveBeenCalledOnce();
});
it('reverting an edit avoids confirmation, and busy operations cannot be dismissed',()=>{
 const close=vi.fn();const view=render(<ModalBackdrop onDismiss={close}><input defaultValue="old"/></ModalBackdrop>);const input=screen.getByRole('textbox');fireEvent.focus(input);fireEvent.change(input,{target:{value:'new'}});fireEvent.change(input,{target:{value:'old'}});fireEvent.click(view.container.firstChild!);expect(close).toHaveBeenCalledOnce();view.rerender(<ModalBackdrop onDismiss={close} disabled><input/></ModalBackdrop>);fireEvent.click(view.container.firstChild!);expect(close).toHaveBeenCalledOnce();
});
