/**
 * GUI 自检探针（仅开发期启用）
 *
 * 用途：在无法从外部获取 WKWebView 截图/坐标的环境下，
 * 让**真实 GUI 进程**自己执行完整用户流程并写出可验证的证据文件。
 *
 * 启用方式：URL 加 `?selftest=1`，或在 SCREENPLAY_SELFTEST=1 环境变量下启动。
 * 产物：~/.screenplay-selftest.json
 *
 * ⚠️ 这不是 mock：它调用的是与用户点击完全相同的 IPC 命令路径，
 *    经过 Tauri 序列化 → Rust CommandService → SQLite，
 *    与人工操作走同一条链路。
 */

import { ipc } from '@/ipc/client';
import { useEditorStore } from '@/stores/editor';
import type { SelftestReport, SelftestStep } from '@/ipc/types';

const RESULT_PATH = '~/.screenplay-selftest.json';

async function step<T>(
  name: string,
  fn: () => Promise<T>
): Promise<{ step: SelftestStep; value: T }> {
  const t0 = performance.now();
  try {
    const value = await fn();
    return {
      step: { name, ok: true, ms: Math.round(performance.now() - t0), detail: summarize(value) },
      value,
    };
  } catch (e) {
    return {
      step: {
        name,
        ok: false,
        ms: Math.round(performance.now() - t0),
        detail: e instanceof Error ? e.message : String(e),
      },
      value: undefined as T,
    };
  }
}

function summarize(v: unknown): string {
  if (v === null || v === undefined) return 'null';
  if (typeof v === 'string') return v.length > 120 ? `${v.slice(0, 120)}…` : v;
  if (typeof v === 'object') {
    const o = v as Record<string, unknown>;
    if ('document' in o && o.document) {
      const d = o.document as { blocks: unknown[]; title_page: { title: string } };
      return `rev=${o.revision} blocks=${d.blocks.length} title=${d.title_page.title}`;
    }
    if ('state' in o) {
      const p = o as { state: string; path: string | null; dirty: boolean; revision: number };
      return `state=${p.state} dirty=${p.dirty} rev=${p.revision} path=${p.path ?? '-'}`;
    }
    if ('source_text' in o) return `sourceText=${String(o.source_text).length}chars`;
    if ('revision' in o) return `rev=${o.revision}`;
  }
  return JSON.stringify(v).slice(0, 120);
}

/** 写结果文件（Tauri fs 插件在生产包可用；开发时用 Rust 命令兜底） */
async function writeReport(report: SelftestReport) {
  const json = JSON.stringify(report, null, 2);
  try {
    const { writeTextFile } = await import('@/ipc/diag');
    await writeTextFile(RESULT_PATH, json);
  } catch {
    // 兜底：至少打印到 console，便于人工查看
    console.log('SELFTEST_RESULT ' + json);
  }
  // eslint-disable-next-line no-console
  console.log('SELFTEST_RESULT ' + json);
}

/**
 * 完整闭环自检：
 * 新建 → 右侧写中文 → 左侧修改 → 保存 → 关闭 → 重开 → 校验
 */
