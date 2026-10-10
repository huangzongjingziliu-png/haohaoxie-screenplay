/** Await editor drafts before save/export/history operations. No document copy lives here. */
const editors = new Set<() => Promise<void>>();
export function registerPendingEditor(flush: () => Promise<void>): () => void {
  editors.add(flush);
  return () => { editors.delete(flush); };
}
export async function flushPendingEdits(): Promise<void> {
  for (const flush of [...editors]) await flush();
}
