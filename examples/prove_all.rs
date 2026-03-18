//! Demonstrates the full validation suite: saturation, demotion/promotion, and
//! per-thread CPU accounting in one long-running proof-oriented example.

use brake::{Brake, BrakeController};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let controller = BrakeController::new()?;
  let ncpus = controller.online_cpus();
  let cold_brake = Brake::custom(0.01)?;
  println!("=== brake prove_all ===");
  println!("platform: {}", std::env::consts::OS);
  println!("online CPUs: {}", ncpus);
  println!();

  let mut pass = true;

  // ── Test 1: Hostile saturation ──────────────────────────────
  {
    println!("--- test 1: hostile saturation ---");

    let mut fg = Vec::new();
    for _ in 0..ncpus {
      fg.push(controller.spawn(Brake::full(), burner)?);
    }
    let mut bg = Vec::new();
    for _ in 0..100 {
      bg.push(controller.spawn(cold_brake, burner)?);
    }

    println!("  warmup 5s...");
    std::thread::sleep(std::time::Duration::from_secs(5));

    let t0 = brake::now_usec();
    let fg_start = controller.brake_stats(Brake::full())?;
    let bg_start = controller.brake_stats(cold_brake)?;

    println!("  observing 10s...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    let wall = brake::now_usec() - t0;
    let fg_end = controller.brake_stats(Brake::full())?;
    let bg_end = controller.brake_stats(cold_brake)?;

    let fg_cpus = (fg_end.usage_usec - fg_start.usage_usec) as f64 / wall as f64;
    let bg_cpus = (bg_end.usage_usec - bg_start.usage_usec) as f64 / wall as f64;
    let ratio = bg_cpus / fg_cpus;

    println!("  FG: {:.2} effective CPUs", fg_cpus);
    println!("  BG: {:.2} effective CPUs (100 threads)", bg_cpus);
    println!("  BG/FG ratio: {:.4}", ratio);

    let ok = ratio < 0.5;
    println!("  RESULT: {}", if ok { "PASS" } else { "FAIL" });
    pass &= ok;

    // Shutdown these threads by dropping the manager? No — we reuse it.
    // Threads will die when the process exits.
    drop(fg);
    drop(bg);
  }

  println!();

  // ── Test 2: Demotion / Promotion ────────────────────────────
  {
    println!("--- test 2: demotion / promotion ---");

    // Filler threads for contention
    for _ in 0..ncpus {
      controller.spawn(Brake::Background, burner)?;
    }
    for _ in 0..ncpus {
      controller.spawn(Brake::full(), burner)?;
    }

    let target = controller.spawn(Brake::full(), burner)?;

    std::thread::sleep(std::time::Duration::from_secs(3));

    let t0 = brake::now_usec();
    let cpu0 = controller.cpu_time(&target)?.total_usec;
    std::thread::sleep(std::time::Duration::from_secs(10));
    let cpu1 = controller.cpu_time(&target)?.total_usec;
    let wall1 = brake::now_usec() - t0;
    let slope_fg = (cpu1 - cpu0) as f64 / wall1 as f64;
    println!("  slope_fg = {:.4}", slope_fg);

    controller.move_thread(&target, Brake::Background)?;
    let t1 = brake::now_usec();
    let cpu2 = controller.cpu_time(&target)?.total_usec;
    std::thread::sleep(std::time::Duration::from_secs(15));
    let cpu3 = controller.cpu_time(&target)?.total_usec;
    let wall2 = brake::now_usec() - t1;
    let slope_bg = (cpu3 - cpu2) as f64 / wall2 as f64;
    println!("  slope_bg = {:.4}", slope_bg);

    controller.move_thread(&target, Brake::full())?;
    let t2 = brake::now_usec();
    let cpu4 = controller.cpu_time(&target)?.total_usec;
    std::thread::sleep(std::time::Duration::from_secs(10));
    let cpu5 = controller.cpu_time(&target)?.total_usec;
    let wall3 = brake::now_usec() - t2;
    let slope_fg_after = (cpu5 - cpu4) as f64 / wall3 as f64;
    println!("  slope_fg_after = {:.4}", slope_fg_after);

    let demotion_factor = slope_bg / slope_fg;
    let promotion_factor = slope_fg_after / slope_bg;
    println!("  demotion factor:  {:.4} (want <= 0.5)", demotion_factor);
    println!("  promotion factor: {:.4} (want >= 1.5)", promotion_factor);

    let d_ok = demotion_factor <= 0.5;
    let p_ok = promotion_factor >= 1.5;
    println!("  demotion:  {}", if d_ok { "PASS" } else { "FAIL" });
    println!("  promotion: {}", if p_ok { "PASS" } else { "FAIL" });
    pass &= d_ok;
    pass &= p_ok;
  }

  println!();

  // ── Test 3: Per-thread CPU accounting ───────────────────────
  {
    println!("--- test 3: per-thread CPU accounting ---");

    let fast = controller.spawn(Brake::full(), burner)?;
    let slow = controller.spawn(Brake::Background, burner)?;

    std::thread::sleep(std::time::Duration::from_secs(5));

    let fast_cpu = controller.cpu_time(&fast)?.total_usec;
    let slow_cpu = controller.cpu_time(&slow)?.total_usec;
    println!(
      "  fast (Brake::full()): {:.3}s, slow (Background): {:.3}s",
      fast_cpu as f64 / 1e6,
      slow_cpu as f64 / 1e6
    );

    let ok = fast_cpu > 0 && (slow_cpu as f64) < (fast_cpu as f64 * 0.5);
    println!("  RESULT: {}", if ok { "PASS" } else { "FAIL" });
    pass &= ok;
  }

  println!();
  println!("=== OVERALL: {} ===", if pass { "PASS" } else { "FAIL" });
  std::process::exit(if pass { 0 } else { 1 });
}

fn burner() {
  let mut x: u64 = 0xdeadbeef;
  loop {
    x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    std::hint::black_box(x);
  }
}