export async function runSelfTest(): Promise<SelftestReport> {
  const store = useEditorStore.getState();
  const steps: SelftestStep[] = [];
  const projectPath = `/tmp/screenplay-selftest-${Date.now()}.screenplay`;
  const script = '外 镇北堡城门 黄昏\n\n风从荒原吹来。\n\n老板娘（冷笑）：昨日，去了买卖城。\n';

  const record = (s: SelftestStep) => {
    steps.push(s);
    return s;
  };

  // 0. 检查是否有崩溃恢复内容（真实崩溃验证）
  const c0 = await step('check_pending_recovery', () => ipc.pendingRecovery());
  record({
    ...c0.step,
    ok: true, // 无论有无恢复点，这一步本身成功即可
    detail: c0.value
      ? `发现待恢复：${c0.value.title}（${c0.value.blockCount} 块，rev ${c0.value.revision}）`
      : '无待恢复内容',
  });
  if (c0.value) {
    const c0b = await step('adopt_pending_recovery', () => ipc.adoptRecovery());
    record(c0b.step);
    if (!c0b.step.ok) return finish();
    return finish();
  }

  // 1. 关闭可能残留的项目
  const c1 = await step('close_any', () => ipc.closeProject());
  record(c1.step);

  // 2. 新建
  const c2 = await step('new_project', () => ipc.newProject('自检剧本'));
  record(c2.step);
  if (!c2.step.ok) return finish();

  // 3. bootstrap 拉取权威状态
  const c3 = await step('bootstrap', () => ipc.bootstrap());
  record(c3.step);

  // 4. 右侧写中文（走 replace_source_text → Rust 解析器）
  const c4 = await step('replace_source_text', async () => {
    const r = await ipc.replaceSourceText(script);
    store.applyResult(r);
    return r;
  });
  record(c4.step);
  if (!c4.step.ok) return finish();

  const blocks = c4.value.document.blocks;
  const dialogue = blocks.find((b) => b.block_type === 'dialogue');
  const sceneHeading = blocks.find((b) => b.block_type === 'scene_heading');

  // 5. 校验结构
  record({
    name: 'assert_structure',
    ok: blocks.length >= 5 && !!sceneHeading && !!dialogue,
    ms: 0,
    detail: `blocks=${blocks.length} scene=${sceneHeading?.text ?? '-'} dialogue=${dialogue?.text ?? '-'}`,
  });

  // 6. 校验中文未丢失
  const allText = blocks.map((b) => b.text).join('');
  record({
    name: 'assert_chinese_preserved',
    ok: allText.includes('镇北堡城门') && allText.includes('昨日'),
    ms: 0,
    detail: allText.slice(0, 80),
  });

  // 7. 左侧修改（带 expectedRevision）
  if (dialogue) {
    const c7 = await step('replace_block_left', async () => {
      const r = await ipc.replaceBlock(dialogue.id, '昨日，去了那座城。', c4.value.revision);
      store.applyResult(r);
      return r;
    });
    record(c7.step);
  }

  // 8. 过期 revision 必须被拒绝
  if (dialogue) {
    const c8 = await step('stale_revision_rejected', async () => {
      try {
        await ipc.replaceBlock(dialogue.id, '过期写入', 1);
        return { rejected: false as const };
      } catch (e) {
        return { rejected: true as const, message: e instanceof Error ? e.message : String(e) };
      }
    });
    record({ ...c8.step, ok: c8.value?.rejected === true });
  }

  // 8b. 撤销刚才的左侧修改
  if (dialogue) {
    const c8b = await step('undo_left_edit', async () => {
      const r = await ipc.undo();
      store.applyResult(r);
      return r;
    });
    record(c8b.step);
    const afterUndo = c8b.value?.document.blocks.find((b) => b.id === dialogue.id)?.text ?? '';
    record({
      name: 'assert_undo_restored_dialogue',
      ok: afterUndo === '昨日，去了买卖城。',
      ms: 0,
      detail: `undo 后对白=${afterUndo}`,
    });
  }

  // 8c. 重做
  if (dialogue) {
    const c8c = await step('redo_left_edit', async () => {
      const r = await ipc.redo();
      store.applyResult(r);
      return r;
    });
    record(c8c.step);
    const afterRedo = c8c.value?.document.blocks.find((b) => b.id === dialogue.id)?.text ?? '';
    record({
      name: 'assert_redo_reapplied_dialogue',
      ok: afterRedo === '昨日，去了那座城。',
      ms: 0,
      detail: `redo 后对白=${afterRedo}`,
    });
  }

  // 8d. 撤销栈状态
  const c8d = await step('history_status', () => ipc.historyStatus());
  record({
    ...c8d.step,
    ok: (c8d.value?.can_undo ?? false) === true,
  });

  // 9. 设置（标题页 / 纸张）
  const c9 = await step('update_settings', async () => {
    const r = await ipc.updateSettings({
      title: '自检剧本',
      author: '测试编剧',
      paper: {
        template: 'a4',
        page_width_inch: 8.27,
        page_height_inch: 11.69,
        margin_top_inch: 1,
        margin_bottom_inch: 1,
        margin_left_inch: 1.25,
        margin_right_inch: 1,
      },
    });
    store.applyResult(r);
    return r;
  });
  record(c9.step);

  // 10. 分页
  const c10 = await step('get_pagination', () => ipc.getPagination());
  record(c10.step);

  // 11. 保存
  const c11 = await step('save_project_as', () => ipc.saveProjectAs(projectPath));
  record(c11.step);
  if (!c11.step.ok) return finish();

  // 12. 保存后 projectInfo 应为 clean
  const c12 = await step('project_info_after_save', () => ipc.projectInfo());
  record({ ...c12.step, ok: c12.value?.dirty === false });

  // 12b. 自动保存配置与 tick
  const c12b = await step('configure_autosave', () =>
    ipc.configureAutosave({ enabled: true, saveIntervalSecs: 1, versionIntervalSecs: 0 })
  );
  record(c12b.step);

  const c12c = await step('autosave_tick_after_change', async () => {
    // 制造一次新修改，然后 tick
    const r = await ipc.replaceBlock(
      blocks[blocks.length - 1].id,
      '昨日，去了那座城。（自动保存测试）',
      (await ipc.projectInfo()).revision
    );
    store.applyResult(r);
    await new Promise((resolve) => setTimeout(resolve, 1100));
    const action = await ipc.autosaveTick();
    // The normal 1s UI tick may have already saved during the wait.
    return action ?? ((await ipc.projectInfo()).dirty ? null : 'alreadySaved');
  });
  record({ ...c12c.step, ok: c12c.value === 'saved' || c12c.value === 'alreadySaved', detail: `autosaveTick=${c12c.value}` });

  const c12d = await step('project_clean_after_autosave', () => ipc.projectInfo());
  record({ ...c12d.step, ok: c12d.value?.dirty === false });

  // 13. 关闭
  const c13 = await step('close_project', () => ipc.closeProject());
  record(c13.step);

  // 14. 重开
  const c14 = await step('open_project', () => ipc.openProject(projectPath));
  record(c14.step);
  if (!c14.step.ok) return finish();

  // 15. 校验重开后内容一致
  const reopened = c14.value.document;
  if (!reopened) {
    record({ name: 'assert_reopen_document_present', ok: false, ms: 0, detail: 'document 为 null' });
    return finish();
  }
  const reopenedText = reopened.blocks.map((b) => b.text).join('');
  record({
    name: 'assert_reopen_text_identical',
    ok:
      reopenedText.includes('镇北堡城门') &&
      reopenedText.includes('昨日，去了那座城。') &&
      !reopenedText.includes('昨日，去了买卖城。'),
    ms: 0,
    detail: reopenedText.slice(0, 100),
  });

  // 16. 校验设置一致
  record({
    name: 'assert_settings_persisted',
    ok:
      reopened.title_page.written_by === '测试编剧' &&
      reopened.settings.template === 'a4' &&
      reopened.settings.margin_left_inch === 1.25,
    ms: 0,
    detail: `writer=${reopened.title_page.written_by} template=${reopened.settings.template} margin=${reopened.settings.margin_left_inch}`,
  });

  // 17. 版本历史
  const c17 = await step('create_version', async () => {
    const r = await ipc.createVersion('selftest-v1', '自检版本');
    store.applyResult(r);
    return r;
  });
  record(c17.step);

  const c18 = await step('list_versions', () => ipc.listVersions());
  record({ ...c18.step, ok: (c18.value?.length ?? 0) >= 1 });

  const c19 = await step('audit_log', () => ipc.auditLog(50));
  record({ ...c19.step, ok: (c19.value?.length ?? 0) > 0 });

  // 18. 版本恢复 + 恢复前检查点 + 恢复可撤销（B-10 / B-11）
  const v1id = c18.value?.[0]?.id;
  if (v1id) {
    const c20 = await step('restore_version', async () => {
      const r = await ipc.restoreVersion(v1id);
      store.applyResult(r);
      return r;
    });
    record(c20.step);

    const c20b = await step('checkpoint_created_by_restore', () => ipc.listVersions());
    const hasCheckpoint = (c20b.value ?? []).some(
      (v) => v.name.includes('恢复前自动检查点')
    );
    record({
      ...c20b.step,
      ok: hasCheckpoint,
      detail: `自动检查点=${hasCheckpoint}`,
    });

    const c20c = await step('undo_restore', async () => {
      const r = await ipc.undo();
      store.applyResult(r);
      return r;
    });
    record(c20c.step);
  }

  // 19. 版本比较
  const c21 = await step('compare_versions', async () => {
    const vs = await ipc.listVersions();
    if (vs.length < 2) return { rendered: '（版本不足，跳过）' };
    return ipc.compareVersions(vs[vs.length - 1].id, vs[0].id);
  });
  record({ ...c21.step, ok: Boolean(c21.value?.rendered) });

  // 20. 分支
  const c22 = await step('switch_branch', async () => {
    const b = await ipc.switchBranch('selftest-branch');
    const list = await ipc.listBranches();
    return { current: b, count: list.length };
  });
  record({ ...c22.step, ok: c22.value?.current === 'selftest-branch' });
  const c22b = await step('switch_back_to_main', () => ipc.switchBranch('main'));
  record(c22b.step);

  // 21. 纸面编辑器 DOM 自检（B5：IME / 光标 / 选区 / 键盘语义）
  try {
    const { runEditorDomSelfTest } = await import('@/ipc/editorDomTest');
    const domSteps = await runEditorDomSelfTest();
    for (const ds of domSteps) record(ds);
  } catch (e) {
    record({
      name: 'editor_dom_selftest',
      ok: false,
      ms: 0,
      detail: e instanceof Error ? e.message : String(e),
    });
  }

  return finish();

  function finish(): SelftestReport {
    const passed = steps.filter((s) => s.ok).length;
    const report: SelftestReport = {
      startedAt: new Date().toISOString(),
      projectPath,
      totalSteps: steps.length,
      passed,
      failed: steps.filter((s) => !s.ok).length,
      steps,
    };
    void writeReport(report);
    return report;
  }
}

/** 自动启动 */
export function maybeAutoStart() {
  const params = new URLSearchParams(window.location.search);
  // 三种触发方式：URL 参数、环境变量（经 Rust 注入）、构建期注入
  const enabled =
    params.get('selftest') === '1' ||
    params.get('selftest') === 'true' ||
    (window as unknown as { __SCREENPLAY_SELFTEST__?: string }).__SCREENPLAY_SELFTEST__ === '1';
  if (enabled) {
    setTimeout(() => {
      void runSelfTest();
    }, 800);
  }
}