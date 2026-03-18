//! Demonstrates the cross-platform stop brake by suspending a thread, resuming
//! it into full speed, and then stopping it again.

use brake::{Brake, BrakeController};

fn burner() {
  let mut x: u64 = 0xdeadbeef;
  loop {
    x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    std::hint::black_box(x);
  }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let controller = BrakeController::new()?;
  let worker = controller.spawn(Brake::Stop, burner)?;

  println!("phase 1: worker starts suspended (3s)");
  let cpu0 = controller.cpu_time(&worker)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(3));
  let cpu1 = controller.cpu_time(&worker)?.total_usec;
  println!("  suspended CPU delta: {}us", cpu1 - cpu0);

  controller.move_thread(&worker, Brake::full())?;
  println!("phase 2: worker resumed into Brake::full() (3s)");
  let cpu2 = controller.cpu_time(&worker)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(3));
  let cpu3 = controller.cpu_time(&worker)?.total_usec;
  println!("  resumed CPU delta:   {}us", cpu3 - cpu2);

  controller.move_thread(&worker, Brake::Stop)?;
  println!("phase 3: worker stopped again (3s)");
  let cpu4 = controller.cpu_time(&worker)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(3));
  let cpu5 = controller.cpu_time(&worker)?.total_usec;
  println!("  suspended CPU delta: {}us", cpu5 - cpu4);

  Ok(())
}
