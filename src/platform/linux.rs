//! Linux CPU constraint backend using cgroup v2.
//!
//! Creates a `thread-lanes` subtree under the process's current cgroup. Each lane is a child
//! cgroup with `cgroup.type = threaded`. CPU fraction is converted to a `cpu.max` quota:
//! `fraction * online_cpus * 100ms period`. Fraction 0.0 uses the kernel minimum (1ms per 100ms
//! period); fraction 1.0 leaves `cpu.max` at the default (unrestricted).
//!
//! Threads are moved into lanes by writing their TID to `cgroup.threads`. This provides hard
//! caps — enforcement is absolute regardless of contention.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::platform::PlatformBackend;

pub(crate) struct LinuxBackend {
  root: PathBuf,
  lane_dirs: Vec<PathBuf>,
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
    let root = parent.join("thread-lanes");
    Ok(Self { root, lane_dirs: Vec::new() })
  }

  fn write_file(path: &Path, value: &str) -> Result<(), Error> {
    fs::write(path, value)
      .map_err(|e| Error::Os(format!("write '{}' to {}: {}", value, path.display(), e)))
  }

  fn gettid() -> u32 {
    unsafe { libc::syscall(libc::SYS_gettid) as u32 }
  }

  fn clock_ticks_per_sec() -> u64 {
    unsafe { libc::sysconf(libc::_SC_CLK_TCK) as u64 }
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
}

impl PlatformBackend for LinuxBackend {
  fn setup(&mut self, lanes: &[(usize, f64)]) -> Result<(), Error> {
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

    self.lane_dirs = Vec::with_capacity(lanes.len());
    let ncpus = self.online_cpus() as f64;

    for &(idx, frac) in lanes {
      let dir = self.root.join(format!("lane-{}", idx));
      fs::create_dir_all(&dir).map_err(|e| Error::Os(format!("mkdir {}: {}", dir.display(), e)))?;

      let ct = dir.join("cgroup.type");
      if let Ok(t) = fs::read_to_string(&ct) {
        if t.trim() != "threaded" {
          Self::write_file(&ct, "threaded")?;
        }
      }

      let frac = frac.clamp(0.0, 1.0);
      if frac < 1.0 {
        let period_us: u64 = 100_000;
        let quota_us = if frac <= 0.0 {
          // OS minimum
          1000
        } else {
          // fraction of total machine → quota across all CPUs
          (frac * ncpus * period_us as f64) as u64
        };
        let val = format!("{} {}", quota_us, period_us);
        Self::write_file(&dir.join("cpu.max"), &val)?;
        let _ = Self::write_file(&dir.join("cpu.max.burst"), "0");
        let _ = Self::write_file(&dir.join("cpu.idle"), "1");
        let _ = Self::write_file(&dir.join("cpu.weight"), "1");
      }
      // frac == 1.0 → leave cpu.max at default ("max period")

      self.lane_dirs.push(dir);
    }

    // Re-enable subtree control after creating children
    let _ = Self::write_file(&self.root.join("cgroup.subtree_control"), "+cpu");

    Ok(())
  }

  fn move_thread(&self, os_id: u32, lane_idx: usize) -> Result<(), Error> {
    let dir = self
      .lane_dirs
      .get(lane_idx)
      .ok_or_else(|| Error::Os(format!("invalid lane index {}", lane_idx)))?;
    Self::write_file(&dir.join("cgroup.threads"), &os_id.to_string())
  }

  fn read_lane_usage(&self, lane_idx: usize) -> Result<u64, Error> {
    let dir = self
      .lane_dirs
      .get(lane_idx)
      .ok_or_else(|| Error::Os(format!("invalid lane index {}", lane_idx)))?;
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
    let path = format!("/proc/self/task/{}/stat", os_id);
    let content =
      fs::read_to_string(&path).map_err(|e| Error::Os(format!("read {}: {}", path, e)))?;
    let after_comm = content
      .rfind(')')
      .map(|i| &content[i + 2..])
      .ok_or_else(|| Error::Os(format!("malformed stat for tid {}", os_id)))?;
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    if fields.len() < 13 {
      return Err(Error::Os(format!("not enough fields for tid {}", os_id)));
    }
    let utime: u64 = fields[11].parse().map_err(|e| Error::Os(format!("parse utime: {}", e)))?;
    let stime: u64 = fields[12].parse().map_err(|e| Error::Os(format!("parse stime: {}", e)))?;
    let ticks = Self::clock_ticks_per_sec();
    Ok((utime + stime) * 1_000_000 / ticks)
  }

  fn register_thread(&self, os_id: u32, lane_idx: usize) -> Result<(), Error> {
    self.move_thread(os_id, lane_idx)
  }

  fn online_cpus(&self) -> usize {
    unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) as usize }
  }

  fn cleanup(&self) -> Result<(), Error> {
    let parent_threads =
      self.root.parent().unwrap_or(Path::new("/sys/fs/cgroup")).join("cgroup.threads");

    // Move all threads back to the parent
    for dir in &self.lane_dirs {
      let tf = dir.join("cgroup.threads");
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

    for dir in &self.lane_dirs {
      let _ = fs::remove_dir(dir);
    }
    let _ = fs::remove_dir(&self.root);
    Ok(())
  }
}
