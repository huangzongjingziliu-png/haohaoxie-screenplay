//! macOS 应用退出拦截。
//!
//! ⌘Q、Dock 菜单「退出」、系统注销最终都走 `-[NSApplication terminate:]`，
//! 由 AppKit 询问 delegate 的 `applicationShouldTerminate:`。tao 没有实现这个方法，
//! AppKit 默认返回 NSTerminateNow 并直接杀进程，前端的未保存确认来不及出现。
//!
//! 这里给 tao 的 delegate 类补上该方法：一律返回 NSTerminateCancel，并通知前端
//! 走「保存并退出 / 不保存 / 取消」。真正的退出由 `confirm_quit` 调 `app.exit`，
//! 那条路径绕过 AppKit terminate，不会再次进入本拦截。

use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};

static APP: OnceLock<AppHandle> = OnceLock::new();

#[cfg(target_os = "macos")]
pub fn install(app: &AppHandle) {
    let _ = APP.set(app.clone());

    unsafe {
        use objc2::ffi::{class_addMethod, object_getClass};
        use objc2::runtime::{AnyObject, Imp, Sel};
        use objc2_app_kit::NSApplication;

        let Some(mtm) = objc2::MainThreadMarker::new() else {
            crate::diag::log("退出拦截未安装：不在主线程");
            return;
        };
        let nsapp = NSApplication::sharedApplication(mtm);
        let Some(delegate) = nsapp.delegate() else {
            crate::diag::log("退出拦截未安装：NSApplication 没有 delegate");
            return;
        };
        let object = objc2::rc::Retained::as_ptr(&delegate).cast::<AnyObject>();
        let class = object_getClass(object).cast_mut();

        // NSApplicationTerminateReply 是 NSUInteger：Q = 返回 unsigned long，
        // @ = self，: = _cmd，@ = sender。
        let added = class_addMethod(
            class,
            objc2::sel!(applicationShouldTerminate:),
            std::mem::transmute::<extern "C" fn(&AnyObject, Sel, *mut AnyObject) -> usize, Imp>(
                should_terminate,
            ),
            c"Q@:@".as_ptr(),
        );
        crate::diag::log(&format!("退出拦截安装: added={}", added.as_bool()));
    }
}

#[cfg(target_os = "macos")]
extern "C" fn should_terminate(
    _this: &objc2::runtime::AnyObject,
    _sel: objc2::runtime::Sel,
    _sender: *mut objc2::runtime::AnyObject,
) -> usize {
    crate::diag::log("applicationShouldTerminate: 拦截退出请求");
    if let Some(app) = APP.get() {
        let _ = app.emit("quit-requested", ());
    } else {
        crate::diag::log("applicationShouldTerminate: 没有 AppHandle");
    }
    0 // NSTerminateCancel：交由前端确认，确认后由 confirm_quit 退出
}

#[cfg(not(target_os = "macos"))]
pub fn install(_app: &AppHandle) {}
