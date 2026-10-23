use tauri::{AppHandle, Manager};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo { cli: String, available: bool, install_dir: String }

fn tools(app: &AppHandle) -> Result<PathBuf, String> {
    let resource = app.path().resource_dir().map_err(|e| e.to_string())?;
    let mut candidates = vec![resource.join("resources/agent"), resource.join("agent")];
    #[cfg(debug_assertions)]
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/agent"));
    candidates.into_iter().find(|dir| dir.join("screenplay").is_file())
        .ok_or_else(|| "安装包缺少CLI程序，请重新安装完整版本".into())
}

#[tauri::command]
pub fn agent_connection_info(app: AppHandle) -> Result<ConnectionInfo, String> {
    let dir = tools(&app)?;
    let install_dir = dirs::home_dir().ok_or("找不到用户目录")?.join(".local/bin");
    Ok(ConnectionInfo { cli: dir.join("screenplay").to_string_lossy().into(), available: true, install_dir: install_dir.to_string_lossy().into() })
}

#[tauri::command]
pub async fn check_agent_connection(app: AppHandle) -> Result<String, String> {
    let executable = tools(&app)?.join("screenplay");
    tauri::async_runtime::spawn_blocking(move || {
        let output = std::process::Command::new(executable).arg("status").output().map_err(|e| e.to_string())?;
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        if output.status.success() { Ok(text) } else { Err(text) }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn install_agent_cli(app: AppHandle) -> Result<String, String> {
    let tools = tools(&app)?;
    let bin = dirs::home_dir().ok_or("找不到用户目录")?.join(".local/bin");
    install_links(&tools, &bin)?;
    Ok(format!("已安装到 {}。终端需将此目录加入PATH；CLI也可直接使用应用内绝对路径。移动或删除应用会使链接失效。", bin.display()))
}

fn install_links(tools: &std::path::Path, bin: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(bin).map_err(|e| e.to_string())?;
    // Preflight both names; never overwrite an unrelated executable.
    for name in ["screenplay"] {
        let target = bin.join(name);
        if std::fs::symlink_metadata(&target).is_ok() && std::fs::read_link(&target).ok().as_ref() != Some(&tools.join(name)) {
            return Err(format!("{} 已存在且不属于此安装位置，请先手动处理", target.display()));
        }
    }
    let mut created = Vec::new();
    for name in ["screenplay"] {
        let link = bin.join(name);
        if std::fs::symlink_metadata(&link).is_ok() { continue; }
        #[cfg(unix)]
        if let Err(error) = std::os::unix::fs::symlink(tools.join(name), &link) {
            for item in created { let _ = std::fs::remove_file(item); }
            return Err(error.to_string());
        }
        created.push(link);
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    #[test]
    fn cli_install_preserves_existing_programs_and_is_repeatable() {
        let dir = std::env::temp_dir().join(screenplay_core::ast::new_id("muben-cli-install-test"));
        let tools = dir.join("agent");
        let bin = dir.join("bin");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("screenplay"), "other app").unwrap();
        assert!(super::install_links(&tools, &bin).is_err());
        assert!(!bin.join("screenplay").exists());
        assert_eq!(std::fs::read_to_string(bin.join("screenplay")).unwrap(), "other app");
        std::fs::remove_file(bin.join("screenplay")).unwrap();
        super::install_links(&tools, &bin).unwrap();
        super::install_links(&tools, &bin).unwrap();
        assert_eq!(std::fs::read_link(bin.join("screenplay")).unwrap(), tools.join("screenplay"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
