//! Linux CPU constraint backend using cgroup v2.
//!
//! Creates a dedicated `brake-*` subtree under the process's current cgroup.
//! Each distinct [`crate::Brake`] is a lazily created child cgroup with
//! `cgroup.type = threaded`. Runnable brakes are translated to `cpu.max`
//! quotas plus `cpu.weight`. `Brake::Stop` is translated to `cgroup.freeze = 1`.
//!
//! Threads are moved into brakes by writing their TID to `cgroup.threads`.
//! Runnable brakes provide hard caps regardless of contention. Per-thread CPU
//! time is read from the kernel's thread CPU clock via `pthread_getcpuclockid`
//! and `clock_gettime`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::brake::Brake;
use crate::error::Error;
use crate::platform::PlatformBackend;

static CGROUP_SEQ: AtomicUsize = AtomicUsize::new(0);

struct BrakeDir {
  path: PathBuf,
}

#[derive(Default)]
struct LinuxState {
  brake_dirs: HashMap<Brake, BrakeDir>,
  thread_brakes: HashMap<u32, Brake>,
  thread_cpu_clocks: HashMap<u32, libc::clockid_t>,
  brake_thread_counts: HashMap<Brake, usize>,
}

pub(crate) struct LinuxBackend {
  root: PathBuf,
  state: Mutex<LinuxState>,
}

impl LinuxBackend {
  pub(crate) fn new() -> Result<Self, Error> {
    let self_cgroup = fs::read_to_string("/proc/self/cgroup")
      .map_err(|e| Error::Os(format!("cannot read /proc/self/cgroup: {}", e)))?;
    let cg_path = self_cgroup
      .lines()
      .find(|l| l.starts_with("0::"))
      .map(|l| &l[3..])
      .unwrap_or("/")
      .trim();
    let parent = PathBuf::from("/sys/fs/cgroup").join(cg_path.trim_start_matches('/'));
    let unique = CGROUP_SEQ.fetch_add(1, Ordering::Relaxed);
    let root = parent.join(format!("brake-{}-{}", std::process::id(), unique));
    Ok(Self { root, state: Mutex::new(LinuxState::default()) })
  }

  fn write_file(path: &Path, value: &str) -> Result<(), Error> {
    fs::write(path, value)
      .map_err(|e| Error::Os(format!("write '{}' to {}: {}", value, path.display(), e)))
  }

  fn gettid() -> u32 {
    unsafe { libc::syscall(libc::SYS_gettid) as u32 }
  }

  fn enable_cpu_controller(dir: &Path) -> Result<(), Error> {
    let sc = dir.join("cgroup.subtree_control");
    if let Ok(content) = fs::read_to_string(&sc) {
      if !content.split_whitespace().any(|c| c == "cpu") {
        let _ = Self::write_file(&sc, "+cpu");
      }
    }
    Ok(())
  }

  fn dir_name(brake: Brake) -> String {
    match brake {
      Brake::Stop => "stop".into(),
      Brake::Background => "background".into(),
      Brake::Custom(value) => format!("custom-{}", value.raw()),
    }
  }

  fn configure_runnable(dir: &Path, cpu_fraction: f64, thread_count: usize) -> Result<(), Error> {
    let period_us: u64 = 100_000;
    let fraction = cpu_fraction.clamp(0.0, 1.0);
    let desired_cores = (fraction * thread_count as f64).max(0.01);
    let weight = (desired_cores * 100.0).round().clamp(1.0, 10_000.0) as u64;
    Self::write_file(&dir.join("cgroup.freeze"), "0")?;
    let _ = Self::write_file(&dir.join("cpu.weight"), &weight.to_string());

    if fraction >= 1.0 {
      return Self::write_file(&dir.join("cpu.max"), &format!("max {}", period_us));
    }

    let quota_us = ((fraction * thread_count as f64 * period_us as f64).round() as u64).max(1000);
    Self::write_file(&dir.join("cpu.max"), &format!("{} {}", quota_us, period_us))?;
    let _ = Self::write_file(&dir.join("cpu.max.burst"), "0");
    Ok(())
  }

