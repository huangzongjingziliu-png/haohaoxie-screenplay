import {tr,fmt} from '@/i18n';
import ModelPicker from './ModelPicker';
import { useEffect, useState, useSyncExternalStore } from 'react';
import { useEditorStore } from '@/stores/editor';
import { ipc, ScreenplayError } from '@/ipc/client';
import { hasPendingDraft, subscribeDrafts } from '@/utils/pendingEdits';
import type { DocumentStats } from '@/ipc/types';

/** 保存状态三态：输入未提交 / 等待写盘 / 已写盘（另有保存失败） */
export function saveStateLabel(opts: {
  error: boolean;
  pendingDraft: boolean;
  hasPath: boolean;
  dirty: boolean;
  saving: boolean;
}): { text: string; hint: string } {
  if (opts.error) return { text: tr('保存失败 · 请重试'), hint: tr('上一次保存没有成功，请重试') };
  if (opts.pendingDraft) {
    return { text: tr('✎ 输入未提交'), hint: tr('正在输入，尚未提交到文档；稍候会自动提交') };
  }
  if (!opts.hasPath) return { text: tr('请先保存到文件'), hint: tr('这个项目还没有保存到磁盘') };
  if (opts.dirty || opts.saving) {
    return { text: tr('● 等待写盘'), hint: tr('修改已提交到文档，正在等待写入磁盘') };
  }
  return { text: tr('○ 已写盘'), hint: tr('所有修改都已写入磁盘') };
}

/** 状态栏：统计来自 Rust `DocumentStats`（与导出口径一致） */
export default function StatusBar() {
  const [saveStatus, setSaveStatus] = useState({ state: 'idle', message: '' });
  useEffect(() => {
    const listener = (event: Event) => setSaveStatus((event as CustomEvent).detail);
    window.addEventListener('save-status', listener);
    return () => window.removeEventListener('save-status', listener);
  }, []);
  // 草稿状态只在编辑器内部可见，单独订阅，避免每秒轮询
  const pendingDraft = useSyncExternalStore(subscribeDrafts, hasPendingDraft, hasPendingDraft);
  const project = useEditorStore((s) => s.project);
  const [stats, setStats] = useState<DocumentStats | null>(null);
  const doc = useEditorStore((s) => s.document);

  useEffect(() => {
    if (!doc) {
      setStats(null);
      return;
    }
    let cancelled = false;
    void ipc
      .documentStats()
      .then((s) => {
        if (!cancelled) setStats(s);
      })
      .catch(() => setStats(null));
    return () => {
      cancelled = true;
    };
  }, [doc]);

  const lowConfidence = (doc?.blocks ?? []).filter(
    (b) => b.confidence > 0 && b.confidence < 0.75
  ).length;

  const state = saveStateLabel({
    error: saveStatus.state === 'error',
    pendingDraft,
    hasPath: !!project.path,
    dirty: project.dirty,
    saving: saveStatus.state === 'saving',
  });

  return (
    <div className="status-bar">
      <span title={saveStatus.state === 'error' && saveStatus.message ? saveStatus.message : state.hint} role="status" className={saveStatus.state === 'error' || project.dirty || pendingDraft ? 'dirty' : 'clean'}>
        {state.text}
      </span>
      <span>rev {project.revision}</span>
      {stats && (
        <>
          <span>{stats.pages}{tr(" 页")}</span>
          <span>{stats.scenes}{tr(" 场")}</span>
          <span>{stats.characters}{tr(" 人物")}</span>
          <span>{stats.words}{tr(" 字")}</span>
          <span>{tr("约 ")}{stats.estimated_minutes}{tr(" 分钟")}</span>
        </>
      )}
      {doc?.import_info && (lowConfidence > 0 || doc.import_info.removed_noise.length > 0) && (
        <button className="warn status-review-button" title={tr("查看识别结果、原文和去噪记录")} onClick={() => window.dispatchEvent(new Event('import-review-requested'))}>
          {lowConfidence > 0 ? fmt("⚠ {0} 处待确认 · 校对",lowConfidence) : tr("查看导入记录")}
        </button>
      )}
      <span className="spacer" />
      <ModelPicker />
      <span className="path" title={project.path ?? tr("未保存到磁盘")}>
        {project.path ?? tr("未保存")}
      </span>
    </div>
  );
}