// Local credential vault. The master key and ciphertext are private to this macOS user.
// Protects against accidental disclosure of config/project files, not account compromise.
use serde::{Serialize, Deserialize};
use ring::{aead, rand::{SecureRandom, SystemRandom}};
use std::path::PathBuf;
use crate::{AgentConfig, AppState, IpcError, IpcResult};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Profile { pub id: String, pub name: String, pub provider_kind: String, pub model: String, pub base_url: String, #[serde(default, skip_serializing)] pub api_key: String }
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Profiles { pub active_id: String, pub profiles: Vec<Profile> }
#[derive(Serialize, Deserialize)]
struct Vault { profiles: Vec<SecretProfile>, active_id: String }
#[derive(Serialize, Deserialize)]
struct SecretProfile { id: String, name: String, provider_kind: String, model: String, base_url: String, api_key: String }
fn folder() -> PathBuf { dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("ScreenplayStudio").join("model-vault") }
fn error(message: impl Into<String>) -> IpcError { IpcError::InvalidArgument { message: message.into() } }
fn private_write(path: &std::path::Path, data: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).map_err(|e|e.to_string())?;
    file.write_all(data).and_then(|_|file.sync_all()).map_err(|e|e.to_string())
}
fn key(dir: &std::path::Path) -> Result<aead::LessSafeKey,String> {
    let path=dir.join("master.key");
    let bytes=std::fs::read(&path).map_err(|e|e.to_string())?;
    aead::UnboundKey::new(&aead::AES_256_GCM,&bytes).map(aead::LessSafeKey::new).map_err(|_|"本机密钥无效".into())
}
fn load_at(dir: &std::path::Path) -> Result<Profiles,String> {
    if !dir.join("profiles.enc").exists() {return Ok(Profiles::default());}
    let mut bytes=std::fs::read(dir.join("profiles.enc")).map_err(|e|e.to_string())?;
    if bytes.len()<12 {return Err("模型存储文件损坏".into());}
    let nonce: [u8;12]=bytes[..12].try_into().unwrap();
    let plain=key(dir)?.open_in_place(aead::Nonce::assume_unique_for_key(nonce),aead::Aad::empty(),&mut bytes[12..]).map_err(|_|"无法解密模型配置，请勿删除或移动本机密钥文件".to_string())?;
    let v: Vault=serde_json::from_slice(plain).map_err(|_|"模型存储格式无效".to_string())?;
    Ok(Profiles {active_id:v.active_id, profiles:v.profiles.into_iter().map(|p|Profile{id:p.id,name:p.name,provider_kind:p.provider_kind,model:p.model,base_url:p.base_url,api_key:p.api_key}).collect()})
}
fn save_at(dir: &std::path::Path, p: &Profiles) -> Result<(),String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).map_err(|e|e.to_string())?;
    std::fs::set_permissions(dir,std::fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
    let rng=SystemRandom::new();
    if !dir.join("master.key").exists() {
        let mut secret=[0u8;32];rng.fill(&mut secret).map_err(|_|"密钥生成失败")?;
        private_write(&dir.join("master.key"),&secret)?;
    }
    let v=Vault{active_id:p.active_id.clone(),profiles:p.profiles.iter().map(|p|SecretProfile{id:p.id.clone(),name:p.name.clone(),provider_kind:p.provider_kind.clone(),model:p.model.clone(),base_url:p.base_url.clone(),api_key:p.api_key.clone()}).collect()};
    let mut bytes=serde_json::to_vec(&v).map_err(|e|e.to_string())?;
    let mut nonce=[0u8;12];rng.fill(&mut nonce).map_err(|_|"随机数生成失败")?;
    key(dir)?.seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(nonce),aead::Aad::empty(),&mut bytes).map_err(|_|"模型加密失败")?;
    let mut data=nonce.to_vec();data.extend(bytes);
    let suffix=nonce.iter().map(|b|format!("{b:02x}")).collect::<String>();
    let tmp=dir.join(format!("profiles-{suffix}.tmp"));
    private_write(&tmp,&data)?;
    std::fs::rename(&tmp,dir.join("profiles.enc")).map_err(|e|e.to_string())
}
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn active_config() -> Option<AgentConfig> {
    let _lock=LOCK.lock().ok()?;
    let p=load_at(&folder()).ok()?;
    p.profiles.iter().find(|v|v.id==p.active_id).map(config)
}
fn persist_settings(cfg: &AgentConfig) -> IpcResult<()> {
 crate::agent_config::save(&crate::agent_config::ProviderSettings {cloud_ai_enabled:cfg.cloud_ai_enabled,provider_kind:cfg.provider_kind.clone(),model:cfg.model.clone(),base_url:cfg.base_url.clone()}).map_err(error)
}
fn config(p: &Profile)->AgentConfig {AgentConfig {cloud_ai_enabled:true,provider_kind:p.provider_kind.clone(),model:p.model.clone(),base_url:p.base_url.clone(),api_key:p.api_key.clone()}}
pub fn remember(cfg: &AgentConfig) -> Result<(),String> {
    let _lock=LOCK.lock().unwrap();
    let mut all=load_at(&folder())?;
    let id=if all.active_id.is_empty() {"default".into()} else {all.active_id.clone()};
    let name=all.profiles.iter().find(|p|p.id==id).map(|p|p.name.clone()).unwrap_or_else(||cfg.model.clone());
    let p=Profile{id:id.clone(),name,provider_kind:cfg.provider_kind.clone(),model:cfg.model.clone(),base_url:cfg.base_url.clone(),api_key:cfg.api_key.clone()};
    all.profiles.retain(|p|p.id!=id);all.profiles.push(p);all.active_id=id;save_at(&folder(),&all)
}
#[tauri::command]
pub fn agent_list_models(state: tauri::State<AppState>)->IpcResult<Profiles> {
    let mut cfg=state.agent.lock().unwrap().clone();
    let existing={let _lock=LOCK.lock().unwrap();load_at(&folder()).map_err(error)?};
    if folder().join("profiles.enc").exists() {return Ok(existing);}
    // Migrate the previous single provider only if access is already allowed.
    // No authorization UI is requested and a denied read is harmless.
    if cfg.api_key.is_empty() {cfg.api_key=crate::credentials::load().ok().flatten().unwrap_or_default();}
    if !cfg.api_key.is_empty() && matches!(cfg.provider_kind.as_str(),"compatible"|"openai") {
        remember(&cfg).map_err(error)?;
        let mut current=state.agent.lock().unwrap();
        if current.api_key.is_empty() && current.model==cfg.model && current.base_url==cfg.base_url {current.api_key=cfg.api_key;}
    }
    let _lock=LOCK.lock().unwrap();load_at(&folder()).map_err(error)
}
#[tauri::command]
pub fn agent_save_model(state: tauri::State<AppState>, mut profile: Profile)->IpcResult<Profiles> {
    let _lock=LOCK.lock().unwrap();
    let mut all=load_at(&folder()).map_err(error)?;
    if profile.id.is_empty() {
        let mut id=[0u8;16];SystemRandom::new().fill(&mut id).map_err(|_|error("无法创建模型 ID"))?;
        profile.id=id.iter().map(|b|format!("{b:02x}")).collect();
    }
    if profile.name.trim().is_empty() || profile.model.trim().is_empty() {return Err(error("请输入模型名称和模型 ID"));}
    if !["compatible","openai","ollama"].contains(&profile.provider_kind.as_str()) {return Err(error("模型类型无效"));}
    if profile.provider_kind=="compatible" && !(profile.base_url.starts_with("https://") || profile.base_url.starts_with("http://localhost:") || profile.base_url.starts_with("http://127.0.0.1:")) {return Err(error("远程模型地址必须使用 HTTPS"));}
    if profile.api_key.is_empty() {profile.api_key=all.profiles.iter().find(|p|p.id==profile.id).map(|p|p.api_key.clone()).unwrap_or_default();}
    if profile.provider_kind!="ollama" && profile.api_key.trim().is_empty() {return Err(error("首次添加模型需要填写 API Key"));}
    all.profiles.retain(|p|p.id!=profile.id);
    all.active_id=profile.id.clone();all.profiles.push(profile.clone());
    save_at(&folder(),&all).map_err(error)?;
    drop(_lock);
    let cfg=config(&profile);persist_settings(&cfg)?;
    *state.agent.lock().unwrap()=cfg;
    Ok(all)
}
#[tauri::command]
pub fn agent_select_model(state: tauri::State<AppState>, id: String)->IpcResult<Profiles> {
    let _lock=LOCK.lock().unwrap();let mut all=load_at(&folder()).map_err(error)?;
    let cfg=config(all.profiles.iter().find(|p|p.id==id).ok_or_else(||error("模型不存在"))?);
    all.active_id=id;save_at(&folder(),&all).map_err(error)?;drop(_lock);persist_settings(&cfg)?;*state.agent.lock().unwrap()=cfg;Ok(all)
}
#[tauri::command]
pub fn agent_delete_model(state: tauri::State<AppState>, id: String)->IpcResult<Profiles> {
    let _lock=LOCK.lock().unwrap();let mut all=load_at(&folder()).map_err(error)?;
    all.profiles.retain(|p|p.id!=id);
    if all.active_id==id {all.active_id=all.profiles.first().map(|p|p.id.clone()).unwrap_or_default();}
    save_at(&folder(),&all).map_err(error)?;
    drop(_lock);
    let cfg=all.profiles.iter().find(|p|p.id==all.active_id).map(config).unwrap_or_default();persist_settings(&cfg)?;
    *state.agent.lock().unwrap()=cfg;
    Ok(all)
}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn vault_roundtrip_and_no_key_in_dto() {
 let dir=std::env::temp_dir().join(format!("muben-vault-test-{}",std::process::id()));
 let p=Profiles{active_id:"a".into(),profiles:vec![Profile{id:"a".into(),name:"test".into(),provider_kind:"compatible".into(),model:"m".into(),base_url:"https://example.com".into(),api_key:"fake-secret-credential".into()}]};
 let mut p=p;let mut second=p.profiles[0].clone();second.id="b".into();second.api_key="different-test-secret".into();p.profiles.push(second);p.active_id="b".into();
 save_at(&dir,&p).unwrap();let loaded=load_at(&dir).unwrap();assert_eq!(loaded.active_id,"b");assert_eq!(loaded.profiles.len(),2);assert_eq!(loaded.profiles[1].api_key,"different-test-secret");assert_eq!(loaded.profiles[0].api_key,"fake-secret-credential");
 assert!(!String::from_utf8_lossy(&std::fs::read(dir.join("profiles.enc")).unwrap()).contains("fake-secret"));
 assert!(serde_json::to_value(&loaded).unwrap()["profiles"][0].get("apiKey").is_none());
 let mut corrupt=std::fs::read(dir.join("profiles.enc")).unwrap();corrupt[15]^=1;std::fs::write(dir.join("profiles.enc"),corrupt).unwrap();assert!(load_at(&dir).is_err());
 std::fs::remove_dir_all(dir).unwrap();
 }
}
