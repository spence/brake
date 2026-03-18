use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use crate::brake::Brake;
use crate::error::Error;
use crate::handle::{BrakeStats, CpuTime, ThreadHandle};
use crate::platform::{self, PlatformBackend};

/// Manages threads across brakes with platform-specific throttling.
///
/// # Example
///
/// ```no_run
/// use brake::{Brake, BrakeController};
///
/// let controller = BrakeController::new().unwrap();
/// let worker = controller.spawn(Brake::full(), || loop {
///   std::hint::black_box(0u64.wrapping_add(1));
/// }).unwrap();
/// controller.move_thread(&worker, Brake::Stop).unwrap();
/// controller.move_thread(&worker, Brake::Background).unwrap();
/// ```
pub struct BrakeController {
  state: Arc<ControllerState>,
}

struct ControllerState {
  backend: Mutex<Box<dyn PlatformBackend>>,
  threads: Mutex<HashMap<u32, Arc<ThreadRecord>>>,
}

struct ThreadRecord {
  brake: Arc<Mutex<Brake>>,
  started: AtomicBool,
  pending_start: Mutex<Option<mpsc::SyncSender<bool>>>,
}

struct ThreadBootstrap {
  os_id: u32,
  thread_cpu_clock: Option<libc::clockid_t>,
}

impl BrakeController {
  pub fn new() -> Result<Self, Error> {
    let mut backend = platform::create_backend()?;
    backend.setup()?;

    Ok(Self {
      state: Arc::new(ControllerState {
        backend: Mutex::new(backend),
        threads: Mutex::new(HashMap::new()),
      }),
    })
  }

  pub fn spawn<F>(&self, brake: Brake, f: F) -> Result<ThreadHandle, Error>
  where
    F: FnOnce() + Send + 'static,
  {
    let (id_tx, id_rx) = mpsc::sync_channel(1);
    let (start_tx, start_rx) = mpsc::sync_channel(1);
    let controller = Arc::downgrade(&self.state);

    std::thread::Builder::new()
      .stack_size(64 * 1024)
      .spawn(move || {
        let bootstrap = match current_thread_bootstrap() {
          Ok(bootstrap) => bootstrap,
          Err(err) => {
            let _ = id_tx.send(Err(err));
            return;
          }
        };
        let os_id = bootstrap.os_id;
        if id_tx.send(Ok(bootstrap)).is_err() {
          return;
        }
        if let Ok(true) = start_rx.recv() {
          let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
          if let Some(state) = controller.upgrade() {
            state.unregister_thread(os_id);
          }
          if let Err(payload) = result {
            std::panic::resume_unwind(payload);
          }
        }
      })
      .map_err(|e| Error::Os(format!("spawn failed: {}", e)))?;

    let bootstrap = id_rx
      .recv()
      .map_err(|e| Error::Os(format!("thread failed to publish bootstrap info: {}", e)))??;
    {
      let backend = self.state.backend.lock().unwrap();
      if let Err(err) = backend.register_thread(bootstrap.os_id, bootstrap.thread_cpu_clock, brake)
      {
        let _ = start_tx.send(false);
        return Err(err);
      }
    }

    let brake_state = Arc::new(Mutex::new(brake));
    let record = Arc::new(ThreadRecord {
      brake: brake_state.clone(),
      started: AtomicBool::new(false),
      pending_start: Mutex::new(Some(start_tx)),
    });
    self.state.threads.lock().unwrap().insert(bootstrap.os_id, record.clone());

    if !brake.is_stopped() {
      Self::start_thread(&record)?;
    }

    let handle = ThreadHandle { os_id: bootstrap.os_id, brake: brake_state };

    Ok(handle)
  }

  pub fn move_thread(&self, handle: &ThreadHandle, brake: Brake) -> Result<(), Error> {
    {
      let backend = self.state.backend.lock().unwrap();
      backend.move_thread(handle.os_id, brake)?;
    }
    if let Some(record) = self.state.find_thread_record(handle.os_id) {
      *record.brake.lock().unwrap() = brake;
      if !brake.is_stopped() && !record.started.load(Ordering::Acquire) {
        Self::start_thread(&record)?;
      }
    }
    Ok(())
  }

  pub fn cpu_time(&self, handle: &ThreadHandle) -> Result<CpuTime, Error> {
    let backend = self.state.backend.lock().unwrap();
    let total_usec = backend.read_thread_cpu_usec(handle.os_id)?;
    Ok(CpuTime { total_usec })
  }

  pub fn brake_stats(&self, brake: Brake) -> Result<BrakeStats, Error> {
    let usage_usec = {
      let backend = self.state.backend.lock().unwrap();
      backend.read_brake_usage(brake)?
    };

    let threads = self.state.threads.lock().unwrap();
    let thread_count =
      threads.values().filter(|record| *record.brake.lock().unwrap() == brake).count();

    Ok(BrakeStats { usage_usec, thread_count })
  }

  pub fn online_cpus(&self) -> usize {
    let backend = self.state.backend.lock().unwrap();
    backend.online_cpus()
  }

  pub fn shutdown(&self) -> Result<(), Error> {
    let backend = self.state.backend.lock().unwrap();
    backend.cleanup()
  }

  fn start_thread(record: &ThreadRecord) -> Result<(), Error> {
    if record
      .started
      .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
      .is_err()
    {
      return Ok(());
    }

    let sender = record.pending_start.lock().unwrap().take();
    if let Some(sender) = sender {
      sender
        .send(true)
        .map_err(|e| Error::Os(format!("thread failed to receive start signal: {}", e)))?;
    }
    Ok(())
  }
}

impl ControllerState {
  fn find_thread_record(&self, os_id: u32) -> Option<Arc<ThreadRecord>> {
    self.threads.lock().unwrap().get(&os_id).cloned()
  }

  fn unregister_thread(&self, os_id: u32) {
    let removed = self.threads.lock().unwrap().remove(&os_id);
    if removed.is_some() {
      let backend = self.backend.lock().unwrap();
      let _ = backend.unregister_thread(os_id);
    }
  }
}

impl Drop for BrakeController {
  fn drop(&mut self) {
    let _ = self.shutdown();
  }
}

#[cfg(target_os = "linux")]
fn get_thread_id() -> u32 {
  unsafe { libc::syscall(libc::SYS_gettid) as u32 }
}

#[cfg(target_os = "linux")]
fn current_thread_bootstrap() -> Result<ThreadBootstrap, Error> {
  let mut thread_cpu_clock: libc::clockid_t = 0;
  let rc = unsafe { libc::pthread_getcpuclockid(libc::pthread_self(), &mut thread_cpu_clock) };
  if rc != 0 {
    return Err(Error::Os(format!(
      "pthread_getcpuclockid(self): {}",
      std::io::Error::from_raw_os_error(rc)
    )));
  }
  Ok(ThreadBootstrap { os_id: get_thread_id(), thread_cpu_clock: Some(thread_cpu_clock) })
}

#[cfg(target_os = "macos")]
fn get_thread_id() -> u32 {
  extern "C" {
    fn mach_thread_self() -> u32;
  }
  unsafe { mach_thread_self() }
}

#[cfg(target_os = "macos")]
fn current_thread_bootstrap() -> Result<ThreadBootstrap, Error> {
  Ok(ThreadBootstrap { os_id: get_thread_id(), thread_cpu_clock: None })
}
