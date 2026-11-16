//! Persist user-selected project access in the sandbox container.
#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2_foundation::{NSData, NSString, NSURL, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions};
    use std::path::PathBuf;
    pub struct Access { url: Retained<NSURL>, started: bool }
    impl Drop for Access {
        fn drop(&mut self) { if self.started { unsafe { self.url.stopAccessingSecurityScopedResource(); } } }
    }
    fn bookmark_path(path: &str) -> Result<PathBuf, String> {
        let root = dirs::data_local_dir().ok_or("无法找到文件授权存储目录")?.join("com.screenplay.studio/bookmarks");
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let digest = ring::digest::digest(&ring::digest::SHA256, path.as_bytes());
        let key: String = digest.as_ref().iter().map(|b| format!("{b:02x}")).collect();
        Ok(root.join(key))
    }
    pub fn acquire(path: &str) -> Result<Option<Access>, String> {
        if std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_none() { return Ok(None); }
        let file = bookmark_path(path)?;
        let url = if file.exists() {
            let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
            unsafe { NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &NSData::from_vec(bytes), NSURLBookmarkResolutionOptions::WithSecurityScope, None, std::ptr::null_mut()) }
                .map_err(|_| "文件授权已失效，请通过“打开”重新选择剧本".to_string())?
        } else { NSURL::fileURLWithPath(&NSString::from_str(path)) };
        if let Some(resolved) = url.path() {
            if std::path::Path::new(&resolved.to_string()) != std::path::Path::new(path) {
                return Err("剧本文件位置已变化，请通过“打开”重新选择文件".into());
            }
        }
        let started = unsafe { url.startAccessingSecurityScopedResource() };
        let access = Access { url, started };
        // Save dialog destinations may not exist until after the first save.
        if std::path::Path::new(path).exists() { remember_url(path, &access.url)?; }
        Ok(Some(access))
    }
    fn remember_url(path: &str, url: &NSURL) -> Result<(), String> {
        let data = url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            NSURLBookmarkCreationOptions::WithSecurityScope, None, None)
            .map_err(|_| "无法保存文件授权，请重新选择剧本文件".to_string())?;
        let file = bookmark_path(path)?;
        let temp = file.with_extension("tmp");
        std::fs::write(&temp, data.to_vec()).map_err(|e| e.to_string())?;
        std::fs::rename(temp, file).map_err(|e| e.to_string())
    }
    pub fn remember(path: &str) -> Result<(), String> { let _access = acquire(path)?; Ok(()) }
}
#[cfg(target_os = "macos")]
pub use mac::{acquire, remember};
#[cfg(not(target_os = "macos"))]
pub fn acquire(_: &str) -> Result<Option<()>, String> { Ok(None) }
#[cfg(not(target_os = "macos"))]
pub fn remember(_: &str) -> Result<(), String> { Ok(()) }
