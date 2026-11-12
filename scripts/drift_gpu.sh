#!/usr/bin/env bash
# The drift suite (cargo ci-test-drift) with the pair forces on the GPU
# (#361). See docs/drift-test.md, "On the GPU force pass".
#
# Builds the tests of thermo_physics with `--cfg gpu_drift`, which adds
# thermo_gpu to them, into a target directory of its own, so the normal build
# is not rebuilt. Without a GPU adapter the scenarios run on the CPU kernel
# and say so (DRIFT_FORCES).
#
# Usage: scripts/drift_gpu.sh [TEST_FILTER] [-- TEST_BINARY_ARGS]
set -euo pipefail

RUSTFLAGS="${RUSTFLAGS:-} --cfg gpu_drift" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/gpu-drift}" \
    cargo test -p thermo_physics "$@" -- --ignored --nocapture
