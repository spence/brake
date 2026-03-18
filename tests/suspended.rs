use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use brake::{Brake, BrakeController};

fn test_guard() -> std::sync::MutexGuard<'static, ()> {
  static TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
  TEST_MUTEX.get_or_init(|| Mutex::new(())).lock().unwrap()
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
  let deadline = Instant::now() + timeout;
  while Instant::now() < deadline {
    if predicate() {
      return true;
    }
    std::thread::sleep(Duration::from_millis(10));
  }
  predicate()
}

#[test]
fn suspended_thread_stays_stopped_until_resumed() {
  let _guard = test_guard();
  let controller = BrakeController::new().unwrap();
  let running = Arc::new(AtomicBool::new(true));
  let counter = Arc::new(AtomicU64::new(0));

  let running_thread = running.clone();
  let counter_thread = counter.clone();
  let handle = controller
    .spawn(Brake::Stop, move || {
      while running_thread.load(Ordering::Relaxed) {
        counter_thread.fetch_add(1, Ordering::Relaxed);
        std::hint::spin_loop();
      }
    })
    .unwrap();

  std::thread::sleep(Duration::from_millis(200));
  assert_eq!(counter.load(Ordering::Relaxed), 0);

  let cpu_before = controller.cpu_time(&handle).unwrap().total_usec;
  std::thread::sleep(Duration::from_millis(200));
  let cpu_after = controller.cpu_time(&handle).unwrap().total_usec;
  assert!(cpu_after - cpu_before <= 20_000, "suspended CPU delta was {}", cpu_after - cpu_before);

  controller.move_thread(&handle, Brake::full()).unwrap();
  assert!(wait_until(Duration::from_secs(1), || counter.load(Ordering::Relaxed) > 0));

  let resumed_cpu_before = controller.cpu_time(&handle).unwrap().total_usec;
  std::thread::sleep(Duration::from_millis(200));
  let resumed_cpu_after = controller.cpu_time(&handle).unwrap().total_usec;
  assert!(
    resumed_cpu_after - resumed_cpu_before >= 50_000,
    "resumed CPU delta was {}",
    resumed_cpu_after - resumed_cpu_before
  );

  running.store(false, Ordering::Relaxed);
  std::thread::sleep(Duration::from_millis(50));
}

#[test]
fn brake_stats_thread_count_tracks_moves() {
  let _guard = test_guard();
  let controller = BrakeController::new().unwrap();
  let running = Arc::new(AtomicBool::new(true));
  let custom = Brake::custom(0.25).unwrap();

  let running_thread = running.clone();
  let handle = controller
    .spawn(Brake::full(), move || {
      while running_thread.load(Ordering::Relaxed) {
        std::hint::black_box(0u64.wrapping_add(1));
      }
    })
    .unwrap();

  assert_eq!(controller.brake_stats(Brake::full()).unwrap().thread_count, 1);
  assert_eq!(controller.brake_stats(Brake::Stop).unwrap().thread_count, 0);
  assert_eq!(handle.brake(), Brake::full());

  controller.move_thread(&handle, Brake::Stop).unwrap();
  assert_eq!(controller.brake_stats(Brake::full()).unwrap().thread_count, 0);
  assert_eq!(controller.brake_stats(Brake::Stop).unwrap().thread_count, 1);
  assert_eq!(handle.brake(), Brake::Stop);

  controller.move_thread(&handle, Brake::Background).unwrap();
  assert_eq!(controller.brake_stats(Brake::Stop).unwrap().thread_count, 0);
  assert_eq!(controller.brake_stats(Brake::Background).unwrap().thread_count, 1);
  assert_eq!(handle.brake(), Brake::Background);

  controller.move_thread(&handle, custom).unwrap();
  assert_eq!(controller.brake_stats(Brake::Background).unwrap().thread_count, 0);
  assert_eq!(controller.brake_stats(custom).unwrap().thread_count, 1);
  assert_eq!(handle.brake(), custom);

  controller.move_thread(&handle, Brake::full()).unwrap();
  running.store(false, Ordering::Relaxed);
  std::thread::sleep(Duration::from_millis(50));
}

#[test]
fn brake_stats_thread_count_drops_after_thread_exit() {
  let _guard = test_guard();
  let controller = BrakeController::new().unwrap();
  let running = Arc::new(AtomicBool::new(true));
  let custom = Brake::custom(0.25).unwrap();

  let running_thread = running.clone();
  let _handle = controller
    .spawn(custom, move || {
      while running_thread.load(Ordering::Relaxed) {
        std::hint::black_box(0u64.wrapping_add(1));
      }
    })
    .unwrap();

  assert!(wait_until(Duration::from_secs(1), || {
    controller.brake_stats(custom).unwrap().thread_count == 1
  }));

  running.store(false, Ordering::Relaxed);

  assert!(wait_until(Duration::from_secs(1), || {
    controller.brake_stats(custom).unwrap().thread_count == 0
  }));
}

#[test]
fn custom_brake_rejects_out_of_range_values() {
  assert!(Brake::custom(0.0).is_err());
  assert!(Brake::custom(1.1).is_err());
  assert!(Brake::custom(f64::NAN).is_err());
  assert!(Brake::custom(0.25).is_ok());
}
