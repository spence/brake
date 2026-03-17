use thread_lanes::{DefaultLanes, LaneManager};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mgr = LaneManager::<DefaultLanes>::new()?;

  let fast = mgr.spawn(DefaultLanes::Full, || loop {
    std::hint::black_box(0u64.wrapping_add(1));
  })?;

  let mut idle = Vec::new();
  for _ in 0..100 {
    idle.push(mgr.spawn(DefaultLanes::Idle, || loop {
      std::hint::black_box(0u64.wrapping_add(1));
    })?);
  }

  std::thread::sleep(std::time::Duration::from_secs(3));
  let t0 = thread_lanes::now_usec();
  let fast_start = mgr.cpu_time(&fast)?.total_usec;

  std::thread::sleep(std::time::Duration::from_secs(10));

  let wall = thread_lanes::now_usec() - t0;
  let fast_cpu = mgr.cpu_time(&fast)?.total_usec - fast_start;

  println!("{:<10}{:<12}{:<12}{:<12}CPU/WALL", "THREAD", "LANE", "CPU", "WALL");
  println!(
    "{:<10}{:<12}{:<12.3}{:<12.3}{:.4}",
    "fast-0",
    "Full",
    fast_cpu as f64 / 1e6,
    wall as f64 / 1e6,
    fast_cpu as f64 / wall as f64
  );

  for (i, h) in idle.iter().enumerate() {
    let cpu = mgr.cpu_time(h)?.total_usec;
    if i < 5 || i == 99 {
      println!(
        "idle-{:<5}{:<12}{:<12.3}{:<12.3}{:.6}",
        i,
        "Idle",
        cpu as f64 / 1e6,
        wall as f64 / 1e6,
        cpu as f64 / wall as f64
      );
    }
  }

  mgr.move_thread(&fast, DefaultLanes::Idle)?;
  println!("\n(moved fast-0 to Idle)");
  let fast_before = mgr.cpu_time(&fast)?.total_usec;
  std::thread::sleep(std::time::Duration::from_secs(5));
  let fast_after = mgr.cpu_time(&fast)?.total_usec;
  println!("fast-0 got {}us more CPU in 5s of wall time", fast_after - fast_before);

  Ok(())
}
