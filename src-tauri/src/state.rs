use std::sync::Mutex;
use crate::core::instance_manager::InstanceManager;

pub struct AppState {
    pub instances: Mutex<InstanceManager>,
    pub minecraft_pid: Mutex<Option<u32>>,
    pub playtime_start: Mutex<Option<(String, std::time::Instant)>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            instances: Mutex::new(InstanceManager { instances: vec![] }),
            minecraft_pid: Mutex::new(None),
            playtime_start: Mutex::new(None),
        }
    }
}