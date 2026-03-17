use thread_lanes::{DefaultLanes, LaneManager, Lanes};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mgr = LaneManager::new()?;
  let ncpus = mgr.online_cpus();
  println!("online CPUs: {}", ncpus);

  // Spawn foreground workers (enough to saturate)
  let mut fg = Vec::new();
  for _ in 0..ncpus {
    fg.push(mgr.spawn(DefaultLanes::Full, || {
      let mut x: u64 = 0xdeadbeef;
      loop {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        std::hint::black_box(x);
      }
    })?);
  }

  // Spawn 100 background workers (hostile — infinite CPU burn)
  let mut bg = Vec::new();
  for _ in 0..100 {
    bg.push(mgr.spawn(DefaultLanes::Idle, || {
      let mut x: u64 = 0xcafebabe;
      loop {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        std::hint::black_box(x);
      }
    })?);
  }

  println!("warmup 5s...");
  std::thread::sleep(std::time::Duration::from_secs(5));

  let t0 = thread_lanes::now_usec();
  let fg_start = mgr.lane_stats(DefaultLanes::Full)?;
  let bg_start = mgr.lane_stats(DefaultLanes::Idle)?;

  println!("observing 10s...");
  std::thread::sleep(std::time::Duration::from_secs(10));

  let wall = thread_lanes::now_usec() - t0;
  let fg_end = mgr.lane_stats(DefaultLanes::Full)?;
  let bg_end = mgr.lane_stats(DefaultLanes::Idle)?;

  let fg_cpu = fg_end.usage_usec - fg_start.usage_usec;
  let bg_cpu = bg_end.usage_usec - bg_start.usage_usec;
  let fg_cpus = fg_cpu as f64 / wall as f64;
  let bg_cpus = bg_cpu as f64 / wall as f64;

  println!("\nResults over {:.3}s wall time:", wall as f64 / 1e6);
  println!(
    "  FG ({} threads, cpu={}): {:.3}s CPU = {:.2} effective CPUs",
    fg.len(),
    DefaultLanes::Full.cpu(),
    fg_cpu as f64 / 1e6,
    fg_cpus
  );
  println!(
    "  BG ({} threads, cpu={}): {:.3}s CPU = {:.2} effective CPUs",
    bg.len(),
    DefaultLanes::Idle.cpu(),
    bg_cpu as f64 / 1e6,
    bg_cpus
  );
  println!("  BG/FG ratio: {:.4} (should be << 1.0)", bg_cpus / fg_cpus);

  Ok(())
}
