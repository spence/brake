//! Demonstrates per-thread and per-brake accounting for one full-speed thread
//! competing with a large background bucket.

use brake::{Brake, BrakeController};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let controller = BrakeController::new()?;

  let fast = controller.spawn(Brake::full(), || loop {
    std::hint::black_box(0u64.wrapping_add(1));
  })?;

  let mut background = Vec::new();
  for _ in 0..100 {
    background.push(controller.spawn(Brake::Background, || loop {
      std::hint::black_box(0u64.wrapping_add(1));
    })?);
  }

  std::thread::sleep(std::time::Duration::from_secs(3));
  let t0 = brake::now_usec();
  let fast_start = controller.cpu_time(&fast)?.total_usec;
  let background_start = controller.brake_stats(Brake::Background)?.usage_usec;

  std::thread::sleep(std::time::Duration::from_secs(10));

  let wall = brake::now_usec() - t0;
  let fast_cpu = controller.cpu_time(&fast)?.total_usec - fast_start;
  let background_cpu = controller.brake_stats(Brake::Background)?.usage_usec - background_start;

  println!("{:<12}{:<18}{:<12}{:<12}CPU/WALL", "THREAD", "BRAKE", "CPU", "WALL");
  println!(
    "{:<12}{:<18}{:<12.3}{:<12.3}{:.4}",
    "fast-0",
    "Brake::full()",
    fast_cpu as f64 / 1e6,
    wall as f64 / 1e6,
    fast_cpu as f64 / wall as f64
  );
  println!(
    "{:<12}{:<18}{:<12.3}{:<12.3}{:.4}",
    "bg-total",
    "Background x100",
    background_cpu as f64 / 1e6,
    wall as f64 / 1e6,
    background_cpu as f64 / wall as f64
  );

  for (i, h) in background.iter().enumerate().take(5) {
    let cpu = controller.cpu_time(h)?.total_usec;
    if i < 5 || i == 99 {
      println!(
        "bg-{:<9}{:<18}{:<12.3}{:<12.3}{:.6}",
        i,
        "Background",
        cpu as f64 / 1e6,
        wall as f64 / 1e6,
        cpu as f64 / wall as f64
      );
    }
  }

  controller.move_thread(&fast, Brake::Background)?;
  println!("\n(moved fast-0 to Background)");
  let fast_before = controller.cpu_time(&fast)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(5));
  let fast_after = controller.cpu_time(&fast)?.total_usec;
  println!("fast-0 got {}us more CPU in 5s of wall time", fast_after - fast_before);

  Ok(())
}
