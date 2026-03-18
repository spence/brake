# Benchmarks

## Single-Thread Brake Sweep

Date: March 17, 2026

Benchmark target: [examples/brake_table.rs](/Users/spence/src/thread-lanes/examples/brake_table.rs)

Method:

- One busy worker thread is spawned and kept alive for the full run.
- The worker is moved through `Brake::Stop`, `Brake::Background`,
  `Brake::custom(0.25)`, `Brake::custom(0.50)`, `Brake::custom(0.75)`, and
  `Brake::full()`.
- Each move gets a `150ms` settle period.
- Each measurement window is `1s`.
- CPU time comes from `BrakeController::cpu_time()`.
- Wall time comes from `brake::now_usec()`.
- No synthetic competing work is added by the harness.

All rows below are direct 1-second samples. The `Wall (s)` column shows the
actual measured wall time for the sample.

These are single-run measurements collected with no other stress examples
running at the same time. A 1-second window is intentionally short and shows
more scheduler noise than the earlier 3-second version.

On Linux, the backend uses `cpu.max` with a `100ms` period, so `0.10`, `0.25`,
`0.50`, and `0.75` map to quotas of `10ms`, `25ms`, `50ms`, and `75ms` per
period. A `1s` window covers about 10 periods, so the one-thread bucket can
still land near those target CPU totals when the quota is the dominant limiter.
The Linux per-thread accounting path now uses the kernel thread CPU clock via
`pthread_getcpuclockid` and `clock_gettime`, so the numbers are no longer
limited by `/proc` task-stat tick granularity. That table is a spot check, not
the whole validation story.

Environment:

- macOS host: 10 online CPUs
- Linux: privileged `rust:1.85-bookworm` container on the same 10-CPU host

### macOS

Command:

```sh
cargo run --example brake_table
```

Results:

| Brake | CPU (s) | Wall (s) |
|-------|---------|----------|
| `Stop` | `0.000` | `1.002` |
| `Background` | `0.698` | `1.005` |
| `Custom(0.25)` | `0.748` | `1.003` |
| `Custom(0.50)` | `0.667` | `1.005` |
| `Custom(0.75)` | `1.027` | `1.008` |
| `Full` | `1.008` | `1.002` |

Interpretation:

- `Stop` is a real stop.
- Runnable brakes on macOS are relative priority changes, not hard caps.
- With no stronger competing work, even low runnable brakes still consume a
  large fraction of one core.

### Linux

Command:

```sh
docker run --rm --privileged -v /Users/spence/src/thread-lanes:/work -w /work rust:1.85-bookworm cargo run --example brake_table
```

Results:

| Brake | CPU (s) | Wall (s) |
|-------|---------|----------|
| `Stop` | `0.000` | `1.002` |
| `Background` | `0.100` | `1.000` |
| `Custom(0.25)` | `0.247` | `1.000` |
| `Custom(0.50)` | `0.497` | `1.000` |
| `Custom(0.75)` | `0.749` | `1.000` |
| `Full` | `1.002` | `1.002` |

Interpretation:

- `Stop` is a real stop.
- Runnable brakes on Linux now scale the bucket quota by live thread
  membership, so a single runnable thread sees roughly the requested fraction
  of one core.
- The cap is still enforced at the shared bucket level. With multiple threads in
  the same brake bucket, the total quota scales with membership and the threads
  compete inside that shared bucket.

## Summary

These results show that `Brake::Stop` is portable and strong, but runnable
brake values still do not mean the same thing across platforms:

- macOS: low runnable brakes reduce scheduling priority but still use spare CPU.
- Linux: a single runnable thread now sees roughly the requested fraction of one
  core, but the limit is still enforced as a shared bucket cap when multiple
  threads use the same brake.
- Stronger Linux validation came from the longer hostile-saturation run, which
  measured `FG: 8.96` effective CPUs, `BG: 0.87`, and `BG/FG ratio: 0.0966`
  after the bucket-resize, `cpu.weight`, and thread CPU clock fixes.

That contract gap is tracked in [PROJECT.md](/Users/spence/src/thread-lanes/PROJECT.md#L45).
