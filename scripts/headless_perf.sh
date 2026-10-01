#!/usr/bin/env bash
# Large-world performance scenario for the headless renderer.
#
# The default world holds ~800 balls, which fit in cache and hide memory-access
# costs. This scenario scales the world so the ball data no longer fits, and
# reports the time per physics step with the setup (seeding) time subtracted.
#
# Each round runs every binary once with 1 step and once with STEPS steps, back
# to back, and takes the difference, so setup cancels within the same pair.
# Binaries alternate within a round, so drift in host load hits all of them;
# the median over the rounds is reported.
#
# Usage: scripts/headless_perf.sh [BINARY...]
#   Compare builds by passing several binaries, e.g. a baseline and a new one.
#   Default: target/release/rust_thermo_wgpu (build with cargo build --release).
# Environment: SCALE (world scale, default 5 -> ~17k balls), STEPS (default 300),
#   RUNS (rounds, default 5, the median is reported), SEED (default 1),
#   BALLS (balls to seed, default 30000; more only makes seeding slower),
#   SPEED (seeded balls get speeds uniform in 0..SPEED, default 0 = at rest).
#
# Hot, sparse gas (many neighbour-list rebuilds): SCALE=8 BALLS=12000 SPEED=6
#
# Needs only bash, coreutils (date, sort) and awk.
set -euo pipefail

SCALE=${SCALE:-5}
STEPS=${STEPS:-300}
RUNS=${RUNS:-5}
SEED=${SEED:-1}
SPEED=${SPEED:-0}
BALLS=${BALLS:-30000} # seeding stops at its attempt limit (4x), so ~17k balls fit at SCALE=5

[ "$#" -gt 0 ] || set -- target/release/rust_thermo_wgpu

run_args() {
    echo --headless --steps "$1" --rng-seed "$SEED" --seed-balls "$BALLS" \
        --world-scale "$SCALE" --seed-speed "$SPEED" --stdout
}

# Wall-clock nanoseconds of one headless run with $2 steps; fails if the run fails
# (e.g. "illegal instruction" from a target-cpu=native build on another CPU).
run_ns() {
    local bin=$1 steps=$2 start end
    start=$(date +%s%N)
    # shellcheck disable=SC2046
    if ! "$bin" $(run_args "$steps") >/dev/null 2>&1; then
        echo "error: $bin failed with --steps $steps" >&2
        return 1
    fi
    end=$(date +%s%N)
    echo $((end - start))
}

declare -A samples
for bin in "$@"; do
    [ -x "$bin" ] || { echo "error: $bin is not an executable" >&2; exit 1; }
    samples[$bin]=""
done

for round in $(seq "$RUNS"); do
    for bin in "$@"; do
        setup=$(run_ns "$bin" 1)
        total=$(run_ns "$bin" "$STEPS")
        samples[$bin]+="$((total - setup)) "
    done
    echo "round $round/$RUNS done" >&2
done

for bin in "$@"; do
    # shellcheck disable=SC2046
    balls=$("$bin" $(run_args 1) 2>/dev/null | grep -o 'balls=[0-9]*' | head -1)
    echo "${samples[$bin]}" | tr ' ' '\n' | grep -v '^$' | sort -n | awk \
        -v bin="$bin" -v balls="$balls" -v steps="$STEPS" '
        { v[NR] = $1 }
        END {
            per_step = (steps - 1) * 1e6
            med = (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2
            printf "%s: %s, %d steps, median %.2f ms/step (min %.2f, max %.2f, %d rounds)\n",
                bin, balls, steps, med / per_step, v[1] / per_step, v[NR] / per_step, NR
        }'
done
