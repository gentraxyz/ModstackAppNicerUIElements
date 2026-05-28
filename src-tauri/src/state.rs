use std::sync::Mutex;
use crate::core::instance_manager::InstanceManager;

pub struct AppState {
    pub instances: Mutex<InstanceManager>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            instances: Mutex::new(InstanceManager { instances: vec![] }),
        }
    }
}