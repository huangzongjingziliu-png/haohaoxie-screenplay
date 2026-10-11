use crate::{AppState, IpcError, IpcResult, build_agent, prepare_agent_credential};
use tauri::{State,Emitter};
use std::path::Path;
pub fn complex(path: &str) -> bool { matches!(Path::new(path).extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),"pdf"|"png"|"jpg"|"jpeg"|"heic"|"tif"|"tiff"|"bmp"|"webp") }
pub async fn ready(state: &AppState) -> IpcResult<()> {
    let cfg=state.agent.lock().unwrap().clone();
    if cfg.model.trim().is_empty() || cfg.provider_kind=="echo" || (cfg.provider_kind!="ollama" && !cfg.cloud_ai_enabled) {return Err(IpcError::InvalidArgument{message:"此功能需要模型。请先在设置中配置模型并允许云模型处理内容（本机Ollama无需开启）。".into()});}
    prepare_agent_credential(state).await?;
    let cfg=state.agent.lock().unwrap().clone();
    if cfg.provider_kind!="ollama" && (cfg.api_key.is_empty() || (cfg.provider_kind=="compatible" && cfg.base_url.is_empty())) {return Err(IpcError::InvalidArgument{message:"请先配置模型地址和API Key".into()});}
    Ok(())
}
pub fn recognize(path: &str, control: &screenplay_import::ImportControl) -> IpcResult<(String,Vec<u8>)> {
    let path=Path::new(path);
    let file=std::fs::File::open(path).map_err(|e|IpcError::InvalidArgument{message:e.to_string()})?;
    if file.metadata().map_err(|e|IpcError::Internal{message:e.to_string()})?.len()>screenplay_import::MAX_SOURCE_BYTES {return Err(IpcError::InvalidArgument{message:"文件超过512MiB".into()});}
    let bytes=std::fs::read(path).map_err(|e|IpcError::Internal{message:e.to_string()})?;
    let pages=screenplay_import::ocr::run_ocr_controlled(path,None,control).map_err(|e|IpcError::Internal{message:e.to_string()})?;
    let text=pages.into_iter().flat_map(|p|p.lines).map(|l|l.text).collect::<Vec<_>>().join("\n");
    if text.chars().filter(|c|c.is_alphabetic()).count()<2 {return Err(IpcError::InvalidArgument{message:"没有识别到正文，未导入。请检查PDF/图片清晰度。".into()});}
    Ok((text,bytes))
}
#[tauri::command]
pub async fn agent_transform_text(app:tauri::AppHandle,state: State<'_,AppState>, text:String, instruction:String, import:bool,request_id:Option<String>) -> IpcResult<String> {
    ready(&state).await?;
    screenplay_agent::transform::transform_with_progress(&build_agent(&state)?.router,&text,&instruction,import,|completed,total|{let _=app.emit("import-progress",serde_json::json!({"requestId":request_id,"stage":"agent","completed":completed,"total":total}));}).await.map_err(|e|IpcError::Internal{message:e.to_string()})
}
#[tauri::command]
pub async fn agent_attachment_text(state: State<'_,AppState>, path:String) -> IpcResult<String> {
    ready(&state).await?;
    tauri::async_runtime::spawn_blocking(move || {
        if complex(&path) {recognize(&path,&screenplay_import::ImportControl::default()).map(|r|r.0)}
        else {crate::read_import(Path::new(&path)).map(|d|screenplay_core::serializer::to_compact_source(&d)).map_err(|e|IpcError::InvalidArgument{message:e.message()})}
    }).await.map_err(|e|IpcError::Internal{message:e.to_string()})?
}

#[tauri::command]
pub fn agent_apply_selection(app:tauri::AppHandle,state:State<'_,AppState>,block_ids:Vec<String>,text:String,expected_revision:u64)->IpcResult<crate::CommandResult>{
    let r=state.project.run(|svc|svc.replace_selected_blocks(crate::Origin::Agent,&block_ids,&text,Some(expected_revision)))?;
    crate::broadcast(&app,"agent",&r);Ok(r)
}
#[tauri::command]
pub fn agent_apply_text(app:tauri::AppHandle,state:State<'_,AppState>,block_id:Option<String>,text:String,expected_revision:u64)->IpcResult<crate::CommandResult>{
    let r=state.project.run(|svc|match block_id {Some(id)=>svc.replace_block(crate::Origin::Agent,&id,&text,Some(expected_revision)),None=>svc.replace_source_text(crate::Origin::Agent,&text,Some(expected_revision))})?;
    crate::broadcast(&app,"agent",&r);Ok(r)
}
