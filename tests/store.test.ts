/**
 * Store 行为测试
 *
 * 验证说明书要求的关键行为：
 * 1. 保存失败**不得**标记为已保存
 * 2. REVISION_CONFLICT 时必须重新拉取权威状态
 * 3. IME 组字期间不提交修改
 * 4. 新建项目后不残留旧文档
 * 5. dirty 状态以后端为准
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useEditorStore } from '@/stores/editor';
import { mockBackend } from './setup';

const EMPTY_PROJECT = {
  state: 'empty' as const,
  path: null,
  title: '',
  revision: 0,
  dirty: false,
  blockCount: 0,
};

function makeDoc(
    blocks: Array<{ id: string; text: string; type?: string }> = [],
    revision = 3
  ) {
    return {
      id: 'doc-1',
      format_version: 1,
      revision,
    metadata: { title: '测试剧本', author: '', language: 'zh' },
    title_page: { title: '测试剧本' },
    blocks: blocks.map((b) => ({
      id: b.id,
      block_type: b.type ?? 'action',
      text: b.text,
      scene_id: null,
      character_id: null,
      source: null,
      confidence: 1,
      revision: 3,
      scene_heading: null,
      metadata: {},
    })),
    characters: [],
    locations: [],
    scenes: [],
    settings: {},
    story_bible: {},
    project_rules: {},
    created_at: '',
    updated_at: '',
  };
}

function okResult(doc = makeDoc(), sourceText = '') {
  return {
    revision: doc.revision,
    document: doc,
    version_id: null,
    affected: [],
    source_text: sourceText,
    pagination: { pages: [], total_pages: 0, has_title_page: false },
  };
}

beforeEach(() => {
  mockBackend.reset();
  useEditorStore.setState({
    document: null,
    project: EMPTY_PROJECT,
    sourceText: '',
    toasts: [],
    composing: false,
    activeBlockId: null,
  });
});

describe('项目生命周期', () => {
  it('t01_新建项目后 store 反映后端状态', async () => {
    mockBackend.on('new_project', () => ({
      state: 'unsaved',
      path: null,
      title: '新剧本',
      revision: 0,
      dirty: true,
      blockCount: 0,
    }));
    mockBackend.on('get_document', () => makeDoc());
    mockBackend.on('get_source_text', () => '');

    await useEditorStore.getState().newProject('新剧本');

    const s = useEditorStore.getState();
    expect(s.project.state).toBe('unsaved');
    expect(s.project.title).toBe('新剧本');
    expect(s.document).not.toBeNull();
  });

  it('t02_新建项目会清空上一个项目的文档内容', async () => {
    // 先装载项目 A
    mockBackend.on('bootstrap', () => ({
      project: { ...EMPTY_PROJECT, state: 'open', path: '/a.screenplay', title: 'A', revision: 5 },
      document: makeDoc([{ id: 'B-1', text: '项目A的内容' }]),
      sourceText: '项目A的内容',
      recent: [],
    }));
    await useEditorStore.getState().bootstrap();
    expect(useEditorStore.getState().document?.blocks[0].text).toBe('项目A的内容');

    // 新建项目 B
    mockBackend.on('new_project', () => ({
      state: 'unsaved',
      path: null,
      title: 'B',
      revision: 0,
      dirty: true,
      blockCount: 0,
    }));
    mockBackend.on('get_document', () => makeDoc([]));
    mockBackend.on('get_source_text', () => '');

    await useEditorStore.getState().newProject('B');

    const s = useEditorStore.getState();
    expect(s.project.title).toBe('B');
    expect(s.document?.blocks).toHaveLength(0);
    expect(s.project.path).toBeNull();
  });

  it('t03_关闭项目后回到空状态', async () => {
    mockBackend.on('bootstrap', () => ({
      project: { ...EMPTY_PROJECT, state: 'open', path: '/a.screenplay', title: 'A', revision: 5 },
      document: makeDoc(),
      sourceText: '',
      recent: [],
    }));
    await useEditorStore.getState().bootstrap();

    mockBackend.on('close_project', () => EMPTY_PROJECT);
    await useEditorStore.getState().closeProject();

    const s = useEditorStore.getState();
    expect(s.project.state).toBe('empty');
    expect(s.document).toBeNull();
  });
});

describe('保存语义（说明书 A-07 / A-10 / A-11）', () => {
  it('t04_保存失败不得标记已保存，且必须展示错误', async () => {
    useEditorStore.setState({
      project: { ...EMPTY_PROJECT, state: 'open', path: '/tmp/x.screenplay', title: 'x', dirty: true, revision: 1 },
    });
    mockBackend.on('save_project', () => {
      throw { error: 'APP_STATE', message: '磁盘只读，保存失败' };
    });

    const result = await useEditorStore.getState().save();

    expect(result).toBeNull();
    const s = useEditorStore.getState();
    // 关键断言：dirty 必须仍为 true
    expect(s.project.dirty).toBe(true);
    // 且必须有可见的错误提示（不能静默吞掉）
    const errToast = s.toasts.find((t) => t.kind === 'error');
    expect(errToast).toBeDefined();
    expect(errToast!.message).toContain('磁盘只读');
  });

  it('t05_保存成功后重新拉取后端 projectInfo（不信任前端判断）', async () => {
    mockBackend.on('save_project', () => ({
      path: '/tmp/x.screenplay',
      savedRevision: 3,
      savedAt: '2026-01-01T00:00:00Z',
    }));
    mockBackend.on('project_info_cmd', () => ({
      state: 'open',
      path: '/tmp/x.screenplay',
      title: 'x',
      revision: 3,
      dirty: false,
      blockCount: 2,
    }));

    await useEditorStore.getState().save();

    expect(mockBackend.callsTo('project_info_cmd')).toHaveLength(1);
    expect(useEditorStore.getState().project.dirty).toBe(false);
  });
});

describe('修改与冲突处理', () => {
  it('t06_REVISION_CONFLICT 时重新 bootstrap 拉取权威状态', async () => {
    useEditorStore.setState({
      document: makeDoc([{ id: 'B-1', text: '原内容' }]) as never,
      project: { ...EMPTY_PROJECT, state: 'open', revision: 3 },
    });

    mockBackend.on('replace_block', () => {
      throw { error: 'REVISION_CONFLICT', expected: 3, actual: 9, message: '已被他人修改' };
    });
    mockBackend.on('bootstrap', () => ({
      project: { ...EMPTY_PROJECT, state: 'open', revision: 9, title: 'x' },
      document: makeDoc([{ id: 'B-1', text: '他人的内容' }]),
      sourceText: '他人的内容',
      recent: [],
    }));

    await useEditorStore.getState().changeBlockText('B-1', '我的修改');

    // 必须重新拉取
    expect(mockBackend.callsTo('bootstrap')).toHaveLength(1);
    // store 必须显示他人的最新内容，而不是我的修改
    expect(useEditorStore.getState().document?.blocks[0].text).toBe('他人的内容');
    // 且展示了冲突错误
    expect(useEditorStore.getState().toasts.some((t) => t.code === 'REVISION_CONFLICT')).toBe(true);
  });

  it('t07_IMA 组字期间不提交修改', async () => {
    useEditorStore.setState({
      document: makeDoc([{ id: 'B-1', text: '' }]) as never,
      project: { ...EMPTY_PROJECT, state: 'open', revision: 1 },
      composing: true,
    });
    mockBackend.on('replace_block', () => okResult());

    await useEditorStore.getState().changeBlockText('B-1', '正在输入的拼音');

    // 关键：组字期间绝不能发起修改
    expect(mockBackend.callsTo('replace_block')).toHaveLength(0);
  });

  it('t08_正常修改后使用后端返回的 source_text（不是前端拼的）', async () => {
    const newDoc = makeDoc([{ id: 'B-1', text: '后端序列化结果' }]);
    mockBackend.on('insert_block', () => okResult(newDoc, '后端给的自由文本'));

    await useEditorStore.getState().insertBlock(null, 'action', 'x');

    expect(useEditorStore.getState().sourceText).toBe('后端给的自由文本');
    expect(mockBackend.callsTo('insert_block')[0].args).toMatchObject({
      afterId: null,
      blockType: 'action',
      text: 'x',
    });
  });

  it('t09_修改后自动刷新 projectInfo（dirty 由后端判定）', async () => {
    mockBackend.on('insert_block', () => okResult());
    mockBackend.on('project_info_cmd', () => ({
      state: 'open',
      path: '/a.screenplay',
      title: 'x',
      revision: 4,
      dirty: true,
      blockCount: 1,
    }));

    await useEditorStore.getState().insertBlock(null, 'action', 'x');

    expect(mockBackend.callsTo('project_info_cmd').length).toBeGreaterThan(0);
    expect(useEditorStore.getState().project.dirty).toBe(true);
  });
});

describe('消息提示', () => {
  it('t10_错误提示保留 code 便于定位', () => {
    useEditorStore.getState().pushToast('error', '出错了', 'REVISION_CONFLICT');
    const t = useEditorStore.getState().toasts[0];
    expect(t.code).toBe('REVISION_CONFLICT');
    expect(t.message).toBe('出错了');
  });

  it('t11_最多保留 5 条消息', () => {
    for (let i = 0; i < 8; i++) {
      useEditorStore.getState().pushToast('info', `消息${i}`);
    }
    const toasts = useEditorStore.getState().toasts;
    expect(toasts.length).toBeLessThanOrEqual(5);
    expect(toasts[toasts.length - 1].message).toBe('消息7');
  });

  it('t12_dismissToast 能移除指定消息', () => {
    useEditorStore.getState().pushToast('info', 'A');
    const id = useEditorStore.getState().toasts[0].id;
    useEditorStore.getState().dismissToast(id);
    expect(useEditorStore.getState().toasts).toHaveLength(0);
  });
});

describe('块类型与合并（B5 专业编辑器）', () => {
  it('t13_类型切换必须走 set_block_type 而不是删除+插入', async () => {
    useEditorStore.setState({
      document: makeDoc([{ id: 'B-1', text: '内场 客厅 日' }]) as never,
      project: { ...EMPTY_PROJECT, state: 'open', revision: 3 },
    });
    mockBackend.on('set_block_type', () => okResult());

    await useEditorStore.getState().changeBlockType('B-1', 'shot');

    expect(mockBackend.callsTo('set_block_type')).toHaveLength(1);
    expect(mockBackend.callsTo('set_block_type')[0].args).toMatchObject({
      blockId: 'B-1',
      blockType: 'shot',
      expectedRevision: 3,
    });
    // 关键：绝不能用「删除+插入」模拟类型切换（会换掉稳定 ID）
    expect(mockBackend.callsTo('delete_block')).toHaveLength(0);
    expect(mockBackend.callsTo('insert_block')).toHaveLength(0);
  });

  it('t14_合并块必须带 expectedRevision', async () => {
    useEditorStore.setState({
      document: makeDoc([
        { id: 'B-1', text: '第一段' },
        { id: 'B-2', text: '第二段' },
      ]) as never,
      project: { ...EMPTY_PROJECT, state: 'open', revision: 3 },
    });
    mockBackend.on('merge_blocks', () => okResult());

    await useEditorStore.getState().mergeBlock('B-1', 'B-2');

    expect(mockBackend.callsTo('merge_blocks')).toHaveLength(1);
    expect(mockBackend.callsTo('merge_blocks')[0].args).toMatchObject({
      firstId: 'B-1',
      secondId: 'B-2',
      expectedRevision: 3,
    });
  });

  it('t15_合并冲突时重新拉取权威状态', async () => {
    useEditorStore.setState({
      document: makeDoc([
        { id: 'B-1', text: '第一段' },
        { id: 'B-2', text: '第二段' },
      ]) as never,
      project: { ...EMPTY_PROJECT, state: 'open', revision: 3 },
    });
    mockBackend.on('merge_blocks', () => {
      throw { error: 'REVISION_CONFLICT', expected: 3, actual: 7, message: '已被他人修改' };
    });
    mockBackend.on('bootstrap', () => ({
      project: { ...EMPTY_PROJECT, state: 'open', revision: 7, title: 'x' },
      document: makeDoc([
        { id: 'B-1', text: '第一段' },
        { id: 'B-2', text: '第二段' },
        { id: 'B-3', text: '他人插入' },
      ]),
      sourceText: '',
      recent: [],
    }));

    await useEditorStore.getState().mergeBlock('B-1', 'B-2');

    expect(mockBackend.callsTo('bootstrap')).toHaveLength(1);
    expect(useEditorStore.getState().document?.blocks).toHaveLength(3);
    expect(useEditorStore.getState().toasts.some((t) => t.code === 'REVISION_CONFLICT')).toBe(true);
  });

  it('t16_所有修改命令都必须携带 expectedRevision', async () => {
    // 后端 project.revision 与 document.revision 始终同源（store.revision()），
    // 前端必须以 document.revision 为准发出乐观并发控制参数。
    useEditorStore.setState({
      document: makeDoc([{ id: 'B-1', text: '内容' }], 7) as never,
      project: { ...EMPTY_PROJECT, state: 'open', revision: 7 },
    });

    // 模拟真实后端：每次成功修改都递增 revision 并回传新文档
    let rev = 7;
    const handler = () => {
      rev += 1;
      return okResult(makeDoc([{ id: 'B-1', text: '内容' }], rev));
    };
    mockBackend.on('replace_block', handler);
    mockBackend.on('insert_block', handler);
    mockBackend.on('delete_block', handler);
    mockBackend.on('set_block_type', handler);
    mockBackend.on('merge_blocks', handler);
    mockBackend.on('replace_source_text', handler);

    const st = useEditorStore.getState();
    await st.changeBlockText('B-1', '改');
    await st.insertBlock('B-1', 'action', 'x');
    await st.deleteBlock('B-1');
    await st.changeBlockType('B-1', 'shot');
    await st.mergeBlock('B-1', 'B-2');
    await st.changeSourceText('新的自由文本');

    const cmds = [
      'replace_block',
      'insert_block',
      'delete_block',
      'set_block_type',
      'merge_blocks',
      'replace_source_text',
    ];
    for (const cmd of cmds) {
      const calls = mockBackend.callsTo(cmd);
      expect(calls.length, `${cmd} 未被调用`).toBeGreaterThan(0);
      for (const c of calls) {
        expect(c.args, `${cmd} 缺少 expectedRevision`).toHaveProperty(
          'expectedRevision'
        );
        // 必须是数字（不能是 undefined / null）
        expect(typeof (c.args as { expectedRevision: unknown }).expectedRevision).toBe(
          'number'
        );
      }
    }
    expect(rev).toBe(13);
  });

  it('t17_文档事件到达时刷新后端权威文档，不信任本地快照', async () => {
    // 模拟 CLI/MCP 写入后后端广播：GUI 必须显示新内容
    useEditorStore.setState({
      document: makeDoc([{ id: 'B-1', text: '旧内容' }], 3) as never,
      project: { ...EMPTY_PROJECT, state: 'open', revision: 3 },
    });
    const incoming = makeDoc([{ id: 'B-1', text: 'Agent 改过的内容' }], 4);
    mockBackend.on('get_document', () => incoming);

    // App.tsx 的 document-changed 订阅会调用 bootstrap
    mockBackend.on('bootstrap', () => ({
      project: { ...EMPTY_PROJECT, state: 'open', revision: 4, title: 'x' },
      document: incoming,
      sourceText: 'Agent 改过的内容',
      recent: [],
    }));

    await useEditorStore.getState().bootstrap();

    expect(useEditorStore.getState().document?.blocks[0].text).toBe(
      'Agent 改过的内容'
    );
    expect(useEditorStore.getState().project.revision).toBe(4);
  });
});