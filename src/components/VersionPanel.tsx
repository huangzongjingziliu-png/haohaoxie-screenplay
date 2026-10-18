import RevisionDiffView from './RevisionDiffView';
import {flushPendingEdits} from '@/utils/pendingEdits';
import type {RevisionComparison} from '@/ipc/types/RevisionComparison';
import ModalBackdrop from './ModalBackdrop';
import {tr,fmt} from '@/i18n';
import { useCallback, useEffect, useRef, useState } from 'react';
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
  const [selected,setSelected]=useState<VersionRecord|null>(null);
  const [preview,setPreview]=useState('');
  const [previewBusy,setPreviewBusy]=useState(false);
  const [filter,setFilter]=useState('all');
  const previewRequest=useRef(0);
  const projectId=useEditorStore(state=>state.document?.id);
  const boundProject=useRef(projectId);
  useEffect(()=>{if(projectId!==boundProject.current)onClose();},[projectId,onClose]);
  useEffect(()=>()=>{previewRequest.current++;},[]);
  const inspect=async(version:VersionRecord)=>{
    const request=++previewRequest.current;setSelected(version);setPreview('');setPreviewBusy(true);
    try{const text=await ipc.previewVersion(version.id);if(request===previewRequest.current&&useEditorStore.getState().document?.id===boundProject.current)setPreview(text);}
    catch(error){if(request===previewRequest.current)pushToast('error',String(error));}
    finally{if(request===previewRequest.current)setPreviewBusy(false);}
  };

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
      if(useEditorStore.getState().document?.id!==boundProject.current)throw new Error(tr('项目已切换，请重新打开历史记录'));
      const r = await ipc.createVersion(name, newNote.trim(),boundProject.current);
      if(useEditorStore.getState().document?.id!==boundProject.current)return;
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
        fmt('恢复到「{0}」？当前稿会先保留一个恢复前检查点，恢复后也可以撤销。',v.name)
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      await flushPendingEdits();
      if(useEditorStore.getState().document?.id!==boundProject.current)throw new Error(tr('项目已切换，请重新打开历史记录'));
      const r = await ipc.restoreVersion(v.id,boundProject.current);
      if(useEditorStore.getState().document?.id!==boundProject.current)return;
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
      await flushPendingEdits();
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
    <ModalBackdrop disabled={busy} onDismiss={onClose} message={tr("放弃尚未提交的版本操作并关闭？")}>
      <div className="modal version-modal" role="dialog" aria-modal="true" aria-label={tr('历史记录')} onClick={(e) => e.stopPropagation()}>
        <div className="version-header">
          <h2>{tr("历史记录")}</h2><p>{tr('每次改稿都有来处。先预览旧稿，再决定是否恢复。')}</p><p>{tr("当前稿号")} · {draftLabel||tr("尚未命名")}</p>
          <p className="modal-hint">{tr('自动记录默认每 5 分钟保留一个有修改的版本；重要改稿前可手动留存。恢复会影响整个工程，包括分集与白板。')}</p>
          <div className="version-branch-badge" title={tr("当前分支")}>{tr(" 分支：")}{currentBranch}
          </div>
        </div>

        <div className="version-tabs">
          {(
            [
              ['versions', fmt("版本 ({0})",versions.length)],
              ['branches', fmt("分支 ({0})",branches.length)],
              ['audit', tr("操作记录")],
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

            <div className="model-filter-row">{[['all',tr('全部')],['manual',tr('手动留存')],['auto',tr('自动记录')]].map(([value,label])=><button key={value} aria-pressed={filter===value} disabled={busy} onClick={()=>setFilter(value)}>{label}</button>)}</div>
            {selected&&<section className="history-preview" aria-label={tr('旧稿预览')}><header><strong>{selected.name}</strong><span>{new Date(selected.created_at).toLocaleString()}</span><button disabled={busy||previewBusy} onClick={()=>void restore(selected)}>{tr('恢复到这一稿')}</button></header><p>{tr('这是只读预览，当前剧本还没有改变。')}</p><pre>{previewBusy?tr('正在读取旧稿…'):preview}</pre></section>}
            <div className="version-list history-timeline">
              {versions.length === 0 && (
                <p className="empty-hint">{tr("还没有版本。编辑剧本后可以创建第一个版本。")}</p>
              )}
              {versions.filter(version=>filter==='all'||(filter==='auto'?version.auto:!version.auto)).slice().sort((left,right)=>right.created_at.localeCompare(left.created_at)).map((v) => (
                <div key={v.id} className={'version-item '+(selected?.id===v.id?'history-selected':'')}>
                  <div className="version-item-main">
                    <span className="version-name">
                      {v.name} · {new Date(v.created_at).toLocaleString()}
                      {v.auto && <span className="version-auto">{tr("自动")}</span>}
                    </span>
                    <span className="version-meta">{v.auto?tr('自动记录'):tr('手动留存')} · rev {v.revision}</span>
                    {v.note && <span className="version-note">{v.note}</span>}
                  </div>
                  <button disabled={busy} onClick={() => void inspect(v)}>{tr('预览旧稿')}</button><button disabled={busy} onClick={()=>{setCompareA(v.id);setCompareB('');void flushPendingEdits().then(()=>ipc.compareProjectVersions(v.id)).then(result=>{if(useEditorStore.getState().document?.id===boundProject.current)setComparison(result);}).catch(error=>pushToast('error',String(error)));}}>{tr('与当前稿比较')}</button>
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
