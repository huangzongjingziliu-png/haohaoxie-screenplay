use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Clone, Default)]
pub struct ImportControl {
    cancelled: Arc<AtomicBool>,
    progress: Option<Arc<dyn Fn(&str, usize, usize) + Send + Sync>>,
}
impl ImportControl {
    pub fn with_progress(callback: impl Fn(&str, usize, usize) + Send + Sync + 'static) -> Self {
        Self { progress: Some(Arc::new(callback)), ..Self::default() }
    }
    pub fn cancel(&self) { self.cancelled.store(true, Ordering::SeqCst); }
    pub fn is_cancelled(&self) -> bool { self.cancelled.load(Ordering::SeqCst) }
    pub fn check(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.is_cancelled(), "导入已取消");
        Ok(())
    }
    pub fn report(&self, stage: &str, completed: usize, total: usize) {
        if let Some(callback) = &self.progress { callback(stage, completed, total); }
    }
}
