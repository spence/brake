use thread_lanes::{DefaultLanes, LaneManager};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mgr = LaneManager::new()?;
  let ncpus = mgr.online_cpus();

  // Filler threads to create contention
  for _ in 0..ncpus {
    mgr.spawn(DefaultLanes::Idle, || {
      let mut x: u64 = 0xdeadbeef;
      loop {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        std::hint::black_box(x);
      }
    })?;
  }
  for _ in 0..ncpus {
    mgr.spawn(DefaultLanes::Full, || {
      let mut x: u64 = 0xcafebabe;
      loop {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        std::hint::black_box(x);
      }
    })?;
  }

  // Target thread — starts in Full
  let target = mgr.spawn(DefaultLanes::Full, || {
    let mut x: u64 = 0x12345678;
    loop {
      x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
      std::hint::black_box(x);
    }
  })?;

  // Phase 1: observe in Full
  println!("phase 1: target in Full (10s)");
  std::thread::sleep(std::time::Duration::from_secs(3));
  let t0 = thread_lanes::now_usec();
  let cpu0 = mgr.cpu_time(&target)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(10));
  let cpu1 = mgr.cpu_time(&target)?.total_usec;
  let wall1 = thread_lanes::now_usec() - t0;
  let slope_fg = (cpu1 - cpu0) as f64 / wall1 as f64;
  println!("  slope_fg = {:.4} CPU/wall", slope_fg);

  // Phase 2: demote to Idle
  mgr.move_thread(&target, DefaultLanes::Idle)?;
  println!("phase 2: target demoted to Idle (20s)");
  let t1 = thread_lanes::now_usec();
  let cpu2 = mgr.cpu_time(&target)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(20));
  let cpu3 = mgr.cpu_time(&target)?.total_usec;
  let wall2 = thread_lanes::now_usec() - t1;
  let slope_bg = (cpu3 - cpu2) as f64 / wall2 as f64;
  println!("  slope_bg = {:.4} CPU/wall", slope_bg);

  // Phase 3: promote back to Full
  mgr.move_thread(&target, DefaultLanes::Full)?;
  println!("phase 3: target promoted to Full (10s)");
  let t2 = thread_lanes::now_usec();
  let cpu4 = mgr.cpu_time(&target)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(10));
  let cpu5 = mgr.cpu_time(&target)?.total_usec;
  let wall3 = thread_lanes::now_usec() - t2;
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