  fn ensure_brake_dir(&self, state: &mut LinuxState, brake: Brake) -> Result<PathBuf, Error> {
    if let Some(existing) = state.brake_dirs.get(&brake) {
      return Ok(existing.path.clone());
    }

    let dir = self.root.join(Self::dir_name(brake));
    fs::create_dir_all(&dir).map_err(|e| Error::Os(format!("mkdir {}: {}", dir.display(), e)))?;

    let cgroup_type = dir.join("cgroup.type");
    if let Ok(content) = fs::read_to_string(&cgroup_type) {
      if content.trim() != "threaded" {
        Self::write_file(&cgroup_type, "threaded")?;
      }
    }

    match brake {
      Brake::Stop => {
        Self::write_file(&dir.join("cgroup.freeze"), "1")?;
      }
      _ => {
        Self::configure_runnable(&dir, brake.cpu_fraction().unwrap_or(1.0), 1)?;
      }
    }

    state.brake_dirs.insert(brake, BrakeDir { path: dir.clone() });
    Ok(dir)
  }

  fn apply_brake_config(&self, dir: &Path, brake: Brake, thread_count: usize) -> Result<(), Error> {
    match brake {
      Brake::Stop => Self::write_file(&dir.join("cgroup.freeze"), "1"),
      _ => Self::configure_runnable(dir, brake.cpu_fraction().unwrap_or(1.0), thread_count.max(1)),
    }
  }
}

impl PlatformBackend for LinuxBackend {
  fn setup(&mut self) -> Result<(), Error> {
    let parent = self.root.parent().unwrap_or(Path::new("/sys/fs/cgroup"));
    Self::enable_cpu_controller(parent)?;

    fs::create_dir_all(&self.root)
      .map_err(|e| Error::Os(format!("mkdir {}: {}", self.root.display(), e)))?;

    let root_type = self.root.join("cgroup.type");
    if let Ok(ct) = fs::read_to_string(&root_type) {
      if ct.trim() == "domain invalid" || ct.trim() == "domain" {
        let _ = Self::write_file(&root_type, "threaded");
      }
    }
    let _ = Self::write_file(&self.root.join("cgroup.subtree_control"), "+cpu");

    // Move main thread into the subtree root so children can be threaded
    let tid = Self::gettid();
    Self::write_file(&self.root.join("cgroup.threads"), &tid.to_string())?;

    Ok(())
  }

  fn move_thread(&self, os_id: u32, brake: Brake) -> Result<(), Error> {
    let mut state = self.state.lock().unwrap();
    let old_brake = *state
      .thread_brakes
      .get(&os_id)
      .ok_or_else(|| Error::Os(format!("unknown thread {}", os_id)))?;
    if old_brake == brake {
      return Ok(());
    }

    let new_count = state.brake_thread_counts.get(&brake).copied().unwrap_or(0) + 1;
    let new_dir = self.ensure_brake_dir(&mut state, brake)?;
    self.apply_brake_config(&new_dir, brake, new_count)?;

    if let Err(err) = Self::write_file(&new_dir.join("cgroup.threads"), &os_id.to_string()) {
      if !brake.is_stopped() {
        let rollback_count = state.brake_thread_counts.get(&brake).copied().unwrap_or(0);
        let _ = self.apply_brake_config(&new_dir, brake, rollback_count.max(1));
      }
      return Err(err);
    }

    let old_count = *state
      .brake_thread_counts
      .get(&old_brake)
      .ok_or_else(|| Error::Os(format!("missing thread count for {:?}", old_brake)))?;
    state.thread_brakes.insert(os_id, brake);
    *state.brake_thread_counts.entry(brake).or_insert(0) += 1;
    if old_count <= 1 {
      state.brake_thread_counts.remove(&old_brake);
    } else if let Some(count) = state.brake_thread_counts.get_mut(&old_brake) {
      *count -= 1;
    }

    if !old_brake.is_stopped() {
      if let Some(old_dir) = state.brake_dirs.get(&old_brake).map(|dir| dir.path.clone()) {
        let remaining = state.brake_thread_counts.get(&old_brake).copied().unwrap_or(0);
        self.apply_brake_config(&old_dir, old_brake, remaining)?;
      }
    }

    Ok(())
  }

  fn read_brake_usage(&self, brake: Brake) -> Result<u64, Error> {
    let mut state = self.state.lock().unwrap();
    let dir = self.ensure_brake_dir(&mut state, brake)?;
    let path = dir.join("cpu.stat");
    let content = fs::read_to_string(&path)
      .map_err(|e| Error::Os(format!("read {}: {}", path.display(), e)))?;
    for line in content.lines() {
      let mut parts = line.split_whitespace();
      if let (Some("usage_usec"), Some(val)) = (parts.next(), parts.next()) {
        return val.parse().map_err(|e| Error::Os(format!("parse usage: {}", e)));
      }
    }
    Ok(0)
  }

