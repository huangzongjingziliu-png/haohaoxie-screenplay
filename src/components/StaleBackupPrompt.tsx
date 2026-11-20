import ModalBackdrop from './ModalBackdrop';
import {tr,fmt} from '@/i18n';
import {useEffect,useState} from 'react';
import {ipc, type StaleSaveBackup} from '@/ipc/client';
import {useEditorStore} from '@/stores/editor';

/**
 * 残留保存备份提示
 *
 * 覆盖保存时进程被强杀，磁盘上的工程可能只剩半截，
 * 完整内容还留在系统临时目录的备份里。
 *
 * 铁律：只在用户明确点「恢复」后才写回工程文件；
 * 选择「保留备份」时只让备份不再参与启动扫描，绝不删除数据。
 */
export default function StaleBackupPrompt() {
  const [found, setFound] = useState<StaleSaveBackup | null>(null);
  const [hidden, setHidden] = useState(false);
  const [busy, setBusy] = useState(false);
  const bootstrap = useEditorStore((s) => s.bootstrap);
  const pushToast = useEditorStore((s) => s.pushToast);

  useEffect(() => {
    let cancelled = false;
    void ipc
      .staleSaveBackup()
      .then((item) => {
        if (!cancelled) setFound(item);
      })
      .catch(() => {
        /* 查询失败不阻塞启动 */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!found || hidden) return null;

  const name = found.container.split('/').pop() || found.container;
  const when = new Date(found.savedAt);
  const timeStr = Number.isNaN(when.getTime()) ? found.savedAt : when.toLocaleString('zh-CN');

  const restore = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await ipc.restoreStaleSaveBackup(found.container);
      await bootstrap();
      setHidden(true);
      pushToast('success', fmt("已用备份恢复工程「{0}」，可在「最近打开」中重新打开", name));
    } catch (e) {
      pushToast('error', (e as Error).message || tr("恢复失败，请重试"));
    } finally { setBusy(false); }
  };

  const keep = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await ipc.dismissStaleSaveBackup(found.container);
      setHidden(true);
      pushToast('info', tr("备份已保留在系统临时目录，之后不再提示"));
    } catch (e) {
      pushToast('error', (e as Error).message || tr("操作失败"));
    } finally { setBusy(false); }
  };

  return (
    <ModalBackdrop className="modal-backdrop recovery-backdrop" onDismiss={()=>setHidden(true)} disabled={busy}>
      <div className="modal recovery-modal" role="dialog" aria-modal="true" aria-labelledby="stale-backup-title">
        <div className="recovery-icon" aria-hidden>
          ⟲
        </div>
        <h2 id="stale-backup-title">{tr("发现未完成的保存备份")}</h2>
        <p className="modal-hint">{tr(" 上次保存进行到一半时进程被强制结束，现有工程文件可能不完整。下面这份备份是保存前的完整内容，只有你确认后才会写回。 ")}</p>

        <div className="recovery-info">
          <div className="recovery-row">
            <span className="recovery-label">{tr("工程")}</span>
            <span>{name}</span>
          </div>
          <div className="recovery-row">
            <span className="recovery-label">{tr("备份时间")}</span>
            <span>{timeStr}</span>
          </div>
          <div className="recovery-row">
            <span className="recovery-label">{tr("备份大小")}</span>
            <span>{formatSize(found.size)}</span>
          </div>
        </div>

        <div className="modal-actions">
          <button disabled={busy} onClick={() => void keep()}>{tr("保留备份，不再提示")}</button>
          <button disabled={busy} className="primary" onClick={() => void restore()}>{tr(" 用备份恢复 ")}</button>
        </div>
      </div>
    </ModalBackdrop>
  );
}

function formatSize(bytes:number):string{
  if (bytes>=1024*1024) return `${(bytes/1024/1024).toFixed(1)} MB`;
  if (bytes>=1024) return `${Math.round(bytes/1024)} KB`;
  return `${bytes} B`;
}
