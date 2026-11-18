//! 权益（专业版解锁）状态与 StoreKit 进程内桥接。
//!
//! 生产路径只有一条：`StoreKitBridge.framework` 随 App 主进程加载，Rust 用
//! dlopen/dlsym 经 C ABI 调用，四个动作（查商品、购买、查权益、恢复购买）
//! 全部在进程内完成。框架缺失时降级到假 provider（`SCREENPLAY_PRO_FAKE=1`
//! 模拟已购），保证开发与直售渠道不因缺少商店能力而崩溃。
//!
//! 本地不做持久缓存：StoreKit 自己的交易记录是权威来源，缓存留给后续版本。

use crate::{IpcError, IpcResult};
use libc::{dlopen, dlerror, dlsym, RTLD_LOCAL, RTLD_NOW};
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};

const PRODUCT_ID: &str = "com.screenplay.studio.pro";
const ABI_VERSION: i32 = 1;

/// 暴露给前端的权益状态（camelCase），契约保持不变。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntitlementStatus {
    pub pro: bool,
    /// `fake` | `storekit-bridge`
    pub source: String,
    pub product_id: Option<String>,
    /// RFC 3339 时间戳
    pub checked_at: String,
    /// StoreKit 本地 JWS 校验是否通过；假 provider 恒为 true
    pub verified: bool,
}

impl EntitlementStatus {
    fn fake(pro: bool) -> Self {
        Self {
            pro,
            source: "fake".into(),
            product_id: pro.then(|| PRODUCT_ID.to_string()),
            checked_at: chrono::Utc::now().to_rfc3339(),
            verified: true,
        }
    }

    fn unavailable() -> Self {
        Self {
            pro: false,
            source: "storekit-bridge".into(),
            product_id: None,
            checked_at: chrono::Utc::now().to_rfc3339(),
            verified: false,
        }
    }

