# thread-lanes

Cross-platform thread lane management for Rust. Spawn threads into named lanes
with CPU budgets, move threads between lanes at runtime, and let the OS enforce
the limits — even on fully hostile, non-cooperative workloads.

## Why

Sometimes you need hard guarantees about CPU allocation across groups of threads.
Background indexers shouldn't starve your request handlers. Runaway tasks
shouldn't steal cycles from critical work. Operating systems have the mechanisms
to enforce this (Linux cgroup v2, macOS Mach priorities), but the APIs are
platform-specific and low-level.

`thread-lanes` provides a single abstraction: define your lanes as an enum with
CPU budgets from 0.0 to 1.0, and the library handles the rest. Threads can be
moved between lanes at any time with immediate effect.

## How it works

You implement the `Lanes` trait on an enum. Each variant declares a CPU fraction:

| Value | Meaning |
|-------|---------|
| `1.0` | Unrestricted — use as much CPU as the OS will give |
| `0.1` | 10% of the machine |
| `0.0` | OS minimum — effectively starved when there's contention |

When you create a `LaneManager`, it sets up platform-specific resources for each
lane. When you spawn or move a thread, the OS immediately begins enforcing the
new budget.

### Platform backends

**Linux** — cgroup v2 with the cpu controller. Each lane becomes a child cgroup
under a `thread-lanes` subtree. The CPU fraction is converted to a `cpu.max`
quota: `fraction * online_cpus * period_us`. A fraction of 0.0 sets the kernel
minimum (1ms per 100ms period). Threads are moved by writing their TID to the
target cgroup's `cgroup.threads` file.

**macOS** — Mach thread priorities via `thread_policy_set`. The CPU fraction is
linearly mapped to a Mach importance value: 0.0 becomes -127 (lowest), 1.0
becomes 63 (highest). Unlike Linux, macOS provides relative priority rather than
hard caps — enforcement depends on contention from higher-priority threads.

## Quick start

```rust
use thread_lanes::{DefaultLanes, LaneManager};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mgr = LaneManager::new()?;

    // Spawn a thread at full speed
    let h = mgr.spawn(DefaultLanes::Full, || loop {
        std::hint::black_box(0u64.wrapping_add(1));
    })?;

    std::thread::sleep(std::time::Duration::from_secs(5));
    println!("CPU used: {}us", mgr.cpu_time(&h)?.total_usec);

    // Demote to idle — takes effect immediately
    mgr.move_thread(&h, DefaultLanes::Idle)?;
    Ok(())
}
```

### Custom lanes

```rust
use thread_lanes::{Lanes, LaneManager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MyLanes { Hot, Warm, Cold }

impl Lanes for MyLanes {
    fn cpu(&self) -> f64 {
        match self {
            Self::Hot  => 1.0,
            Self::Warm => 0.1,
            Self::Cold => 0.0,
        }
    }
    fn all() -> &'static [Self] {
        &[Self::Hot, Self::Warm, Self::Cold]
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mgr = LaneManager::<MyLanes>::new()?;
    let h = mgr.spawn(MyLanes::Hot, || { /* work */ })?;
    mgr.move_thread(&h, MyLanes::Cold)?;
    Ok(())
}
```

## API

### `Lanes` trait

```rust
pub trait Lanes: Debug + Clone + Copy + PartialEq + Eq + Hash + Send + Sync + 'static {
    fn cpu(&self) -> f64;           // 0.0 = starved, 1.0 = unrestricted
    fn all() -> &'static [Self];    // every variant
}
```

### `DefaultLanes`

Shipped with the crate for quick use:

| Variant      | `cpu()` | Meaning                   |
|--------------|---------|---------------------------|
| `Full`       | 1.0     | Unrestricted              |
| `Background` | 0.1     | 10% of the machine        |
| `Idle`       | 0.0     | OS minimum                |

### `LaneManager<L>`

| Method | Description |
|--------|-------------|
| `new()` | Create manager, set up OS resources for all lanes |
| `spawn(lane, closure)` | Spawn a thread into a lane |
| `move_thread(handle, lane)` | Move a thread to a different lane |
| `cpu_time(handle)` | Read thread's cumulative CPU time |
| `lane_stats(lane)` | Aggregate CPU usage and thread count for a lane |
| `online_cpus()` | Number of online CPUs |
| `shutdown()` | Clean up OS resources (also called on drop) |

### `ThreadHandle<L>`

| Method | Description |
|--------|-------------|
| `lane()` | Current lane |
| `os_id()` | OS thread identifier |

## Running the examples

All examples work on macOS natively. On Linux, run inside a container or
environment with cgroup v2 and the cpu controller enabled.

```sh
# Per-thread CPU accounting: 1 Full thread vs 100 Idle threads
cargo run --example show_threads

# 100 hostile background threads vs foreground workers
cargo run --example hostile_saturation

# Live demotion and promotion of a single thread
cargo run --example demotion_promotion

# Dynamic triage: demote threads that exceed a CPU budget
cargo run --example dynamic_triage

# Full proof suite (saturation + demotion/promotion + accounting)
cargo run --example prove_all
```

### Linux testing

The examples work on macOS natively. To test the Linux cgroup v2 backend, see
[`examples/linux-docker/`](examples/linux-docker/) for a Dockerfile that builds
and runs the full suite.

### Example output (macOS, 10 CPUs)

```
$ cargo run --example show_threads

THREAD    LANE        CPU         WALL        CPU/WALL
fast-0    Full        9.999       10.004      0.9995
idle-0    Idle        0.000       10.004      0.000001
idle-1    Idle        0.000       10.004      0.000000
idle-2    Idle        0.000       10.004      0.000000
idle-3    Idle        0.000       10.004      0.000000
idle-4    Idle        0.000       10.004      0.000000
idle-99   Idle        0.000       10.004      0.000000

(moved fast-0 to Idle)
fast-0 got 0us more CPU in 5s of wall time
```

```
$ cargo run --example hostile_saturation

online CPUs: 10
warmup 5s...
observing 10s...

Results over 10.005s wall time:
  FG (10 threads, cpu=1): 88.820s CPU = 8.88 effective CPUs
  BG (100 threads, cpu=0): 0.000s CPU = 0.00 effective CPUs
  BG/FG ratio: 0.0000 (should be << 1.0)
```

## License

MIT OR Apache-2.0
