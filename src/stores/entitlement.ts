/**
 * 专业版权益 Store（仅商店渠道有意义）。
 *
 * 规则：
 * - 权威状态在 Rust（StoreKit），前端只缓存查询结果；
 * - 直售 / GitHub 渠道 `STORE_BUILD` 为 false，`usePro()` 恒为已解锁，
 *   不渲染任何付费墙；
 * - 价格与商品信息来自 `store_products`，前端不写死金额。
 */
import { create } from 'zustand';
import { STORE_BUILD } from '@/buildChannel';
import { ipc, type EntitlementStatus, type StoreProduct } from '@/ipc/client';

interface EntitlementState {
  status: EntitlementStatus | null;
  products: StoreProduct[];
  busy: boolean;
  error: string | null;
  dialogOpen: boolean;
  openDialog: () => void;
  closeDialog: () => void;
  refresh: () => Promise<void>;
  purchase: () => Promise<'purchased' | 'cancelled' | 'pending' | null>;
  restore: () => Promise<boolean>;
}

export const useEntitlementStore = create<EntitlementState>((set) => ({
  status: null,
  products: [],
  busy: false,
  error: null,
  dialogOpen: false,
  openDialog: () => {
    if (STORE_BUILD) set({ dialogOpen: true, error: null });
  },
  closeDialog: () => set({ dialogOpen: false }),
  refresh: async () => {
    if (!STORE_BUILD) return;
    try {
      const [status, products] = await Promise.all([ipc.entitlementStatus(), ipc.storeProducts()]);
      set({ status, products, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },
  purchase: async () => {
    set({ busy: true, error: null });
    try {
      const outcome = await ipc.storePurchase();
      set({ status: outcome.entitlement });
      return outcome.status;
    } catch (e) {
      set({ error: String(e) });
      return null;
    } finally {
      set({ busy: false });
    }
  },
  restore: async () => {
    set({ busy: true, error: null });
    try {
      const status = await ipc.storeRestore();
      set({ status });
      return status.pro;
    } catch (e) {
      set({ error: String(e) });
      return false;
    } finally {
      set({ busy: false });
    }
  },
}));

/** 当前是否已解锁专业版；非商店构建恒为 true。 */
export function usePro(): boolean {
  return useEntitlementStore((s) => s.status?.pro ?? !STORE_BUILD);
}
