use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::lanes::Lanes;

#[derive(Debug, Clone)]
pub struct ThreadHandle<L: Lanes> {
  pub(crate) os_id: u32,
  pub(crate) lane_idx: Arc<AtomicUsize>,
  pub(crate) lane_variants: &'static [L],
}

impl<L: Lanes> ThreadHandle<L> {
  pub fn lane(&self) -> L {
    let idx = self.lane_idx.load(Ordering::Acquire);
    self.lane_variants[idx]
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
pub struct LaneStats {
  pub usage_usec: u64,
  pub thread_count: usize,
}
