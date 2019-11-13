#[derive(Debug, Default, Clone)]
pub struct Event {
    pub ty: String,
    pub data: String,
    pub origin: String,
    pub last_event_id: String,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct EventBuf {
    pub(crate) ty: Option<String>,
    pub(crate) data: Vec<String>,
    pub(crate) last_event_id: String,
}
