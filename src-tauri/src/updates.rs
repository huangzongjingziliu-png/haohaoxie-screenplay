//! Updates are selected from this repository only; Tauri verifies the publisher signature.
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use std::time::Duration;
const REPO: &str = "https://github.com/huangzongjingziliu-png/haohaoxie-screenplay";
#[derive(Default)]
pub struct UpdateState(tokio::sync::Mutex<Pending>);
#[derive(Default)]
struct Pending { update: Option<Update>, bytes: Option<Vec<u8>> }
#[derive(Deserialize)]
struct Asset { name: String, browser_download_url: String }
#[derive(Deserialize)]
struct Release { tag_name: String, html_url: String, body: Option<String>, draft: bool, assets: Vec<Asset> }
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct Available { version: String, notes: String, url: String, automatic: bool }
fn release_version(tag: &str) -> Option<semver::Version> {
    // Desktop beta builds use a numeric app version; each new beta increments it.
    let core = tag.trim_start_matches('v').split('-').next()?;
    semver::Version::parse(core).ok()
}
fn trusted_asset(url: &str) -> bool { url.starts_with(&format!("{REPO}/releases/download/")) && !url.contains('?') && !url.contains('#') }
#[tauri::command]
pub async fn check_update(app: tauri::AppHandle, state: State<'_, UpdateState>) -> Result<Option<Available>, String> {
    let mut pending=state.0.try_lock().map_err(|_|"更新任务正在进行，请稍后再试".to_string())?;
    if pending.bytes.is_some() { return Err("更新已下载，请安装或稍后再检查".into()); }
    let client=reqwest::Client::builder().timeout(Duration::from_secs(25)).user_agent("Haohaoxie-Updater").build().map_err(|e|e.to_string())?;
    let releases: Vec<Release>=client.get("https://api.github.com/repos/huangzongjingziliu-png/haohaoxie-screenplay/releases?per_page=100").send().await.map_err(|e|e.to_string())?.error_for_status().map_err(|e|e.to_string())?.json().await.map_err(|e|e.to_string())?;
    let current=app.package_info().version.clone();
    let selected=releases.into_iter().filter(|r|!r.draft && r.html_url.starts_with(&format!("{REPO}/releases/tag/"))).filter_map(|r|release_version(&r.tag_name).map(|v|(v,r))).filter(|(v,_)|v>&current).max_by(|a,b|a.0.cmp(&b.0));
    let Some((version,release))=selected else { pending.update=None; return Ok(None); };
    let mut automatic=false;
    pending.update=None;
    if let Some(asset)=release.assets.iter().find(|a|a.name=="latest.json" && trusted_asset(&a.browser_download_url)) {
        let endpoint=asset.browser_download_url.parse().map_err(|e|format!("更新地址无效：{e}"))?;
        let update=app.updater_builder().endpoints(vec![endpoint]).map_err(|e|e.to_string())?.timeout(Duration::from_secs(25)).build().map_err(|e|e.to_string())?.check().await.map_err(|e|e.to_string())?;
        if let Some(update)=update {
            if update.version!=version.to_string() || !trusted_asset(update.download_url.as_str()) { return Err("更新包信息与发布版本不一致，已停止更新".into()); }
            automatic=true; pending.update=Some(update);
        }
    }
    Ok(Some(Available{version:version.to_string(),notes:release.body.unwrap_or_default(),url:release.html_url,automatic}))
}
#[tauri::command]
pub async fn download_update(app:tauri::AppHandle,state:State<'_,UpdateState>) -> Result<(),String> {
    let mut pending=state.0.try_lock().map_err(|_|"更新任务正在进行".to_string())?;
    let mut update=pending.update.as_ref().ok_or("请先检查更新")?.clone();
    update.timeout=Some(Duration::from_secs(300));
    let mut received=0u64;
    let bytes=update.download(|size,total|{received+=size as u64;let _=app.emit("update-progress",serde_json::json!({"received":received,"total":total}));},||{}).await.map_err(|e|format!("下载或签名验证失败：{e}"))?;
    pending.bytes=Some(bytes); Ok(())
}
#[tauri::command]
pub async fn install_update(app:tauri::AppHandle,state:State<'_,UpdateState>,project_id:Option<String>,revision:u64) -> Result<(),String> {
    let mut pending=state.0.try_lock().map_err(|_|"更新任务正在进行".to_string())?;
    let app_state=app.state::<crate::AppState>();
    let info=crate::project_info(&app_state.project);
    let actual_id=app_state.project.document().ok().map(|d|d.id);
    if info.dirty || info.state=="unsaved" || info.revision!=revision || actual_id!=project_id { return Err("剧本有新的修改或项目已切换，请保存后再更新".into()); }
    let bytes=pending.bytes.take().ok_or("更新包尚未下载")?;
    let result=pending.update.as_ref().ok_or("更新信息已失效")?.install(&bytes);
    if let Err(e)=result { pending.bytes=Some(bytes); return Err(format!("安装失败，可从 GitHub 下载安装包：{e}")); }
    // Preserve a recovery point for any concurrent external edit made during installation.
    app_state.project.shutdown();
    app_state.cleanup_bridge();
    app_state.quitting.store(true,std::sync::atomic::Ordering::SeqCst);
    app.restart();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn versions_and_asset_origin() {
        assert_eq!(release_version("v0.1.53-beta.1").unwrap().to_string(),"0.1.53");
        assert!(release_version("garbage").is_none());
        assert!(trusted_asset(&format!("{REPO}/releases/download/v0.1.53-beta.1/app.tar.gz")));
        assert!(!trusted_asset("https://github.com/evil/project/releases/download/tag/app.tar.gz"));
        assert!(!trusted_asset(&format!("{REPO}.evil/releases/download/tag/app.tar.gz")));
    }
}
