//! Cross-platform thread lane management: configure, spawn, throttle.
//!
//! Implement the [`Lanes`] trait on your enum, spawn threads into lanes with
//! [`LaneManager::spawn`], and move threads between lanes at runtime.
//!
//! # Example
//!
//! ```no_run
//! use thread_lanes::{DefaultLanes, LaneManager};
//!
//! let mgr = LaneManager::new().unwrap();
//! let h = mgr.spawn(DefaultLanes::Full, || loop {
//!   std::hint::black_box(0u64.wrapping_add(1));
//! }).unwrap();
//! mgr.move_thread(&h, DefaultLanes::Idle).unwrap();
//! ```

mod error;
mod handle;
mod lanes;
mod manager;
mod platform;

pub use crate::error::Error;
pub use crate::handle::{CpuTime, LaneStats, ThreadHandle};
pub use crate::lanes::Lanes;
pub use crate::manager::LaneManager;

/// Default lane configuration shipped with the crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefaultLanes {
  /// Unrestricted CPU.
  Full,
  /// Throttled to 10% of a core.
  Background,
  /// As slow as the OS allows.
  Idle,
}

impl Lanes for DefaultLanes {
  fn cpu(&self) -> f64 {
    match self {
      Self::Full => 1.0,
      Self::Background => 0.1,
      Self::Idle => 0.0,
    }
  }

  fn all() -> &'static [Self] {
    &[Self::Full, Self::Background, Self::Idle]
  }
}

/// Monotonic time in microseconds.
pub fn now_usec() -> u64 {
  let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
  #[cfg(target_os = "linux")]
  unsafe {
    libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &mut ts);
  }
  #[cfg(not(target_os = "linux"))]
  unsafe {
    libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
  }
  (ts.tv_sec as u64) * 1_000_000 + (ts.tv_nsec as u64) / 1_000
}
