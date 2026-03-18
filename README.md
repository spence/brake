# brake

Cross-platform thread braking for Rust. Spawn threads into built-in brakes,
move them between a hard stop, a low background level, and validated custom CPU
fractions, and let the OS enforce as much of that contract as the platform can.

## Why

Sometimes you need to slow work down or stop it without requiring the workload
to cooperate. `brake` exposes a small built-in API for that:

| Brake | Meaning |
|-------|---------|
| `Brake::Stop` | Fully pause the thread until it is moved away |
| `Brake::Background` | Low built-in brake level, roughly 10% |
| `Brake::custom(x)` | Validated custom CPU fraction in `(0.0, 1.0]` |
| `Brake::full()` | Convenience helper for `Brake::custom(1.0)` |

## How it works

Create a [`BrakeController`], spawn a thread into a [`Brake`], and move it to a
different brake at runtime. The controller applies the OS-specific mechanism
before user code starts running, so a thread spawned into `Brake::Stop` stays
stopped until you resume it.

### Platform backends

**Linux**: cgroup v2 `cpu.max` plus `cpu.weight` for runnable brakes and
`cgroup.freeze = 1` for `Brake::Stop`. Each controller creates a dedicated
`brake-*` subtree and lazily creates child cgroups for each brake value it
sees. Runnable brakes are hard caps on the total CPU available to that brake
bucket, the bucket quota scales with live thread membership so a single thread
sees roughly the requested fraction of one core, and sibling buckets compete
roughly in proportion to their summed requested cores when the machine is
oversubscribed.

**macOS**: Mach thread precedence for runnable brakes and `thread_suspend` /
`thread_resume` for `Brake::Stop`. Runnable brakes are relative priorities, not
hard caps. A low-priority thread can still use spare CPU when the system is
otherwise idle.

**Windows**: not yet supported.

### Stop caveat

Stopped threads make no progress. If a stopped thread is holding a mutex, file
descriptor, allocator lock, or other shared resource, other work can block
behind it.

## Quick start

```rust
use brake::{Brake, BrakeController};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let controller = BrakeController::new()?;

    let worker = controller.spawn(Brake::full(), || loop {
        std::hint::black_box(0u64.wrapping_add(1));
    })?;

    std::thread::sleep(std::time::Duration::from_secs(1));
    println!("CPU used: {}us", controller.cpu_time(&worker)?.total_usec);

    controller.move_thread(&worker, Brake::Background)?;
    controller.move_thread(&worker, Brake::Stop)?;
    controller.move_thread(&worker, Brake::custom(0.35)?)?;
    Ok(())
}
```

## API

### `Brake`

```rust
pub enum Brake {
    Stop,
    Background,
    Custom(BrakeValue),
}
```

Use `Brake::custom(x)` to validate a custom fraction in `(0.0, 1.0]`.

### `BrakeController`

| Method | Description |
|--------|-------------|
| `new()` | Create the controller and set up OS resources |
| `spawn(brake, closure)` | Spawn a thread into a brake |
| `move_thread(handle, brake)` | Move a thread to a different brake |
| `cpu_time(handle)` | Read the thread's cumulative CPU time |
| `brake_stats(brake)` | Aggregate CPU usage and thread count for a brake |
| `online_cpus()` | Number of online CPUs |
| `shutdown()` | Clean up OS resources (also called on drop) |

### `ThreadHandle`

| Method | Description |
|--------|-------------|
| `brake()` | Current brake |
| `os_id()` | OS thread identifier |

## Running the examples

All examples work on macOS natively. On Linux, run inside a container or
environment with cgroup v2 and the cpu controller enabled.

```sh
# Single-thread brake sweep table across stop, background, and custom values
cargo run --example brake_table

# Per-thread and per-brake accounting: 1 full-speed thread vs 100 background threads
cargo run --example show_threads

# 100 hostile low-brake threads vs foreground workers
cargo run --example hostile_saturation

# Live demotion and promotion of a single thread
cargo run --example demotion_promotion

# Dynamic triage: demote threads that exceed a CPU budget
cargo run --example dynamic_triage

# Stop a thread, resume it, then stop it again
cargo run --example suspend_resume

# Full proof suite (saturation + demotion/promotion + accounting)
cargo run --example prove_all
```

## License

MIT OR Apache-2.0

## Linux Testing In Docker

The examples work on macOS natively. To test the Linux cgroup v2 backend
without changing hosts, run an example directly inside a privileged Linux
container with the repo mounted into `/work`.

Example:

```sh
docker run --rm --privileged \
  -v "$PWD":/work \
  -w /work \
  rust:1.85-bookworm \
  cargo run --example brake_table
```

The same pattern works for other examples, such as `prove_all`. Measured output
from the single-thread sweep is recorded in
[BENCH.md](/Users/spence/src/thread-lanes/BENCH.md#L1).
