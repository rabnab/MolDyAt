#!/usr/bin/env bash
# Performance baseline for the headless renderer (#221).
#
# Runs fixed, seeded scenarios and prints one JSON report with the time per
# physics step, split by phase (neighbours, positions, forces, apply, cells,
# thermo, snapshot, output, other). See docs/perf-baseline.md for the
# scenarios, the committed baseline and how to compare a branch against it.
#
# Each scenario's world is seeded once, saved (--save-setup) and cached; every
# run then loads it (--load), so all runs and all binaries start from the same
# state and seeding (which is O(n^2) and takes minutes at 200k balls) is not
# repeated. The binary times the steps itself (--timings), after WARMUP steps
# that are not counted. Binaries alternate within a round, so drift in host
# load hits all of them; the median over the rounds is reported.
#
# Usage: scripts/headless_perf.sh [BINARY...]
#   Compare builds by passing several binaries, e.g. a baseline and a new one.
#   Default: target/release/rust_thermo_wgpu (build with cargo build --release).
# Environment:
#   SCENARIOS       scenarios to run (default "dilute dense walls_valves large")
#   STEPS           steps per run, warmup included (default 300)
#   WARMUP          steps before timing starts (default 50)
#   RUNS            rounds; the median is reported (default 5)
#   SNAPSHOT_EVERY  snapshot cadence in steps (default 10)
#   THREADS         rayon worker threads (default: the binary's, CPU cores - 2)
#   SEED            RNG seed for seeding (default 1)
#   CACHE_DIR       cached seeded worlds (default target/perf-worlds)
#   PROFILE         build profile label (default: taken from the binary path)
#   OUT             write the report here instead of stdout
#
# Needs bash 4+, coreutils, awk and cksum.
set -euo pipefail

SCENARIOS=${SCENARIOS:-dilute dense walls_valves large}
STEPS=${STEPS:-300}
WARMUP=${WARMUP:-50}
RUNS=${RUNS:-5}
SNAPSHOT_EVERY=${SNAPSHOT_EVERY:-10}
THREADS=${THREADS:-}
SEED=${SEED:-1}
CACHE_DIR=${CACHE_DIR:-target/perf-worlds}
OUT=${OUT:-}

[ "$#" -gt 0 ] || set -- target/release/rust_thermo_wgpu

# Seeding arguments per scenario. The default world is 112 x 62; --world-scale
# multiplies both sides. Changing a scenario changes its cache key, but the
# committed baseline must then be recaptured (docs/perf-baseline.md).
scenario_args() {
    case $1 in
    # Hot, sparse gas: many neighbour-list rebuilds, few neighbours each.
    dilute) echo --seed-balls 10000 --world-scale 16 --seed-speed 6 ;;
    # Dense liquid-like packing seeded at rest: many neighbours per ball.
    dense) echo --seed-balls 50000 --world-scale 10 --seed-speed 0 ;;
    # Walls with closed and open valves: wall sweep and overlap passes.
    walls_valves)
        echo --seed-balls 20000 --world-scale 8 --seed-speed 4 \
            --line-segment -150,-240,-150,240,2 --valve 0,0.4,0.6,closed \
            --line-segment 150,-240,150,240,2 --valve 1,0.45,0.55,open \
            --line-segment -140,0,140,0,2 --valve 2,0.3,0.7,open \
            --line-segment -400,-200,-200,-50,2
        ;;
    # Ball data far beyond cache: memory-bound.
    large) echo --seed-balls 200000 --world-scale 28 --seed-speed 2 ;;
    *)
        echo "error: unknown scenario $1" >&2
        return 1
        ;;
    esac
}

for bin in "$@"; do
    [ -x "$bin" ] || { echo "error: $bin is not an executable" >&2; exit 1; }
done

# Fail on a misspelt scenario before seeding or timing anything.
for name in $SCENARIOS; do
    scenario_args "$name" >/dev/null || exit 1
done

run_env=()
[ -z "$THREADS" ] || run_env=(RAYON_NUM_THREADS="$THREADS")

