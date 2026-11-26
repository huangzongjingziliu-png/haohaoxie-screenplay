/** Await editor drafts before save/export/history operations. No document copy lives here. */
const editors = new Set<() => Promise<void>>();
export function registerPendingEditor(flush: () => Promise<void>): () => void {
  editors.add(flush);
  return () => { editors.delete(flush); };
}
export async function flushPendingEdits(): Promise<void> {
  for (const flush of [...editors]) await flush();
}

/**
 * 界面里已经输入、但还没提交到后端的草稿。
 *
 * 用于状态栏区分「输入未提交」与「已提交、等待写盘」：
 * 后端 revision/dirty 看不到前者，只有编辑器自己知道。
 */
const drafts = new Set<string>();
const draftListeners = new Set<() => void>();
function notifyDrafts() {
  for (const listener of [...draftListeners]) listener();
}
export function setDraftPending(key: string, pending: boolean): void {
  const had = drafts.has(key);
  if (pending) drafts.add(key); else drafts.delete(key);
  if (had !== pending) notifyDrafts();
}
export function hasPendingDraft(): boolean {
  return drafts.size > 0;
}
export function subscribeDrafts(listener: () => void): () => void {
  draftListeners.add(listener);
  return () => { draftListeners.delete(listener); };
}
