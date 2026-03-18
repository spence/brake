use std::sync::{Arc, Mutex};

use crate::brake::Brake;

#[derive(Debug, Clone)]
pub struct ThreadHandle {
  pub(crate) os_id: u32,
  pub(crate) brake: Arc<Mutex<Brake>>,
}

impl ThreadHandle {
  pub fn brake(&self) -> Brake {
    *self.brake.lock().unwrap()
  }

  pub fn os_id(&self) -> u32 {
    self.os_id
  }
}

#[derive(Debug, Clone, Copy)]
pub struct CpuTime {
  pub total_usec: u64,
}

#[derive(Debug, Clone)]
pub struct BrakeStats {
  pub usage_usec: u64,
  pub thread_count: usize,
}
