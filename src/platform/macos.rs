//! macOS CPU constraint backend using Mach thread precedence policy.
//!
//! Uses `thread_policy_set` with `THREAD_PRECEDENCE_POLICY` to control scheduling priority.
//! CPU fraction is linearly mapped to a Mach importance value: 0.0 → −127 (lowest),
//! 1.0 → 63 (highest). The scheduler uses importance to determine priority when threads compete
//! for CPU.
//!
//! This is relative priority, not a hard cap — a low-priority thread can still use full CPU if
//! nothing else is running. Enforcement depends on contention from higher-priority threads.
//! CPU time is read via `thread_info` with `THREAD_BASIC_INFO`.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::Error;
use crate::platform::PlatformBackend;

const THREAD_PRECEDENCE_POLICY: u32 = 3;
const THREAD_PRECEDENCE_POLICY_COUNT: u32 = 1;
const THREAD_BASIC_INFO: u32 = 3;
const THREAD_BASIC_INFO_COUNT: u32 = 10;
const KERN_SUCCESS: i32 = 0;

#[repr(C)]
struct ThreadPrecedencePolicy {
  importance: i32,
}

#[repr(C)]
#[derive(Default)]
struct TimeValue {
  seconds: i32,
  microseconds: i32,
}

#[repr(C)]
#[derive(Default)]
struct ThreadBasicInfo {
  user_time: TimeValue,
  system_time: TimeValue,
  cpu_usage: i32,
  policy: i32,
  run_state: i32,
  flags: i32,
  suspend_count: i32,
  sleep_time: i32,
}

unsafe extern "C" {
  fn thread_policy_set(
    thread: u32,
    flavor: u32,
    policy_info: *const ThreadPrecedencePolicy,
    count: u32,
  ) -> i32;
  fn thread_info(
    target_act: u32,
    flavor: u32,
    thread_info_out: *mut ThreadBasicInfo,
    thread_info_count: *mut u32,
  ) -> i32;
}

struct LaneConfig {
  importance: i32,
}

struct ThreadEntry {
  lane_idx: usize,
}

pub(crate) struct MacosBackend {
  lane_configs: Vec<LaneConfig>,
  threads: Mutex<HashMap<u32, ThreadEntry>>,
}

impl MacosBackend {
  pub(crate) fn new() -> Self {
    Self { lane_configs: Vec::new(), threads: Mutex::new(HashMap::new()) }
  }

  fn fraction_to_importance(frac: f64) -> i32 {
    // 0.0 → -127, 1.0 → 63, linear interpolation
    let frac = frac.clamp(0.0, 1.0);
    (-127.0 + frac * 190.0) as i32
  }

  fn set_thread_importance(mach_port: u32, importance: i32) -> Result<(), Error> {
    let policy = ThreadPrecedencePolicy { importance };
    let kr = unsafe {
      thread_policy_set(
        mach_port,
        THREAD_PRECEDENCE_POLICY,
        &policy,
        THREAD_PRECEDENCE_POLICY_COUNT,
      )
    };
    if kr != KERN_SUCCESS {
      return Err(Error::Os(format!("thread_policy_set failed: kern_return={}", kr)));
    }
    Ok(())
  }

  fn read_mach_thread_cpu_usec(mach_port: u32) -> Result<u64, Error> {
    let mut info = ThreadBasicInfo::default();
    let mut count = THREAD_BASIC_INFO_COUNT;
    let kr = unsafe { thread_info(mach_port, THREAD_BASIC_INFO, &mut info, &mut count) };
    if kr != KERN_SUCCESS {
      return Err(Error::ThreadGone);
    }
    let user_us = info.user_time.seconds as u64 * 1_000_000 + info.user_time.microseconds as u64;
    let sys_us = info.system_time.seconds as u64 * 1_000_000 + info.system_time.microseconds as u64;
    Ok(user_us + sys_us)
  }
}

impl PlatformBackend for MacosBackend {
  fn setup(&mut self, lanes: &[(usize, f64)]) -> Result<(), Error> {
    self.lane_configs = lanes
      .iter()
      .map(|&(_, frac)| LaneConfig { importance: Self::fraction_to_importance(frac) })
      .collect();
    Ok(())
  }

  fn move_thread(&self, os_id: u32, lane_idx: usize) -> Result<(), Error> {
    let config = self
      .lane_configs
      .get(lane_idx)
      .ok_or_else(|| Error::Os(format!("invalid lane index {}", lane_idx)))?;
    Self::set_thread_importance(os_id, config.importance)?;
    let mut threads = self.threads.lock().unwrap();
    threads.insert(os_id, ThreadEntry { lane_idx });
    Ok(())
  }

  fn read_lane_usage(&self, lane_idx: usize) -> Result<u64, Error> {
    let threads = self.threads.lock().unwrap();
    let mut total = 0u64;
    for (mach_port, entry) in threads.iter() {
      if entry.lane_idx == lane_idx {
        if let Ok(cpu_us) = Self::read_mach_thread_cpu_usec(*mach_port) {
          total += cpu_us;
        }
      }
    }
    Ok(total)
  }

  fn read_thread_cpu_usec(&self, os_id: u32) -> Result<u64, Error> {
    Self::read_mach_thread_cpu_usec(os_id)
  }

  fn register_thread(&self, os_id: u32, lane_idx: usize) -> Result<(), Error> {
    let config = self
      .lane_configs
      .get(lane_idx)
      .ok_or_else(|| Error::Os(format!("invalid lane index {}", lane_idx)))?;
    Self::set_thread_importance(os_id, config.importance)?;
    let mut threads = self.threads.lock().unwrap();
    threads.insert(os_id, ThreadEntry { lane_idx });
    Ok(())
  }

  fn online_cpus(&self) -> usize {
    unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) as usize }
  }

  fn cleanup(&self) -> Result<(), Error> {
    let mut threads = self.threads.lock().unwrap();
    for (&mach_port, _) in threads.iter() {
      let _ = Self::set_thread_importance(mach_port, 0);
    }
    threads.clear();
    Ok(())
  }
}
