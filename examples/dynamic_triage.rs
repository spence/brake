use thread_lanes::{DefaultLanes, LaneManager};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mgr = LaneManager::<DefaultLanes>::new()?;

  let mut workers = Vec::new();
  for _ in 0..20 {
    workers.push(mgr.spawn(DefaultLanes::Full, || {
      let mut x: u64 = 0xdeadbeef;
      loop {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        std::hint::black_box(x);
      }
    })?);
  }

  let budget_usec: u64 = 5_000_000; // 5s of CPU time
  println!(
    "dynamic triage: {} workers, budget = {}s CPU each",
    workers.len(),
    budget_usec as f64 / 1e6
  );

  // Monitor loop — demote threads that exceed budget
  for tick in 0..30 {
    std::thread::sleep(std::time::Duration::from_secs(1));
    let mut demoted = 0;
    for (i, h) in workers.iter().enumerate() {
      if h.lane() == DefaultLanes::Full {
        let cpu = mgr.cpu_time(h)?.total_usec;
        if cpu > budget_usec {
          mgr.move_thread(h, DefaultLanes::Idle)?;
          println!("  [t={}s] worker {} demoted (used {:.3}s CPU)", tick + 1, i, cpu as f64 / 1e6);
          demoted += 1;
        }
      }
    }
    if demoted == 0 {
      let still_full = workers.iter().filter(|h| h.lane() == DefaultLanes::Full).count();
      if still_full == 0 {
        println!("  [t={}s] all workers demoted", tick + 1);
        break;
      }
    }
  }

  println!("\nfinal state:");
  for (i, h) in workers.iter().enumerate() {
    let cpu = mgr.cpu_time(h)?.total_usec;
    println!("  worker {:<3} lane={:?}  cpu={:.3}s", i, h.lane(), cpu as f64 / 1e6);
  }

  Ok(())
}
