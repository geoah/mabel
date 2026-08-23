# 013: Test and build time budgets

- Date: 2026-08-24
- Status: accepted
- Source: product owner

- No test may need a 10-minute timeout. Long timeouts hide hangs; a test
  that slow is a defect.
- Budgets: unit and component tests finish in milliseconds; a networked
  integration test carries an explicit timeout of at most 30 seconds and
  normally finishes in under 5; `cargo test --workspace` stays under 2
  minutes; an end-to-end scenario stays under 60 seconds with per-step
  timeouts.
- Tests never sleep-and-poll on wall time when they can await an event,
  and never talk to the public internet.
- Build commands are scoped and bounded: `cargo build|test|clippy -p
  <crate>` in the inner loop, workspace-wide runs only at commit points,
  timeouts 120 seconds warm and 300 seconds for a known-cold build, never
  `cargo clean`, no `--all-features` sweeps in the inner loop. A command
  that hits its timeout is investigated as a hang or a lock contention,
  not retried with a bigger timeout.
