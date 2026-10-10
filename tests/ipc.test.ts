/**
 * IPC 客户端测试
 *
 * 验证：
 * 1. 命令名与参数名正确（Tauri 会把 camelCase 转 snake_case）
 * 2. 结构化错误被正确解析，不是字符串
 * 3. 修改类命令都带 expectedRevision
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { ipc, ScreenplayError } from '@/ipc/client';
import { mockBackend } from './setup';

beforeEach(() => mockBackend.reset());

describe('IPC 命令契约', () => {
  it('t01_修改命令携带 expectedRevision', async () => {
    mockBackend.on('replace_block', () => ({
      revision: 2,
      document: { blocks: [] },
      version_id: null,
      affected: ['B-1'],
      source_text: '',
      pagination: { pages: [], total_pages: 0, has_title_page: false },
    }));

    await ipc.replaceBlock('B-1', '新内容', 1);

    const calls = mockBackend.callsTo('replace_block');
    expect(calls).toHaveLength(1);
    // 参数名必须是 camelCase，Tauri 会自动转 snake_case
    expect(calls[0].args).toMatchObject({
      blockId: 'B-1',
      text: '新内容',
      expectedRevision: 1,
    });
  });

  it('t02_insertBlock 的 afterId 为 null 时传 null 而非 undefined', async () => {
    mockBackend.on('insert_block', () => ({
      revision: 1,
      document: { blocks: [] },
      version_id: null,
      affected: [],
      source_text: '',
      pagination: { pages: [], total_pages: 0, has_title_page: false },
    }));

    await ipc.insertBlock(null, 'action', '文字');

    const calls = mockBackend.callsTo('insert_block');
    expect(calls[0].args).toMatchObject({ afterId: null, blockType: 'action', text: '文字' });
  });

  it('t03_replaceSourceText 携带 expectedRevision', async () => {
    mockBackend.on('replace_source_text', () => ({
      revision: 5,
      document: { blocks: [] },
      version_id: null,
      affected: [],
      source_text: 'x',
      pagination: { pages: [], total_pages: 0, has_title_page: false },
    }));

    await ipc.replaceSourceText('新文本', 4);
    expect(mockBackend.callsTo('replace_source_text')[0].args).toMatchObject({
      text: '新文本',
      expectedRevision: 4,
    });
  });
});

describe('结构化错误处理', () => {
  it('t04_REVISION_CONFLICT 被解析为 ScreenplayError 且带 code', async () => {
    mockBackend.on('replace_block', () => {
      throw {
        error: 'REVISION_CONFLICT',
        expected: 3,
        actual: 7,
        message: '文档已被其他来源修改',
      };
    });

    await expect(ipc.replaceBlock('B-1', 'x', 3)).rejects.toThrow(ScreenplayError);

    try {
      await ipc.replaceBlock('B-1', 'x', 3);
      expect.unreachable('应当抛出');
    } catch (e) {
      const err = e as ScreenplayError;
      expect(err.code).toBe('REVISION_CONFLICT');
      expect(err.detail.expected).toBe(3);
      expect(err.detail.actual).toBe(7);
      // 错误消息必须包含可操作信息
      expect(err.message).toContain('rev 3');
      expect(err.message).toContain('rev 7');
    }
  });

  it('t05_APP_STATE 错误也被正确处理', async () => {
    mockBackend.on('save_project', () => {
      throw { error: 'APP_STATE', message: '尚未指定保存路径' };
    });

    try {
      await ipc.saveProject();
      expect.unreachable('应当抛出');
    } catch (e) {
      const err = e as ScreenplayError;
      expect(err.code).toBe('APP_STATE');
      expect(err.message).toBe('尚未指定保存路径');
    }
  });

  it('t06_字符串异常被规整为结构化错误', async () => {
    mockBackend.on('get_document', () => {
      throw '纯字符串错误';
    });

    try {
      await ipc.getDocument();
      expect.unreachable('应当抛出');
    } catch (e) {
      const err = e as ScreenplayError;
      expect(err.code).toBe('APP_STATE');
      expect(err.message).toBe('纯字符串错误');
    }
  });

  it('t07_双重编码的 JSON 字符串也能解析（旧实现遗留）', async () => {
    mockBackend.on('list_versions', () => {
      // 模拟旧代码把结构化错误再 stringify 一次
      throw JSON.stringify({ error: 'LOCKED', id: 'B-9' });
    });

    try {
      await ipc.listVersions();
      expect.unreachable('应当抛出');
    } catch (e) {
      const err = e as ScreenplayError;
      expect(err.code).toBe('LOCKED');
      expect(err.detail.id).toBe('B-9');
    }
  });

  it('t08_NOT_FOUND 错误消息包含 id', async () => {
    mockBackend.on('delete_block', () => {
      throw { error: 'NOT_FOUND', what: 'block', id: 'B-missing' };
    });

    try {
      await ipc.deleteBlock('B-missing');
      expect.unreachable('应当抛出');
    } catch (e) {
      const err = e as ScreenplayError;
      expect(err.code).toBe('NOT_FOUND');
      expect(err.message).toContain('B-missing');
    }
  });
});

describe('生命周期命令', () => {
  it('t09_新项目不传路径（后端负责创建新 Workspace）', async () => {
    mockBackend.on('new_project', () => ({
      state: 'unsaved',
      path: null,
      title: '测试',
      revision: 0,
      dirty: true,
      blockCount: 0,
    }));

    const info = await ipc.newProject('测试');
    expect(info.state).toBe('unsaved');
    expect(mockBackend.callsTo('new_project')[0].args).toEqual({ title: '测试' });
  });

  it('t10_saveProject 不传 payload（关键：防止前端快照覆盖后端）', async () => {
    mockBackend.on('save_project', () => ({
      path: '/tmp/x.screenplay',
      savedRevision: 3,
      savedAt: '2026-01-01T00:00:00Z',
    }));

    await ipc.saveProject();

    const args = mockBackend.callsTo('save_project')[0].args ?? {};
    // 绝不能出现 payload / document 字段（防止前端快照覆盖后端权威状态）
    expect(Object.keys(args)).toHaveLength(0);
    expect(args).not.toHaveProperty('payload');
    expect(args).not.toHaveProperty('document');
  });

  it('t11_saveProjectAs 只传路径', async () => {
    mockBackend.on('save_project_as', () => ({
      path: '/tmp/y.screenplay',
      savedRevision: 1,
      savedAt: '2026-01-01T00:00:00Z',
    }));

    await ipc.saveProjectAs('/tmp/y.screenplay');
    expect(mockBackend.callsTo('save_project_as')[0].args).toEqual({ path: '/tmp/y.screenplay' });
  });
});