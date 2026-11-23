//! 多实例支持：以独立进程启动当前应用，用于同时打开多个工程。
//!
//! macOS 上双击工程文件时，LaunchServices 会把打开请求交给**已运行实例**，
//! 因此必须由应用主动用 `NSWorkspace` 的 `createsNewApplicationInstance`
//! 拉起第二个进程（沙盒内同样可用），再把文件交给新实例打开。

use std::path::{Path, PathBuf};

/// 当前应用包路径（仅打包后的 `.app` 可用）。
fn bundle_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法定位应用路径: {e}"))?;
    let bundle = exe
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .filter(|p| p.extension().is_some_and(|ext| ext == "app"))
        .ok_or_else(|| "当前不是打包运行，暂不支持新实例".to_string())?;
    Ok(bundle.to_path_buf())
}

/// 以新实例启动当前应用；`file` 非空时让新实例直接打开该工程。
pub fn spawn(file: Option<&Path>) -> Result<(), String> {
    let bundle = bundle_path()?;
    platform::spawn(&bundle, file)
}

/// 尝试把已打开该工程的另一个实例带到前台。
pub fn activate(pid: u32) -> bool {
    platform::activate(pid)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2_app_kit::{
        NSApplicationActivationOptions, NSRunningApplication, NSWorkspace,
        NSWorkspaceOpenConfiguration,
    };
    use objc2_foundation::{NSArray, NSString, NSURL};

    pub fn spawn(bundle: &Path, file: Option<&Path>) -> Result<(), String> {
        let app_url = NSURL::fileURLWithPath(&NSString::from_str(&bundle.to_string_lossy()));
        let config = NSWorkspaceOpenConfiguration::configuration();
        config.setCreatesNewApplicationInstance(true);
        // 让新窗口到前台：用户刚要求打开这份稿子，应直接看到它。
        config.setActivates(true);
        let workspace = NSWorkspace::sharedWorkspace();
        match file {
            Some(path) => {
                let file_url =
                    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
                let urls = NSArray::from_slice(&[&*file_url]);
                workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
                    &urls,
                    &app_url,
                    &config,
                    None,
                );
            }
            None => {
                workspace.openApplicationAtURL_configuration_completionHandler(
                    &app_url, &config, None,
                );
            }
        }
        Ok(())
    }

    pub fn activate(pid: u32) -> bool {
        let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
        else {
            return false;
        };
        app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows)
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;

    pub fn spawn(_bundle: &Path, _file: Option<&Path>) -> Result<(), String> {
        Err("当前平台暂不支持多实例".to_string())
    }

    pub fn activate(_pid: u32) -> bool {
        false
    }
}
