use crate::error::Error;

pub(crate) trait PlatformBackend: Send + Sync {
  /// Set up OS resources. Each entry: (lane_index, cpu_fraction 0.0..1.0)
  fn setup(&mut self, lanes: &[(usize, f64)]) -> Result<(), Error>;
  fn move_thread(&self, os_id: u32, lane_idx: usize) -> Result<(), Error>;
  fn read_lane_usage(&self, lane_idx: usize) -> Result<u64, Error>;
  fn read_thread_cpu_usec(&self, os_id: u32) -> Result<u64, Error>;
  fn register_thread(&self, os_id: u32, lane_idx: usize) -> Result<(), Error>;
  fn online_cpus(&self) -> usize;
  fn cleanup(&self) -> Result<(), Error>;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

pub(crate) fn create_backend() -> Result<Box<dyn PlatformBackend>, Error> {
  #[cfg(target_os = "linux")]
  {
    linux::LinuxBackend::new().map(|b| Box::new(b) as Box<dyn PlatformBackend>)
  }

  #[cfg(target_os = "macos")]
  {
    Ok(Box::new(macos::MacosBackend::new()))
  }

  #[cfg(not(any(target_os = "linux", target_os = "macos")))]
  {
    Err(Error::Os("unsupported platform".into()))
  }
}