# Path of the cached world of scenario $1, seeded with binary $2 if missing.
scenario_world() {
    local name=$1 bin=$2 args key dir
    args=$(scenario_args "$name") || return 1
    args="$args --rng-seed $SEED"
    key=$(echo "$args" | cksum | cut -d' ' -f1)
    dir="$CACHE_DIR/$name-$key"
    if ! ls "$dir"/*.json >/dev/null 2>&1; then
        echo "seeding $name ($args) ..." >&2
        mkdir -p "$dir"
        # shellcheck disable=SC2086
        env ${run_env[@]+"${run_env[@]}"} "$bin" --headless --steps 1 $args --save-setup \
            --save-dir "$dir" --no-stdout --output /dev/null >/dev/null 2>&1 || {
            echo "error: seeding $name with $bin failed" >&2
            rm -rf "$dir"
            return 1
        }
    fi
    ls "$dir"/*.json | head -1
}

# Build profile of a binary from its path (target/<profile>/...).
profile_of() {
    if [ -n "${PROFILE:-}" ]; then
        echo "$PROFILE"
    else
        echo "$1" | awk -F/ '{ for (i = 1; i < NF; i++) if ($i == "target") { print $(i + 1); exit } }'
    fi
}

json_escape() { printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

for name in $SCENARIOS; do
    world=$(scenario_world "$name" "$1")
    for round in $(seq "$RUNS"); do
        for b in $(seq "$#"); do
            bin=${!b}
            out="$tmp/$name.$b.$round.json"
            env ${run_env[@]+"${run_env[@]}"} "$bin" --headless --load "$world" --steps "$STEPS" \
                --timing-warmup "$WARMUP" --snapshot-every "$SNAPSHOT_EVERY" \
                --timings "$out" --no-stdout --output /dev/null >/dev/null 2>&1 || {
                echo "error: $bin failed on $name" >&2
                exit 1
            }
        done
        echo "$name: round $round/$RUNS done" >&2
    done
done

cpu=$(awk -F': *' '/^model name/ { print $2; exit }' /proc/cpuinfo 2>/dev/null || true)
[ -n "$cpu" ] || cpu=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo unknown)
cores=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 0)

report() {
    printf '{\n  "schema_version": "headless-perf.v2",\n'
    printf '  "date": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '  "machine": {"cpu": "%s", "cores": %s, "os": "%s"},\n' \
        "$(json_escape "$cpu")" "$cores" "$(json_escape "$(uname -sr)")"
    printf '  "settings": {"steps": %s, "warmup": %s, "runs": %s, "snapshot_every": %s, "seed": %s},\n' \
        "$STEPS" "$WARMUP" "$RUNS" "$SNAPSHOT_EVERY" "$SEED"
    printf '  "results": [\n'
    local first=1 name b bin
    for name in $SCENARIOS; do
        for b in $(seq "$#"); do
            bin=${!b}
            [ "$first" = 1 ] || printf ',\n'
            first=0
            cat "$tmp/$name.$b".*.json | awk -v scenario="$name" \
                -v bin="$(json_escape "$bin")" -v profile="$(profile_of "$bin")" '
                function num(key,   m) {
                    if (!match($0, "\"" key "\":[-0-9.eE+]+")) return 0
                    m = substr($0, RSTART, RLENGTH)
                    sub(/^[^:]*:/, "", m)
                    return m + 0
                }
                function str(key,   m) {
                    if (!match($0, "\"" key "\":\"[^\"]*\"")) return "unknown"
                    m = substr($0, RSTART, RLENGTH)
                    sub(/^[^:]*:"/, "", m)
                    sub(/"$/, "", m)
                    return m
                }
                function median(a, n,   i, j, t, s) {
                    for (i = 1; i <= n; i++) s[i] = a[i]
                    for (i = 2; i <= n; i++)
                        for (j = i; j > 1 && s[j - 1] > s[j]; j--) { t = s[j]; s[j] = s[j - 1]; s[j - 1] = t }
                    lo = s[1]; hi = s[n]
                    return (n % 2) ? s[(n + 1) / 2] : (s[n / 2] + s[n / 2 + 1]) / 2
                }
                BEGIN { np = split("total neighbours positions forces apply cells thermo snapshot output other", phase, " ") }
                {
                    runs++
                    commit = str("commit")
                    balls = num("balls"); threads = num("threads"); steps = num("steps")
                    walls = num("wall_pieces"); valves = num("valves")
                    for (p = 1; p <= np; p++) v[p, runs] = num(phase[p])
                }
                END {
                    printf "    {\"scenario\": \"%s\", \"binary\": \"%s\", \"commit\": \"%s\", \"profile\": \"%s\", ", scenario, bin, commit, profile
                    printf "\"balls\": %d, \"wall_pieces\": %d, \"valves\": %d, \"threads\": %d, \"steps\": %d, \"runs\": %d,\n", \
                        balls, walls, valves, threads, steps, runs
                    printf "     \"ms_per_step\": {"
                    for (p = 1; p <= np; p++) {
                        for (r = 1; r <= runs; r++) a[r] = v[p, r]
                        med = median(a, runs)
                        spread = (med > 0) ? (hi - lo) / med * 100 : 0
                        printf "%s\n       \"%s\": {\"median\": %.4f, \"min\": %.4f, \"max\": %.4f, \"spread_pct\": %.1f}", \
                            (p > 1 ? "," : ""), phase[p], med, lo, hi, spread
                    }
                    printf "\n     }}"
                }'
        done
    done
    printf '\n  ]\n}\n'
}

if [ -n "$OUT" ]; then
    report "$@" >"$OUT"
    echo "report written to $OUT" >&2
else
    report "$@"
fi
