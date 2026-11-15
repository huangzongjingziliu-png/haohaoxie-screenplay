// 商店渠道由 `npm run build:store` 注入；直售/GitHub 渠道为空。
// 只用于隐藏渠道专属入口，真正的权限拒止在 Rust 的 `app-store` feature 里。
export const STORE_BUILD = import.meta.env.VITE_BUILD_CHANNEL === 'app-store';
