use crate::{IpcError,IpcResult};
use tauri_plugin_shell::ShellExt;
const LINKS:&[&str]=&[
 "https://openrouter.ai/settings/keys","https://openrouter.ai/docs/quickstart",
 "https://console.groq.com/keys","https://console.groq.com/docs/quickstart",
 "https://platform.deepseek.com","https://api-docs.deepseek.com/",
 "https://cloud.siliconflow.cn","https://siliconflow.com","https://docs.siliconflow.cn/docs/userguide/quickstart",
 "https://platform.openai.com","https://help.openai.com/en/articles/9039756-managing-billing-for-chatgpt-and-the-api-platform",
 "https://help.openai.com/en/articles/4936850-where-do-i-find-my-openai-api-key",
 "https://ollama.com/download","https://docs.ollama.com/api/openai-compatibility",
 "mailto:851194703@qq.com","mailto:huangzongjing.ziliu@gmail.com",
];
#[tauri::command]
pub fn open_help_link(app:tauri::AppHandle,url:String)->IpcResult<()>{
 if !LINKS.contains(&url.as_str()){return Err(IpcError::InvalidArgument{message:"Unknown help link".into()});}
 app.shell().open(url,None).map_err(|e|IpcError::AppState{message:e.to_string()})
}
#[cfg(test)]mod tests{use super::*;#[test]fn only_official_links_and_author_contacts(){assert!(LINKS.contains(&"mailto:851194703@qq.com"));assert!(!LINKS.contains(&"file:///etc/passwd"));assert!(!LINKS.contains(&"https://platform.openai.com.evil.test"));}}
