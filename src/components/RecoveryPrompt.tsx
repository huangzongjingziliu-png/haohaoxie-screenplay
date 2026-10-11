import ModalBackdrop from './ModalBackdrop';
import {tr,fmt} from '@/i18n';
import { useEffect, useState } from 'react';
import { ipc, type PendingRecovery } from '@/ipc/client';
import { useEditorStore } from '@/stores/editor';

/**
 * 崩溃恢复提示
 *
 * 应用启动时如果检测到上次异常退出，会弹出此对话框，
 * 让用户选择「恢复」还是「丢弃」。
 *
 * 这是数据安全的关键一环：用户可能因为崩溃丢失了数小时的工作。
 */
export default function RecoveryPrompt() {
  const [pending, setPending] = useState<PendingRecovery | null>(null);
  const [busy, setBusy] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const bootstrap = useEditorStore((s) => s.bootstrap);
  const pushToast = useEditorStore((s) => s.pushToast);

  useEffect(() => {
    let cancelled = false;
    void ipc
      .pendingRecovery()
      .then((p) => {
        if (!cancelled) setPending(p);
      })
      .catch(() => {
        /* 查询失败不阻塞启动 */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!pending || dismissed) return null;

  const when = new Date(pending.savedAt);
  const timeStr = Number.isNaN(when.getTime())
    ? pending.savedAt
    : when.toLocaleString('zh-CN');

  const adopt = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await ipc.adoptRecovery();
      await bootstrap();
      setDismissed(true);
      pushToast('success', fmt("已恢复 {0} 个元素的未保存内容",pending.blockCount));
    } catch {
      pushToast('error', tr("恢复失败，请重试"));
    } finally { setBusy(false); }
  };

  const discard = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await ipc.discardRecovery();
      setDismissed(true);
      pushToast('info', tr("已丢弃崩溃前的临时内容"));
    } catch {
      pushToast('error', tr("操作失败"));
    } finally { setBusy(false); }
  };

  return (
    <ModalBackdrop className="modal-backdrop recovery-backdrop" onDismiss={()=>setDismissed(true)} disabled={busy}>
      <div className="modal recovery-modal" role="dialog" aria-modal="true" aria-labelledby="recovery-title">
        <div className="recovery-icon" aria-hidden>
          ⟲
        </div>
        <h2 id="recovery-title">{tr("发现未保存的内容")}</h2>
        <p className="modal-hint">{tr(" 好好写剧本上次退出时没有正常关闭，以下内容已自动保存： ")}</p>

        <div className="recovery-info">
          <div className="recovery-row">
            <span className="recovery-label">{tr("剧本")}</span>
            <span>{pending.title || tr("未命名剧本")}</span>
          </div>
          <div className="recovery-row">
            <span className="recovery-label">{tr("元素数")}</span>
            <span>{pending.blockCount}</span>
          </div>
          <div className="recovery-row">
            <span className="recovery-label">{tr("修订号")}</span>
            <span>rev {pending.revision}</span>
          </div>
          <div className="recovery-row">
            <span className="recovery-label">{tr("保存时间")}</span>
            <span>{timeStr}</span>
          </div>
          {pending.preview && (
            <div className="recovery-preview">
              <span className="recovery-label">{tr("内容预览")}</span>
              <p>{pending.preview}</p>
            </div>
          )}
        </div>

        <div className="modal-actions">
          <button disabled={busy} onClick={()=>setDismissed(true)}>{tr("取消")}</button>
          <button disabled={busy} onClick={() => void discard()}>{tr("丢弃")}</button>
          <button disabled={busy} className="primary" onClick={() => void adopt()}>{tr(" 恢复这些内容 ")}</button>
        </div>
      </div>
    </ModalBackdrop>
  );
}