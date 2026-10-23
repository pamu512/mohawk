# mohawk-fsrs

FSRS-6 memory-state math used by Mohawk: stability (`S`) and difficulty (`D`).

## Interval clamp

Scheduled intervals are whole days:

`raw_days.round().clamp(1.0, maximum_interval)`

- **1-day floor.** Values that round below 1 become 1 day. This crate does not schedule sub-day intervals.
- **`maximum_interval` cap.** Values that round above `FsrsParams::maximum_interval` become that max (default 36_500 days).

## What is — and is not — claimed vs py-fsrs

Memory-state (`S`/`D`) trajectories match py-fsrs 6.3.2 (`enable_fuzzing=False`). That is the contract; see `parity_with_py_fsrs_reference_trajectory`.

**Intervals are not in that contract.** py-fsrs learning/relearning steps are minute-scale (for example 10 minutes). This crate does not implement those steps and does not claim interval parity with py-fsrs.
