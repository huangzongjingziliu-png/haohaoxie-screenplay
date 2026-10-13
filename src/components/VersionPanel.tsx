import RevisionDiffView from './RevisionDiffView';
import {flushPendingEdits} from '@/utils/pendingEdits';
import type {RevisionComparison} from '@/ipc/types/RevisionComparison';
import ModalBackdrop from './ModalBackdrop';
import {tr,fmt} from '@/i18n';
import { useCallback, useEffect, useState } from 'react';
import { ipc, ScreenplayError } from '@/ipc/client';
import { useEditorStore } from '@/stores/editor';
import type { VersionRecord, AuditEntry, Branch } from '@/ipc/types';

/**
 * 版本历史面板
 *
 * 覆盖说明书 B-12：版本历史、命名、比较、恢复、分支。
 *
 * 关键安全设计：
 * - 恢复版本**前**后端会自动创建检查点（保住用户当前内容）
 * - 恢复本身可撤销
 * - 恢复后自动刷新撤销栈状态
 */
export default function VersionPanel({ onClose }: { onClose: () => void }) {
  const applyResult = useEditorStore((s) => s.applyResult);
  const bootstrap = useEditorStore((s) => s.bootstrap);
  const pushToast = useEditorStore((s) => s.pushToast);

  const [tab, setTab] = useState<'versions' | 'branches' | 'audit'>('versions');
  const [versions, setVersions] = useState<VersionRecord[]>([]);
  const [branches, setBranches] = useState<Branch[]>([]);
  const [currentBranch, setCurrentBranch] = useState('main');
  const [audit, setAudit] = useState<AuditEntry[]>([]);
  const [busy, setBusy] = useState(false);

  const [newName, setNewName] = useState('');
  const [newNote, setNewNote] = useState('');
  const [newBranch, setNewBranch] = useState('');
  const [compareA, setCompareA] = useState<string>('');
  const [compareB, setCompareB] = useState<string>('');
  const [diffText, setDiffText] = useState<string>('');
  const [comparison,setComparison]=useState<RevisionComparison|null>(null);
  const draftLabel=useEditorStore(s=>s.document?.draft_label);

  const reload = useCallback(async () => {
    try {
      const [v, b, cb] = await Promise.all([
        ipc.listVersions(),
        ipc.listBranches(),
        ipc.currentBranch(),
      ]);
      setVersions(v);
      setBranches(b);
      setCurrentBranch(cb);
      setAudit(await ipc.auditLog(50));
    } catch (e) {
      pushToast('error', e instanceof ScreenplayError ? e.message : String(e));
    }
  }, [pushToast]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const createVersion = async () => {
    const name = newName.trim();
    if (!name) {
      pushToast('error', tr("请输入版本名称"));
      return;
    }
    setBusy(true);
    try {
      await flushPendingEdits();
      const r = await ipc.createVersion(name, newNote.trim());
      await applyResult(r);
      setNewName('');
      setNewNote('');
      await reload();
      pushToast('success', fmt("已创建版本「{0}」",name));
    } catch (e) {
      pushToast('error', e instanceof ScreenplayError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const restore = async (v: VersionRecord) => {
    if (
      !window.confirm(
        `恢复到版本「${v.name}」？\n\n` +
          `当前内容会先自动保存为检查点，不会丢失。\n` +
          `恢复后还可以撤销。`
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      await flushPendingEdits();
      const r = await ipc.restoreVersion(v.id);
      await applyResult(r);
      await reload();
      pushToast('success', fmt("已恢复到「{0}」，可按 ⌘Z 撤销",v.name));
    } catch (e) {
      pushToast('error', e instanceof ScreenplayError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const compare = async () => {
    if (!compareA) {
      pushToast('error', tr("请选择要比较的第一个版本"));
      return;
    }
    setBusy(true);
    try {
      await flushPendingEdits();
      const d = await ipc.compareProjectVersions(compareA, compareB || undefined);
      setComparison(d);setDiffText('');
    } catch (e) {
      pushToast('error', e instanceof ScreenplayError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const switchBranch = async (name: string) => {
    setBusy(true);
    try {
      await ipc.switchBranch(name);
      await reload();
      await bootstrap();
      pushToast('success', fmt("已切换到分支「{0}」",name));
    } catch (e) {
      pushToast('error', e instanceof ScreenplayError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <ModalBackdrop onDismiss={onClose} message={tr("放弃尚未提交的版本操作并关闭？")}>
      <div className="modal version-modal" onClick={(e) => e.stopPropagation()}>
        <div className="version-header">
          <h2>{tr("版本与历史")}</h2><p>{tr("当前稿号")} · {draftLabel||tr("尚未命名")}</p>
          <div className="version-branch-badge" title={tr("当前分支")}>{tr(" 分支：")}{currentBranch}
          </div>
        </div>

        <div className="version-tabs">
          {(
            [
              ['versions', fmt("版本 ({0})",versions.length)],
              ['branches', fmt("分支 ({0})",branches.length)],
              ['audit', tr("审计日志")],
            ] as const
          ).map(([t, label]) => (
            <button
              key={t}
              className={tab === t ? 'active' : ''}
              onClick={() => setTab(t)}
            >
              {tr(label)}
            </button>
          ))}
        </div>

        {tab === 'versions' && (
          <>
            <div className="version-create">
              <input
                placeholder={tr("稿号（如 V2.1 或 第三稿）")}
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
              />
              <input
                placeholder={tr("备注（可选）")}
                value={newNote}
                onChange={(e) => setNewNote(e.target.value)}
              />
              <button disabled={busy} onClick={() => void createVersion()}>{tr(" 创建版本 ")}</button>
            </div>

            <div className="version-compare">
              <select value={compareA} onChange={(e) => setCompareA(e.target.value)}>
                <option value="">{tr("选择版本 A")}</option>
                {versions.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.name}（rev {v.revision}）
                  </option>
                ))}
              </select>
              <span className="vs">{tr("对比")}</span>
              <select value={compareB} onChange={(e) => setCompareB(e.target.value)}>
                <option value="">{tr("当前内容")}</option>
                {versions.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.name} · {new Date(v.created_at).toLocaleString()}
                  </option>
                ))}
              </select>
              <button disabled={busy || !compareA} onClick={() => void compare()}>{tr(" 比较 ")}</button>
            </div>

            {comparison&&<RevisionDiffView comparison={comparison}/>}
            {diffText && (
              <pre className="version-diff">{diffText}</pre>
            )}

            <div className="version-list">
              {versions.length === 0 && (
                <p className="empty-hint">{tr("还没有版本。编辑剧本后可以创建第一个版本。")}</p>
              )}
              {versions.map((v) => (
                <div key={v.id} className="version-item">
                  <div className="version-item-main">
                    <span className="version-name">
                      {v.name} · {new Date(v.created_at).toLocaleString()}
                      {v.auto && <span className="version-auto">{tr("自动")}</span>}
                    </span>
                    <span className="version-meta">
                      rev {v.revision} · {v.origin} · {new Date(v.created_at).toLocaleString('zh-CN')}
                    </span>
                    {v.note && <span className="version-note">{v.note}</span>}
                  </div>
                  <button disabled={busy} onClick={() => void restore(v)}>{tr(" 恢复 ")}</button>
                </div>
              ))}
            </div>
          </>
        )}

        {tab === 'branches' && (
          <>
            <div className="version-create">
              <input
                placeholder={tr("新分支名称")}
                value={newBranch}
                onChange={(e) => setNewBranch(e.target.value)}
              />
              <button
                disabled={busy || !newBranch.trim()}
                onClick={() => {
                  const n = newBranch.trim();
                  setNewBranch('');
                  void switchBranch(n);
                }}
              >{tr(" 创建并切换 ")}</button>
            </div>
            <div className="version-list">
              {branches.map((b) => (
                <div key={b.name} className="version-item">
                  <div className="version-item-main">
                    <span className="version-name">
                      {b.name}
                      {b.name === currentBranch && (
                        <span className="version-current">{tr("当前")}</span>
                      )}
                    </span>
                    <span className="version-meta">{tr(" 创建于 ")}{new Date(b.created_at).toLocaleString('zh-CN')}
                      {b.parent_branch && fmt(" · 源自 {0}",b.parent_branch)}
                    </span>
                  </div>
                  {b.name !== currentBranch && (
                    <button disabled={busy} onClick={() => void switchBranch(b.name)}>{tr(" 切换 ")}</button>
                  )}
                </div>
              ))}
            </div>
          </>
        )}

        {tab === 'audit' && (
          <div className="version-list audit-list">
            {audit.length === 0 && <p className="empty-hint">{tr("暂无记录")}</p>}
            {audit.map((a) => (
              <div key={a.id} className="audit-item">
                <span className={`audit-origin origin-${a.origin}`}>{a.origin}</span>
                <span className="audit-command">{a.command}</span>
                <span className="audit-target" title={a.target}>
                  {a.target.slice(0, 24)}
                </span>
                <span className="audit-rev">rev {a.revision}</span>
                <span className="audit-at">
                  {new Date(a.at).toLocaleTimeString('zh-CN')}
                </span>
              </div>
            ))}
          </div>
        )}

        <div className="modal-actions">
          <button onClick={onClose}>{tr("关闭")}</button>
        </div>
      </div>
    </ModalBackdrop>
  );
}