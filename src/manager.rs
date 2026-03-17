use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::Error;
use crate::handle::{CpuTime, LaneStats, ThreadHandle};
use crate::lanes::Lanes;
use crate::platform::{self, PlatformBackend};
use crate::DefaultLanes;

/// Manages threads across lanes with platform-specific throttling.
///
/// # Example
///
/// ```no_run
/// use thread_lanes::{DefaultLanes, LaneManager};
///
/// let mgr = LaneManager::new().unwrap();
/// let h = mgr.spawn(DefaultLanes::Full, || loop {
///   std::hint::black_box(0u64.wrapping_add(1));
/// }).unwrap();
/// mgr.move_thread(&h, DefaultLanes::Idle).unwrap();
/// ```
pub struct LaneManager<L: Lanes = DefaultLanes> {
  backend: Mutex<Box<dyn PlatformBackend>>,
  variants: &'static [L],
  variant_to_idx: HashMap<L, usize>,
  threads: Mutex<Vec<Arc<ThreadRecord>>>,
}

struct ThreadRecord {
  os_id: AtomicU32,
  lane_idx: AtomicUsize,
}

impl<L: Lanes> LaneManager<L> {
  pub fn new() -> Result<Self, Error> {
    let variants = L::all();
    let lane_specs: Vec<(usize, f64)> =
      variants.iter().enumerate().map(|(i, l)| (i, l.cpu().clamp(0.0, 1.0))).collect();
    let variant_to_idx: HashMap<L, usize> =
      variants.iter().enumerate().map(|(i, l)| (*l, i)).collect();

    let mut backend = platform::create_backend()?;
    backend.setup(&lane_specs)?;

    Ok(Self {
      backend: Mutex::new(backend),
      variants,
      variant_to_idx,
      threads: Mutex::new(Vec::new()),
    })
  }

  pub fn spawn<F>(&self, lane: L, f: F) -> Result<ThreadHandle<L>, Error>
  where
    F: FnOnce() + Send + 'static,
  {
    let lane_idx = *self
      .variant_to_idx
      .get(&lane)
      .ok_or_else(|| Error::Os("unknown lane variant".into()))?;

    let slot = Arc::new(AtomicU32::new(0));
    let slot_clone = slot.clone();
    let lane_idx_atomic = Arc::new(AtomicUsize::new(lane_idx));

    let record =
      Arc::new(ThreadRecord { os_id: AtomicU32::new(0), lane_idx: AtomicUsize::new(lane_idx) });

    std::thread::Builder::new()
      .stack_size(64 * 1024)
      .spawn(move || {
        let os_id = get_thread_id();
        slot_clone.store(os_id, Ordering::Release);
        f();
      })
      .map_err(|e| Error::Os(format!("spawn failed: {}", e)))?;

    // Wait for the thread to publish its OS ID
    while slot.load(Ordering::Acquire) == 0 {
      std::hint::spin_loop();
    }
    let os_id = slot.load(Ordering::Acquire);
    record.os_id.store(os_id, Ordering::Release);

    // Register and move thread into the correct lane
    let backend = self.backend.lock().unwrap();
    backend.register_thread(os_id, lane_idx)?;

    let handle = ThreadHandle { os_id, lane_idx: lane_idx_atomic, lane_variants: self.variants };

    self.threads.lock().unwrap().push(record);

    Ok(handle)
  }

  pub fn move_thread(&self, handle: &ThreadHandle<L>, lane: L) -> Result<(), Error> {
    let lane_idx = *self
      .variant_to_idx
      .get(&lane)
      .ok_or_else(|| Error::Os("unknown lane variant".into()))?;
    let backend = self.backend.lock().unwrap();
    backend.move_thread(handle.os_id, lane_idx)?;
    handle.lane_idx.store(lane_idx, Ordering::Release);
    Ok(())
  }

  pub fn cpu_time(&self, handle: &ThreadHandle<L>) -> Result<CpuTime, Error> {
    let backend = self.backend.lock().unwrap();
    let total_usec = backend.read_thread_cpu_usec(handle.os_id)?;
    Ok(CpuTime { total_usec })
  }

  pub fn lane_stats(&self, lane: L) -> Result<LaneStats, Error> {
    let lane_idx = *self
      .variant_to_idx
      .get(&lane)
      .ok_or_else(|| Error::Os("unknown lane variant".into()))?;
    let backend = self.backend.lock().unwrap();
    let usage_usec = backend.read_lane_usage(lane_idx)?;

    let threads = self.threads.lock().unwrap();
    let thread_count = threads
      .iter()
      .filter(|r| r.lane_idx.load(Ordering::Acquire) == lane_idx)
      .count();

    Ok(LaneStats { usage_usec, thread_count })
  }

  pub fn online_cpus(&self) -> usize {
    let backend = self.backend.lock().unwrap();
    backend.online_cpus()
  }

  pub fn shutdown(&self) -> Result<(), Error> {
    let backend = self.backend.lock().unwrap();
    backend.cleanup()
  }
}

impl<L: Lanes> Drop for LaneManager<L> {
  fn drop(&mut self) {
    let _ = self.shutdown();
  }
}

#[cfg(target_os = "linux")]
fn get_thread_id() -> u32 {
  unsafe { libc::syscall(libc::SYS_gettid) as u32 }
}

#[cfg(target_os = "macos")]
fn get_thread_id() -> u32 {
  extern "C" {
    fn mach_thread_self() -> u32;
  }
  unsafe { mach_thread_self() }
}
