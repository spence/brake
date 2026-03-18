# Charter

`brake` is a Rust crate for placing threads into built-in execution brakes,
moving them between stop, background, and validated custom CPU fractions at
runtime, and exposing per-thread and per-brake CPU accounting through a small
cross-platform API. Its scope is a cross-platform core library with Linux
cgroup v2 enforcement, macOS Mach priority and suspension support, and
proof-oriented examples that demonstrate behavior under hostile CPU
contention.

# Milestones

## 0.2 Rebrand

- [x] Package metadata, docs, imports, and examples use the `brake` crate
  name.

## 0.2 Concrete Brake API

- [x] Public API uses a built-in `Brake` enum instead of custom lane traits.
- [x] Users can stop threads, use a built-in background brake, and choose a
  validated custom CPU fraction.
- [x] Examples and docs use the brake abstraction end to end.

## 0.2 Measurement Examples

- [x] A single-thread brake sweep reports CPU time versus wall time in a table
  across stop, background, and custom brake values.
- [x] `BENCH.md` records the measured single-thread brake sweep output for
  macOS and Linux.
- [x] README ends with a direct Docker example for running Linux validation
  without a repo-owned Docker harness.
- [x] Each example file starts with a short comment that explains what it
  demonstrates.

## 0.2 Stop Brakes

- [x] Built-in brakes include a cross-platform stop state that pauses threads
  until they are resumed.
- [x] Linux stops threads with frozen cgroups and macOS stops threads with Mach
  thread suspension.
- [x] Verification covers stop/resume behavior on macOS and Linux.

## 0.1 Correctness

- [ ] Single-thread brake values have a consistent documented meaning across
  platforms and CPU counts.
- [x] `brake_stats()` reports accurate thread counts after live brake changes.
- [x] `brake_stats()` avoids backend/thread lock inversion during thread exit.
- [x] Spawned threads enter their requested brake before user code begins
  consuming CPU.
- [x] Linux managers use isolated cgroup roots so concurrent managers do not
  tear down each other's brake hierarchy.
- [x] Linux runnable brake buckets scale quota with live thread membership so a
  single thread sees roughly its requested fraction of one core.
- [x] Linux per-thread CPU accounting uses the kernel thread CPU clock instead
  of coarse tick-based `/proc` task stats.
- [x] Live thread membership stays accurate when threads exit so per-brake
  accounting and any dynamic quota updates remain correct.
- [x] Linux runnable brakes use explicit `cpu.max` settings without idle-only
  special casing.
- [x] Public contract does not imply macOS provides Linux-style hard per-brake
  CPU caps.
- [x] Release validation covers runtime behavior beyond compile-only doc tests
  and long-running manual examples.
