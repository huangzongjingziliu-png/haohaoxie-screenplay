import { useEffect } from 'react';
import { ipc } from '@/ipc/client';
import { useEditorStore } from '@/stores/editor';
import { flushPendingEdits } from '@/utils/pendingEdits';

export function publishSaveStatus(state: 'idle' | 'saving' | 'error', message = '') {
  window.dispatchEvent(new CustomEvent('save-status', { detail: { state, message } }));
}

export function useAutoSave() {
  const project = useEditorStore((s) => s.project);
  useEffect(() => {
    if (project.state === 'empty') return;
    let busy = false, disposed = false, composing = false;
    const start = () => { composing = true; };
    const end = () => { composing = false; };
    window.addEventListener('compositionstart', start, true);
    window.addEventListener('compositionend', end, true);
    publishSaveStatus('idle');
    const timer = window.setInterval(() => {
      if (busy || composing) return;
      busy = true;
      void (async () => {
        try {
          // Flush the visible draft first; never force a composing IME draft to commit.
          await flushPendingEdits();
          if (disposed) return;
          if (useEditorStore.getState().project.dirty) publishSaveStatus('saving');
          const action = await ipc.autosaveTick();
          if (disposed) return;
          if (action) {
            await useEditorStore.getState().refreshProjectInfo();
            await useEditorStore.getState().refreshHistory();
          }
          publishSaveStatus('idle');
        } catch (e) {
          if (!disposed) publishSaveStatus('error', String(e));
        } finally { busy = false; }
      })();
    }, 1000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
      window.removeEventListener('compositionstart', start, true);
      window.removeEventListener('compositionend', end, true);
    };
  }, [project.state, project.path]);
}

export async function configureAutoSave(opts: { enabled: boolean; saveIntervalSecs: number; versionIntervalSecs: number }) {
  return ipc.configureAutosave(opts);
}
