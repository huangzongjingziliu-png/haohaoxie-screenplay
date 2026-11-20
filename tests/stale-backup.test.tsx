import {afterEach,beforeEach,it,expect} from 'vitest';
import {cleanup,render,screen,fireEvent,waitFor} from '@testing-library/react';
import StaleBackupPrompt from '@/components/StaleBackupPrompt';
import {mockBackend} from './setup';

const found={
  backup:'/tmp/screenplay-save-0a1b2c3d-剧.screenplay.bak',
  container:'/Users/me/剧.screenplay',
  savedAt:'2026-11-20T10:00:00Z',
  size:4096,
};

beforeEach(()=>{
  mockBackend.reset();
  mockBackend.on('stale_save_backup',()=>found);
  mockBackend.on('bootstrap',()=>({
    project:{state:'empty',path:null,title:'',revision:0,dirty:false,blockCount:0},
    document:null,sourceText:'',recent:[],
  }));
});
afterEach(cleanup);

it('启动时发现残留备份只提示，不自动写回',async()=>{
  render(<StaleBackupPrompt/>);
  await screen.findByRole('dialog',{name:'发现未完成的保存备份'});
  expect(screen.getByText('剧.screenplay')).toBeTruthy();
  expect(mockBackend.callsTo('restore_stale_save_backup')).toHaveLength(0);
  expect(mockBackend.callsTo('dismiss_stale_save_backup')).toHaveLength(0);
});

it('用户确认后才恢复，并刷新界面状态',async()=>{
  mockBackend.on('restore_stale_save_backup',()=>null);
  render(<StaleBackupPrompt/>);
  await screen.findByRole('dialog',{name:'发现未完成的保存备份'});

  fireEvent.click(screen.getByText('用备份恢复'));

  await waitFor(()=>expect(mockBackend.callsTo('restore_stale_save_backup')).toHaveLength(1));
  expect(mockBackend.callsTo('restore_stale_save_backup')[0].args).toEqual({path:found.container});
  await waitFor(()=>expect(mockBackend.callsTo('bootstrap').length).toBeGreaterThan(0));
  await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());
});

it('拒绝恢复时保留备份且不再提示',async()=>{
  mockBackend.on('dismiss_stale_save_backup',()=>true);
  render(<StaleBackupPrompt/>);
  await screen.findByRole('dialog',{name:'发现未完成的保存备份'});

  fireEvent.click(screen.getByText('保留备份，不再提示'));

  await waitFor(()=>expect(mockBackend.callsTo('dismiss_stale_save_backup')).toHaveLength(1));
  expect(mockBackend.callsTo('dismiss_stale_save_backup')[0].args).toEqual({path:found.container});
  expect(mockBackend.callsTo('restore_stale_save_backup')).toHaveLength(0);
  await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());
});

it('没有残留备份时不打扰用户',async()=>{
  mockBackend.on('stale_save_backup',()=>null);
  render(<StaleBackupPrompt/>);
  await waitFor(()=>expect(mockBackend.callsTo('stale_save_backup')).toHaveLength(1));
  expect(screen.queryByRole('dialog')).toBeNull();
});