    fn from_bridge(entitlement: BridgeEntitlement) -> Self {
        Self {
            pro: entitlement.is_pro,
            source: "storekit-bridge".into(),
            product_id: entitlement.product_id,
            checked_at: chrono::Utc::now().to_rfc3339(),
            verified: entitlement.verified,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreProduct {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub display_price: String,
    #[serde(rename = "type")]
    pub product_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseOutcome {
    /// `purchased` | `cancelled` | `pending`
    pub status: String,
    pub entitlement: EntitlementStatus,
}

// ---------------------------------------------------------------------------
// framework 返回的 JSON 形态（Swift 侧 Protocol.swift 的编码结果）
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct BridgeResponse {
    ok: bool,
    #[serde(default)]
    products: Option<Vec<StoreProduct>>,
    #[serde(default)]
    entitlement: Option<BridgeEntitlement>,
    #[serde(default)]
    purchase: Option<BridgePurchase>,
    #[serde(default)]
    error: Option<BridgeError>,
}

#[derive(Deserialize)]
struct BridgeEntitlement {
    #[serde(rename = "isPro")]
    is_pro: bool,
    #[serde(rename = "productID")]
    product_id: Option<String>,
    #[serde(default)]
    verified: bool,
}

#[derive(Deserialize)]
struct BridgePurchase {
    status: String,
    entitlement: BridgeEntitlement,
}

#[derive(Deserialize)]
struct BridgeError {
    message: String,
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

type AbiVersionFn = unsafe extern "C" fn() -> i32;
type ProductsFn = unsafe extern "C" fn() -> *mut c_char;
type PurchaseFn = unsafe extern "C" fn(*const c_char) -> *mut c_char;
type EntitlementFn = unsafe extern "C" fn() -> *mut c_char;
type RestoreFn = unsafe extern "C" fn() -> *mut c_char;
type FreeFn = unsafe extern "C" fn(*mut c_char);

struct Bridge {
    _handle: *mut c_void,
    #[cfg_attr(not(test), allow(dead_code))]
    abi_version: AbiVersionFn,
    products: ProductsFn,
    purchase: PurchaseFn,
    entitlement: EntitlementFn,
    restore: RestoreFn,
    free: FreeFn,
}

// dlopen 句柄与函数指针在进程生命周期内只读使用。
unsafe impl Send for Bridge {}
unsafe impl Sync for Bridge {}

impl Bridge {
    /// 调用返回 JSON 的 C 函数并解析；失败时释放内存并返回错误信息。
    unsafe fn json(&self, pointer: *mut c_char) -> Result<BridgeResponse, String> {
        if pointer.is_null() {
            return Err("StoreKitBridge 未返回结果".into());
        }
        let text = CStr::from_ptr(pointer).to_string_lossy().into_owned();
        (self.free)(pointer);
        let response: BridgeResponse =
            serde_json::from_str(&text).map_err(|e| format!("StoreKitBridge 返回非 JSON: {e}"))?;
        if !response.ok {
            return Err(response
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| "StoreKitBridge 返回失败".into()));
        }
        Ok(response)
    }
}

fn load(path: &Path) -> Result<Bridge, String> {
    let cpath = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let handle = unsafe { dlopen(cpath.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
    if handle.is_null() {
        return Err(format!("dlopen {} 失败: {}", path.display(), loader_error()));
    }
    unsafe {
        let abi_version: AbiVersionFn = symbol(handle, "sk_abi_version")?;
        if abi_version() != ABI_VERSION {
            return Err(format!("StoreKitBridge ABI 版本不匹配（期望 {ABI_VERSION}）"));
        }
        Ok(Bridge {
            _handle: handle,
            abi_version,
            products: symbol(handle, "sk_products")?,
            purchase: symbol(handle, "sk_purchase")?,
            entitlement: symbol(handle, "sk_entitlement")?,
            restore: symbol(handle, "sk_restore")?,
            free: symbol(handle, "sk_free")?,
        })
    }
}

unsafe fn symbol<T: Copy>(handle: *mut c_void, name: &str) -> Result<T, String> {
    let cname = CString::new(name).map_err(|e| e.to_string())?;
    let pointer = dlsym(handle, cname.as_ptr());
    if pointer.is_null() {
        return Err(format!("StoreKitBridge 缺少符号 {name}"));
    }
    Ok(std::mem::transmute_copy(&pointer))
}

fn loader_error() -> String {
    unsafe {
        let error = dlerror();
        if error.is_null() {
            "未知加载错误".into()
        } else {
            CStr::from_ptr(error).to_string_lossy().into_owned()
        }
    }
}

fn dev_framework_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../packaging/iap/Framework/StoreKitBridge.framework/StoreKitBridge")
}

fn locate(app: &AppHandle) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("SCREENPLAY_STOREKIT_FRAMEWORK") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(resources) = app.path().resource_dir() {
        if let Some(contents) = resources.parent() {
            candidates.push(contents.join("Frameworks/StoreKitBridge.framework/StoreKitBridge"));
            candidates.push(
                contents.join("Frameworks/StoreKitBridge.framework/Versions/A/StoreKitBridge"),
            );
        }
    }
    candidates.push(dev_framework_path());
    candidates.into_iter().find(|path| path.is_file())
}

/// 整个进程只加载一次；加载失败后不再重试，避免每次查询都卡在 dlopen 上。
fn bridge(app: &AppHandle) -> Option<&'static Bridge> {
    static BRIDGE: OnceLock<Option<Bridge>> = OnceLock::new();
    BRIDGE
        .get_or_init(|| match locate(app) {
            Some(path) => match load(&path) {
                Ok(bridge) => {
                    crate::diag::log(&format!("StoreKitBridge 已加载: {}", path.display()));
                    Some(bridge)
                }
                Err(error) => {
                    crate::diag::log(&format!("StoreKitBridge 加载失败: {error}"));
                    None
                }
            },
            None => {
                crate::diag::log("未找到 StoreKitBridge.framework，权益查询降级为假 provider");
                None
            }
        })
        .as_ref()
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub trait EntitlementProvider: Send + Sync {
    fn status(&self) -> EntitlementStatus;
    fn products(&self) -> Result<Vec<StoreProduct>, String>;
    fn purchase(&self, product_id: Option<&str>) -> Result<PurchaseOutcome, String>;
    fn restore(&self) -> Result<EntitlementStatus, String>;
}

/// 假 provider：只读环境变量，不发任何网络请求。
pub struct FakeProvider {
    unlocked: AtomicBool,
}

impl FakeProvider {
    pub fn from_env() -> Self {
        Self {
            unlocked: AtomicBool::new(
                std::env::var("SCREENPLAY_PRO_FAKE").map(|v| v == "1").unwrap_or(false),
            ),
        }
    }
}

impl EntitlementProvider for FakeProvider {
    fn status(&self) -> EntitlementStatus {
        EntitlementStatus::fake(self.unlocked.load(Ordering::SeqCst))
    }

    fn products(&self) -> Result<Vec<StoreProduct>, String> {
        Ok(vec![StoreProduct {
            id: PRODUCT_ID.into(),
            display_name: "专业版".into(),
            description: "本地联调占位商品".into(),
            display_price: "—".into(),
            product_type: "NonConsumable".into(),
        }])
    }

    fn purchase(&self, _product_id: Option<&str>) -> Result<PurchaseOutcome, String> {
        self.unlocked.store(true, Ordering::SeqCst);
        Ok(PurchaseOutcome {
            status: "purchased".into(),
            entitlement: self.status(),
        })
    }

    fn restore(&self) -> Result<EntitlementStatus, String> {
        Ok(self.status())
    }
}

struct BridgeProvider {
    bridge: &'static Bridge,
}

impl EntitlementProvider for BridgeProvider {
    fn status(&self) -> EntitlementStatus {
        match unsafe { self.bridge.json((self.bridge.entitlement)()) } {
            Ok(response) => response
                .entitlement
                .map(EntitlementStatus::from_bridge)
                .unwrap_or_else(EntitlementStatus::unavailable),
            Err(error) => {
                crate::diag::log(&format!("权益查询失败: {error}"));
                EntitlementStatus::unavailable()
            }
        }
    }

    fn products(&self) -> Result<Vec<StoreProduct>, String> {
        let response = unsafe { self.bridge.json((self.bridge.products)()) }?;
        Ok(response.products.unwrap_or_default())
    }

    fn purchase(&self, product_id: Option<&str>) -> Result<PurchaseOutcome, String> {
        let requested = CString::new(product_id.unwrap_or(PRODUCT_ID))
            .map_err(|_| "商品 ID 含非法字符".to_string())?;
        let response = unsafe { self.bridge.json((self.bridge.purchase)(requested.as_ptr())) }?;
        let purchase = response.purchase.ok_or("StoreKitBridge 未返回购买结果")?;
        Ok(PurchaseOutcome {
            status: purchase.status,
            entitlement: EntitlementStatus::from_bridge(purchase.entitlement),
        })
    }

    fn restore(&self) -> Result<EntitlementStatus, String> {
        let response = unsafe { self.bridge.json((self.bridge.restore)()) }?;
        Ok(response
            .entitlement
            .map(EntitlementStatus::from_bridge)
            .unwrap_or_else(EntitlementStatus::unavailable))
    }
}

fn provider(app: &AppHandle) -> Box<dyn EntitlementProvider> {
    if std::env::var("SCREENPLAY_ENTITLEMENT_SOURCE").as_deref() != Ok("fake") {
        if let Some(bridge) = bridge(app) {
            return Box::new(BridgeProvider { bridge });
        }
    }
    Box::new(FakeProvider::from_env())
}

// ---------------------------------------------------------------------------
// 渠道能力门禁
// ---------------------------------------------------------------------------

/// 免费版 PDF 页脚水印文案。
pub const FREE_PDF_WATERMARK: &str = "好好写剧本 · 免费版";

/// 当前构建是否为商店渠道。
///
/// 只有带 `app-store` feature 编译的包才执行付费边界；直售 / GitHub 渠道
/// 恒为专业版，不出现任何付费墙。
pub fn store_build() -> bool {
    cfg!(feature = "app-store")
}

/// 当前是否已解锁专业版。非商店构建恒为 true。
pub fn unlocked(app: &AppHandle) -> bool {
    if !store_build() {
        return true;
    }
    provider(app).status().pro
}

/// 专业版功能门禁：未解锁时返回可直接展示的中文错误。
pub fn require_pro(app: &AppHandle, feature: &str) -> Result<(), IpcError> {
    if unlocked(app) {
        return Ok(());
    }
    Err(IpcError::InvalidArgument {
        message: format!("{feature}属于专业版功能，可在「专业版」中购买或恢复购买后使用。"),
    })
}

async fn run<T, F>(app: AppHandle, action: F) -> IpcResult<T>
where
    T: Send + 'static,
    F: FnOnce(Box<dyn EntitlementProvider>) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || action(provider(&app)))
        .await
        .map_err(|e| IpcError::AppState { message: e.to_string() })?
        .map_err(IpcError::from)
}

// ---------------------------------------------------------------------------
// IPC
// ---------------------------------------------------------------------------

/// 查询当前权益。前端只依赖这一条命令，契约不变。
#[tauri::command]
pub async fn entitlement_status(app: AppHandle) -> EntitlementStatus {
    tauri::async_runtime::spawn_blocking(move || provider(&app).status())
        .await
        .unwrap_or_else(|_| EntitlementStatus::unavailable())
}

/// 查询商品；价格由 StoreKit 返回，前端不得写死金额。
#[tauri::command]
pub async fn store_products(app: AppHandle) -> IpcResult<Vec<StoreProduct>> {
    run(app, |provider| provider.products()).await
}

/// 发起购买。购买弹窗由 StoreKit 呈现，必须从非主线程调用（命令已是 async）。
#[tauri::command]
pub async fn store_purchase(
    app: AppHandle,
    product_id: Option<String>,
) -> IpcResult<PurchaseOutcome> {
    run(app, move |provider| provider.purchase(product_id.as_deref())).await
}

/// 恢复购买。会触发 Apple ID 验证，不要频繁调用。
#[tauri::command]
pub async fn store_restore(app: AppHandle) -> IpcResult<EntitlementStatus> {
    run(app, |provider| provider.restore()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_framework_and_reports_entitlement() {
        let path = dev_framework_path();
        assert!(
            path.is_file(),
            "缺少 framework，先运行 packaging/iap/build-framework.sh"
        );
        let bridge = load(&path).expect("加载 StoreKitBridge.framework");
        assert_eq!(unsafe { (bridge.abi_version)() }, ABI_VERSION);
        let response = unsafe { bridge.json((bridge.entitlement)()) }.expect("查询权益");
        let entitlement = response.entitlement.expect("缺少 entitlement 字段");
        // ad-hoc 本机进程没有 App Store 收据，不应解锁。
        assert!(!entitlement.is_pro);
        assert!(!entitlement.verified);
    }

    #[test]
    fn resale_build_never_gates_features() {
        // 直售 / GitHub 渠道不带 app-store feature，付费墙必须完全关闭。
        if !cfg!(feature = "app-store") {
            assert!(!store_build(), "直售构建不得被识别为商店渠道");
        }
        assert!(FREE_PDF_WATERMARK.contains("免费版"));
    }
}
