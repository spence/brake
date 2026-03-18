use crate::brake::Brake;
use crate::error::Error;

pub(crate) trait PlatformBackend: Send + Sync {
  fn setup(&mut self) -> Result<(), Error>;
  fn move_thread(&self, os_id: u32, brake: Brake) -> Result<(), Error>;
  fn read_brake_usage(&self, brake: Brake) -> Result<u64, Error>;
  fn read_thread_cpu_usec(&self, os_id: u32) -> Result<u64, Error>;
  fn register_thread(
    &self,
    os_id: u32,
    thread_cpu_clock: Option<libc::clockid_t>,
    brake: Brake,
  ) -> Result<(), Error>;
  fn unregister_thread(&self, os_id: u32) -> Result<(), Error>;
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
