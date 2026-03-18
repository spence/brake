//! Demonstrates a single busy thread moved across several brakes while printing
//! the measured CPU time and wall time for each level.

use brake::{Brake, BrakeController};

const WINDOW_SECS: u64 = 1;

#[derive(Clone, Copy)]
struct Row {
  label: &'static str,
  brake: Brake,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let controller = BrakeController::new()?;
  let rows = [
    Row { label: "Stop", brake: Brake::Stop },
    Row { label: "Background", brake: Brake::Background },
    Row { label: "Custom(0.25)", brake: Brake::custom(0.25)? },
    Row { label: "Custom(0.50)", brake: Brake::custom(0.50)? },
    Row { label: "Custom(0.75)", brake: Brake::custom(0.75)? },
    Row { label: "Full", brake: Brake::full() },
  ];

  let worker = controller.spawn(Brake::Stop, burner)?;

  println!("{:<14}{:<12}{}", "Brake", "CPU (s)", "Wall (s)");

  for row in rows {
    controller.move_thread(&worker, row.brake)?;
    std::thread::sleep(std::time::Duration::from_millis(150));

    let cpu_before = controller.cpu_time(&worker)?.total_usec;
    let wall_before = brake::now_usec();

    std::thread::sleep(std::time::Duration::from_secs(WINDOW_SECS));

    let wall_after = brake::now_usec();
    let cpu_after = controller.cpu_time(&worker)?.total_usec;

    let cpu_delta = cpu_after - cpu_before;
    let wall_delta = wall_after - wall_before;
    println!("{:<14}{:<12.3}{:.3}", row.label, cpu_delta as f64 / 1e6, wall_delta as f64 / 1e6);
  }

  controller.move_thread(&worker, Brake::Stop)?;
  Ok(())
}
