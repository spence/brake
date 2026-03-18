//! Cross-platform thread braking: configure, spawn, throttle.
//!
//! Use the built-in [`Brake`] enum to spawn threads into stopped, background,
//! or custom CPU fractions, then move them between brakes at runtime.
//!
//! # Example
//!
//! ```no_run
//! use brake::{Brake, BrakeController};
//!
//! let controller = BrakeController::new().unwrap();
//! let worker = controller.spawn(Brake::full(), || loop {
//!   std::hint::black_box(0u64.wrapping_add(1));
//! }).unwrap();
//! controller.move_thread(&worker, Brake::Stop).unwrap();
//! controller.move_thread(&worker, Brake::Background).unwrap();
//! ```

mod brake;
mod error;
mod handle;
mod manager;
mod platform;

pub use crate::brake::{Brake, BrakeValue};
pub use crate::error::Error;
pub use crate::handle::{BrakeStats, CpuTime, ThreadHandle};
pub use crate::manager::BrakeController;

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
