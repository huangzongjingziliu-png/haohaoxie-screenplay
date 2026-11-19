import { useEffect } from 'react';
import ModalBackdrop from './ModalBackdrop';
import { STORE_BUILD } from '@/buildChannel';
import { useEntitlementStore, usePro } from '@/stores/entitlement';
import { useEditorStore } from '@/stores/editor';
import { tr } from '@/i18n';

/** 1.0 专业版权益（与 docs/app-store/free-vs-pro-boundary.md 一致） */
const FEATURES = [
  'PDF 导出不带页脚水印',
  'Fountain / Final Draft (FDX) 导出',
  '分集批量导出与合并 PDF',
  '命名稿号与版本对比导出',
  '导出文件名与封面携带稿号',
];

/**
 * 专业版购买面板。
 *
 * 只在商店构建、且被显式打开时渲染；直售构建直接返回 null。
 * 价签来自 `store_products`（StoreKit 返回），不在前端写死金额。
 */
export default function ProPurchase() {
  const open = useEntitlementStore((s) => s.dialogOpen);
  const close = useEntitlementStore((s) => s.closeDialog);
  const status = useEntitlementStore((s) => s.status);
  const products = useEntitlementStore((s) => s.products);
  const busy = useEntitlementStore((s) => s.busy);
  const error = useEntitlementStore((s) => s.error);
  const refresh = useEntitlementStore((s) => s.refresh);
  const purchase = useEntitlementStore((s) => s.purchase);
  const restore = useEntitlementStore((s) => s.restore);
  const pushToast = useEditorStore((s) => s.pushToast);
  const pro = usePro();

  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  if (!STORE_BUILD || !open) return null;

  const product = products[0];
  const buy = async () => {
    const result = await purchase();
    if (result === 'purchased') pushToast('success', tr('已解锁专业版'));
    else if (result === 'cancelled') pushToast('info', tr('已取消购买'));
    else if (result === 'pending') pushToast('info', tr('购买等待确认，完成后会自动解锁'));
  };
  const restorePurchase = async () => {
    const unlocked = await restore();
    pushToast(unlocked ? 'success' : 'info', unlocked ? tr('已恢复专业版') : tr('没有可恢复的购买'));
  };

  return (
    <ModalBackdrop disabled={busy} onDismiss={close} dirty={false}>
      <div className="modal pro-purchase" role="dialog" aria-modal="true" aria-label={tr('专业版')} onClick={(e) => e.stopPropagation()}>
        <h2>{tr('专业版')}</h2>
        <p className="modal-hint">{tr('一次买断，解锁桌面版的交付能力。免费版可以完整写作、保存与导出 PDF / Word / Markdown / 纯文本 / HTML。')}</p>
        <ul className="pro-features">
          {FEATURES.map((f) => <li key={f}>{tr(f)}</li>)}
        </ul>
        {pro ? (
          <p className="pro-owned" role="status">{tr('已购买 · 专业版已解锁')}</p>
        ) : (
          <p className="pro-price">
            <strong>{product?.displayPrice ?? '—'}</strong>
            <small>{product?.displayName || tr('专业版 · 非消耗型买断')}</small>
          </p>
        )}
        {error && <p className="pro-error" role="alert">{error}</p>}
        <div className="modal-actions">
          {!pro && <button className="primary" disabled={busy} onClick={() => void buy()}>{tr('购买专业版')}</button>}
          <button disabled={busy} onClick={() => void restorePurchase()}>{tr('恢复购买')}</button>
          <button disabled={busy} onClick={close}>{tr('关闭')}</button>
        </div>
      </div>
    </ModalBackdrop>
  );
}
