#!/usr/bin/env bash
# Large-world performance scenario for the headless renderer.
#
# The default world holds ~800 balls, which fit in cache and hide memory-access
# costs. This scenario scales the world so the ball data no longer fits, and
# reports the time per physics step with the setup (seeding) time subtracted.
#
# Usage: scripts/headless_perf.sh [BINARY...]
#   Compare builds by passing several binaries, e.g. a baseline and a new one.
#   Default: target/release/rust_thermo_wgpu (build with cargo build --release).
# Environment: SCALE (world scale, default 5 -> ~17k balls), STEPS (default 300),
#   RUNS (default 3, the fastest run is reported), SEED (default 1),
#   BALLS (balls to seed, default 30000; more only makes seeding slower).
set -euo pipefail

SCALE=${SCALE:-5}
STEPS=${STEPS:-300}
RUNS=${RUNS:-3}
SEED=${SEED:-1}
BALLS=${BALLS:-30000} # seeding stops at its attempt limit (4x), so ~17k balls fit at SCALE=5

[ "$#" -gt 0 ] || set -- target/release/rust_thermo_wgpu

# Wall-clock seconds of one headless run.
run_seconds() {
    local bin=$1 steps=$2 start end
    start=$(date +%s.%N)
    "$bin" --headless --steps "$steps" --rng-seed "$SEED" --seed-balls "$BALLS" \
        --world-scale "$SCALE" --stdout >/dev/null 2>&1
    end=$(date +%s.%N)
    echo "$end - $start" | bc -l
}

fastest() {
    local bin=$1 steps=$2 best="" t
    for _ in $(seq "$RUNS"); do
        t=$(run_seconds "$bin" "$steps")
        if [ -z "$best" ] || [ "$(echo "$t < $best" | bc -l)" = 1 ]; then best=$t; fi
    done
    echo "$best"
}

for bin in "$@"; do
    balls=$("$bin" --headless --steps 1 --rng-seed "$SEED" --seed-balls "$BALLS" \
        --world-scale "$SCALE" --stdout 2>/dev/null | grep -o 'balls=[0-9]*' | head -1)
    setup=$(fastest "$bin" 1)
    total=$(fastest "$bin" "$STEPS")
    printf '%s: %s, %s steps, %.1f ms/step (setup %.1f s)\n' "$bin" "$balls" "$STEPS" \
        "$(echo "($total - $setup) * 1000 / ($STEPS - 1)" | bc -l)" "$setup"
done
