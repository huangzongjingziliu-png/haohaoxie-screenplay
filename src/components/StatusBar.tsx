import {tr,fmt} from '@/i18n';
import ModelPicker from './ModelPicker';
import { useEffect, useState } from 'react';
import { useEditorStore } from '@/stores/editor';
import { ipc, ScreenplayError } from '@/ipc/client';
import type { DocumentStats } from '@/ipc/types';

/** 状态栏：统计来自 Rust `DocumentStats`（与导出口径一致） */
export default function StatusBar() {
  const [saveStatus, setSaveStatus] = useState({ state: 'idle', message: '' });
  useEffect(() => {
    const listener = (event: Event) => setSaveStatus((event as CustomEvent).detail);
    window.addEventListener('save-status', listener);
    return () => window.removeEventListener('save-status', listener);
  }, []);
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

  return (
    <div className="status-bar">
      <span title={saveStatus.message} role="status" className={saveStatus.state === 'error' || project.dirty ? 'dirty' : 'clean'}>
        {saveStatus.state === 'error' ? tr("保存失败 · 请重试") : saveStatus.state === 'saving' ? tr("正在检查保存…") : !project.path ? tr("请先保存到文件") : project.dirty ? tr("● 未保存") : tr("○ 已保存")}
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