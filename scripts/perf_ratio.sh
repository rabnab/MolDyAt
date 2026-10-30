#!/usr/bin/env bash
# Compares the two binaries of a headless_perf.sh report (headless-perf.v2):
# one line per scenario and phase with the base median, the new median and the
# change in percent (ms per step).
#
# Usage: scripts/perf_ratio.sh perf.json
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 REPORT.json" >&2
  exit 2
fi

jq -r '.results | group_by(.scenario)[] | .[0] as $a | .[1] as $b
  | $a.ms_per_step | keys_unsorted[] as $p
  | "\($a.scenario)\t\($p)\t\($a.ms_per_step[$p].median)\t\($b.ms_per_step[$p].median)\t\(($b.ms_per_step[$p].median / ($a.ms_per_step[$p].median + 1e-12) * 100 - 100) | floor)%"' "$1"
