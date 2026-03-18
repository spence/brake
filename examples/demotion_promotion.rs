//! Demonstrates that a live thread loses CPU when demoted to a lower brake and
//! regains CPU when promoted back to full speed under contention.

use brake::{Brake, BrakeController};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let controller = BrakeController::new()?;
  let ncpus = controller.online_cpus();

  // Filler threads to create contention
  for _ in 0..ncpus {
    controller.spawn(Brake::Background, || {
      let mut x: u64 = 0xdeadbeef;
      loop {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        std::hint::black_box(x);
      }
    })?;
  }
  for _ in 0..ncpus {
    controller.spawn(Brake::full(), || {
      let mut x: u64 = 0xcafebabe;
      loop {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        std::hint::black_box(x);
      }
    })?;
  }

  let target = controller.spawn(Brake::full(), || {
    let mut x: u64 = 0x12345678;
    loop {
      x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
      std::hint::black_box(x);
    }
  })?;

  println!("phase 1: target at Brake::full() (10s)");
  std::thread::sleep(std::time::Duration::from_secs(3));
  let t0 = brake::now_usec();
  let cpu0 = controller.cpu_time(&target)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(10));
  let cpu1 = controller.cpu_time(&target)?.total_usec;
  let wall1 = brake::now_usec() - t0;
  let slope_fg = (cpu1 - cpu0) as f64 / wall1 as f64;
  println!("  slope_fg = {:.4} CPU/wall", slope_fg);

  controller.move_thread(&target, Brake::Background)?;
  println!("phase 2: target demoted to Brake::Background (20s)");
  let t1 = brake::now_usec();
  let cpu2 = controller.cpu_time(&target)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(20));
  let cpu3 = controller.cpu_time(&target)?.total_usec;
  let wall2 = brake::now_usec() - t1;
  let slope_bg = (cpu3 - cpu2) as f64 / wall2 as f64;
  println!("  slope_bg = {:.4} CPU/wall", slope_bg);

  controller.move_thread(&target, Brake::full())?;
  println!("phase 3: target promoted to Brake::full() (10s)");
  let t2 = brake::now_usec();
  let cpu4 = controller.cpu_time(&target)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(10));
  let cpu5 = controller.cpu_time(&target)?.total_usec;
  let wall3 = brake::now_usec() - t2;
  let slope_fg_after = (cpu5 - cpu4) as f64 / wall3 as f64;
  println!("  slope_fg_after = {:.4} CPU/wall", slope_fg_after);

  // Evaluate
  let demotion_factor = slope_bg / slope_fg;
  let promotion_factor = slope_fg_after / slope_bg;
  println!("\nResults:");
  println!("  demotion factor:  {:.4} (want <= 0.5)", demotion_factor);
  println!("  promotion factor: {:.4} (want >= 1.5)", promotion_factor);
  println!("  demotion:  {}", if demotion_factor <= 0.5 { "PASS" } else { "FAIL" });
  println!("  promotion: {}", if promotion_factor >= 1.5 { "PASS" } else { "FAIL" });

  Ok(())
}
