---
name: ci-triage
description: Procedure for diagnosing a failed or warning CI run - finding the failing job, reading the drift, headless-regression and test logs, and telling a real regression from platform differences, a stale fixture or infrastructure trouble. Use when a GitHub Actions run of ci.yml or release.yml fails, shows DRIFT_WARNING annotations, or a PR check is red.
---

# CI Triage

`.github/workflows/ci.yml` runs on every pull request and on pushes to `master`; a newer push cancels the running one. Never "fix" CI by loosening thresholds, refreshing fixtures or retrying until green without understanding the failure.

## Jobs
| Job | Checks | Typical failure |
|---|---|---|
| `fmt` | `cargo fmt -- --check` | run `cargo fmt` |
| `clippy` | `cargo clippy` on the workspace with `-D warnings`, and `-p moldyat_core --all-targets --no-default-features` | warnings, or code that only compiles with `measure-time` |
| `licenses` | `cargo deny check licenses`, `scripts/check_release_files.sh` | new dependency with a license missing from `deny.toml`/`about.toml`, `.rs`/`.wgsl` file without the MPL-2.0 header, stale `Cargo.lock`, `CITATION.cff` version mismatch |
| `test` | `cargo nextest run --workspace --profile ci`, then `cargo ci-test-headless-regression`, the core tests and fixtures with `--no-default-features`, then `cargo ci-test-drift` | see below |

The pinned nightly is set in `rust-toolchain.toml` and mirrored in `RUST_NIGHTLY` in `ci.yml`; a mismatch shows up as different clippy or rustfmt results than locally.

## Getting the facts
```bash
gh pr checks <PR>
gh run list --branch <branch> --limit 5
gh run view <run-id> --log-failed          # only the failing steps
gh run download <run-id> -n drift-harness-log -n headless-regression-log -n test-results
```
Artifacts: `test-results` (JUnit), `headless-regression-log` (on failure), `drift-harness-log` (always). The job summary lists the runner CPU, the SIMD path (compiled `target_feature`s), every `DRIFT_SCENARIO` / `DRIFT_CONVERGENCE` line and every `DRIFT_WARNING`.

## Reading test failures
- **Unit tests (nextest):** reproduce locally with the exact test name: `cargo test -p <package> <test> -- --exact --nocapture`. `retries = 0`, so a failure is not hidden by a retry; a test that fails only sometimes is a bug (usually timing in physics-thread tests or a thread-count dependence), not noise.
- **Headless regression:** the log names the first mismatching JSONL line, expected vs actual. Ask: did this PR intend to change results?
  - No: it is a regression. Reproduce with `cargo ci-test-headless-regression` and bisect the PR's commits.
  - Yes: the fixture must be refreshed in the PR (see the `headless-contract` skill), never on Windows.
  - Only in the `--no-default-features` step: output depends on the `measure-time` feature, which it must not.
- **Drift suite:** each scenario prints a `DRIFT_SCENARIO … result=` line even when it fails; compare its metrics with the table in `docs/drift-test.md`. Exact checks (missed neighbour pairs, thread-count hash) point to a logic bug; a slightly exceeded energy threshold after a kernel or summation-order change points to an intended change that needs new baselines (see the `physics-change` skill).
- **`DRIFT_WARNING` annotations:** a passing metric above 70 % of its threshold. Not a failure, but the PR must explain it if the PR moved it.

## Platform and infrastructure
- f32 results depend on the SIMD target: CI's dev profile on x86_64 uses SSE2; local `release` and `perf` builds use `target-cpu=native`. Compare like with like before suspecting a regression.
- Windows (MSVC) can produce different `sin`/`cos`/`exp` bits; fixture tests may fail there without a behaviour change. CI's test job runs on Linux.
- `gpu_smoke.rs` is skipped on runners without a GPU adapter; a pipeline bug can pass CI.
- Cache or sccache trouble, network errors installing tools, or runner cancellation: rerun with `gh run rerun <run-id> --failed`, once. If it fails the same way again, it is not infrastructure.

## After fixing
Push the fix and watch the new run (`gh run watch`). In the PR, say what failed and why, especially if a fixture or baseline changed.