  fn read_thread_cpu_usec(&self, os_id: u32) -> Result<u64, Error> {
    let clock_id = {
      let state = self.state.lock().unwrap();
      *state.thread_cpu_clocks.get(&os_id).ok_or(Error::ThreadGone)?
    };

    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    let rc = unsafe { libc::clock_gettime(clock_id, &mut ts) };
    if rc != 0 {
      let err = std::io::Error::last_os_error();
      if matches!(err.raw_os_error(), Some(libc::ESRCH | libc::EINVAL)) {
        return Err(Error::ThreadGone);
      }
      return Err(Error::Os(format!("clock_gettime(thread_cpu_clock {}): {}", os_id, err)));
    }

    Ok(ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1_000)
  }

  fn register_thread(
    &self,
    os_id: u32,
    thread_cpu_clock: Option<libc::clockid_t>,
    brake: Brake,
  ) -> Result<(), Error> {
    let mut state = self.state.lock().unwrap();
    let new_count = state.brake_thread_counts.get(&brake).copied().unwrap_or(0) + 1;
    let dir = self.ensure_brake_dir(&mut state, brake)?;
    self.apply_brake_config(&dir, brake, new_count)?;
    let thread_cpu_clock = thread_cpu_clock
      .ok_or_else(|| Error::Os(format!("missing thread CPU clock for {}", os_id)))?;

    if let Err(err) = Self::write_file(&dir.join("cgroup.threads"), &os_id.to_string()) {
      if !brake.is_stopped() {
        let rollback_count = state.brake_thread_counts.get(&brake).copied().unwrap_or(0);
        let _ = self.apply_brake_config(&dir, brake, rollback_count.max(1));
      }
      return Err(err);
    }

    state.thread_brakes.insert(os_id, brake);
    state.thread_cpu_clocks.insert(os_id, thread_cpu_clock);
    *state.brake_thread_counts.entry(brake).or_insert(0) += 1;
    Ok(())
  }

  fn unregister_thread(&self, os_id: u32) -> Result<(), Error> {
    let mut state = self.state.lock().unwrap();
    let Some(brake) = state.thread_brakes.remove(&os_id) else {
      return Ok(());
    };
    state.thread_cpu_clocks.remove(&os_id);

    let old_count = *state
      .brake_thread_counts
      .get(&brake)
      .ok_or_else(|| Error::Os(format!("missing thread count for {:?}", brake)))?;
    if old_count <= 1 {
      state.brake_thread_counts.remove(&brake);
    } else if let Some(count) = state.brake_thread_counts.get_mut(&brake) {
      *count -= 1;
    }

    if !brake.is_stopped() {
      if let Some(dir) = state.brake_dirs.get(&brake).map(|dir| dir.path.clone()) {
        let remaining = state.brake_thread_counts.get(&brake).copied().unwrap_or(0);
        self.apply_brake_config(&dir, brake, remaining)?;
      }
    }

    Ok(())
  }

  fn online_cpus(&self) -> usize {
    unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) as usize }
  }

  fn cleanup(&self) -> Result<(), Error> {
    let parent_threads =
      self.root.parent().unwrap_or(Path::new("/sys/fs/cgroup")).join("cgroup.threads");
    let state = self.state.lock().unwrap();

    // Move all threads back to the parent
    for dir in state.brake_dirs.values() {
      let _ = Self::write_file(&dir.path.join("cgroup.freeze"), "0");
      let tf = dir.path.join("cgroup.threads");
      if let Ok(c) = fs::read_to_string(&tf) {
        for line in c.lines() {
          let tid = line.trim();
          if !tid.is_empty() {
            let _ = fs::write(&parent_threads, tid);
          }
        }
      }
    }
    let rt = self.root.join("cgroup.threads");
    if let Ok(c) = fs::read_to_string(&rt) {
      for line in c.lines() {
        let tid = line.trim();
        if !tid.is_empty() {
          let _ = fs::write(&parent_threads, tid);
        }
      }
    }

    for dir in state.brake_dirs.values() {
      let _ = fs::remove_dir(&dir.path);
    }
    let _ = fs::remove_dir(&self.root);
    Ok(())
  }
}
