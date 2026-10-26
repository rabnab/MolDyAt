---
name: dependency-update
description: Procedure for updating Rust dependencies, the pinned nightly toolchain or GitHub Actions - reviewing Dependabot PRs, major upgrades (wgpu, winit, rand), cargo update, and checking that simulation results, licenses and release builds are unaffected. Use when handling a Dependabot PR, bumping a crate or the toolchain, or adding a new dependency.
---

# Dependency Updates

Dependabot (`.github/dependabot.yml`) opens monthly, grouped PRs: one for cargo minor and patch updates (`cargo-minor-and-patch`), separate PRs for major versions, and one for GitHub Actions. Security updates come separately from the repository settings.

## What an update can break here
| Dependency | Risk | Check |
|---|---|---|
| `rand`, `rand_distr` | seeded runs place and accelerate balls differently: every fixture and drift baseline moves | `cargo ci-test-headless-regression`, `cargo ci-test-drift`; a change here is a behaviour change (see `physics-change`) |
| `rayon` | scheduling must not affect results | `cargo test --workspace thread_count` |
| `wgpu`, `winit` | API changes in the backend; surface, present mode and event handling | build, `cargo test -p moldyat_wgpu`, run `gpu_smoke.rs` on a machine with a GPU, start the app and resize the window |
| `winit` | `thermo_ui` and `wgpu_renderer` must use the same `winit` version (the energy window is created in `thermo_ui`, its surface in the binary) | versions in both `Cargo.toml`s match |
| `serde`, `serde_json` | save file format | `cargo test -p thermo_physics persistence` |
| `env_logger`, `log` | log filters and targets | `RUST_LOG` targets still work |
| any new crate | license, release build | `cargo deny check licenses`, `scripts/check_release_files.sh` |
| nightly toolchain | `std::simd` API, new clippy lints, rustfmt changes; SIMD codegen can change f32 results | full gates below |

## Procedure
1. Read the changelog of every major update and of anything numeric (`rand`, `wgpu`).
2. Update:
   - one crate: `cargo update -p <crate>` (or edit the version in `Cargo.toml` for a major bump);
   - toolchain: change `channel` in `rust-toolchain.toml` **and** `RUST_NIGHTLY` in `.github/workflows/ci.yml` (they must match).
3. Fix API changes. For `wgpu`, keep resource creation inside `gpu/batch.rs` (see the `render-pipeline` skill).
4. Run the gates:
   ```bash
   cargo fmt -- --check
   cargo clippy --workspace -- -D warnings
   cargo clippy -p thermo_physics -p thermo_runtime -p thermo_ui --all-targets --no-default-features -- -D warnings
   cargo test --workspace
   cargo ci-test-headless-regression
   cargo ci-test-drift
   cargo build --release -p moldyat_wgpu   # release profile uses target-cpu=native
   scripts/check_release_files.sh
   ```
   The release profile disables AMX (`-C target-feature=-amx-tile`) because some dependencies fail to compile AMX intrinsics with `native`; keep that flag when touching `[profile.release]`.
5. If fixtures or drift baselines move, the update changes behaviour: do not merge it as a routine bump. Find out why (changelog, `rand` stream changes, SIMD codegen), then follow the `physics-change` skill for new baselines and fixtures, or pin the old version.
6. For a hot-path dependency (`rayon`, toolchain), compare performance with the `perf-compare` skill.

## New dependencies
- Prefer the standard library or an existing dependency. Physics stays free of windowing and GPU crates (plan direction for `thermo_physics`).
- The license must be in the allow lists of both `deny.toml` and `about.toml` (they must be identical; `check_release_files.sh` checks it).
- Release builds use `--locked`: commit the updated `Cargo.lock`.

## Dependabot PRs
- Check out the PR branch, run the gates above, and read the CI drift summary.
- A grouped minor/patch PR that moves fixtures: split it (`cargo update -p` one crate at a time) to find the culprit.
- GitHub Actions bumps: check the action's changelog for changed inputs (upload/download-artifact major versions change artifact naming and merging) and that `release.yml` still works (it runs on tags only; use `workflow_dispatch` with an existing tag to test).
