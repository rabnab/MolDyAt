#!/usr/bin/env bash
# Production code size of a change: lines added and removed per .rs/.wgsl file
# and in total, without tests (tests/ directories, tests.rs) and fixtures.
# Inline #[cfg(test)] modules still count. Used by CI (job summary) and before
# opening a PR (issue-workflow skill).
#
#   scripts/code_delta.sh         # HEAD against its merge base with origin/master
#   scripts/code_delta.sh BASE    # HEAD against its merge base with BASE
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

base=$(git merge-base "${1:-origin/master}" HEAD)
git diff --numstat "$base" HEAD -- '*.rs' '*.wgsl' \
  ':(exclude,glob)**/tests/**' ':(exclude,glob)**/tests.rs' ':(exclude,glob)**/fixtures/**' |
  awk '{ add += $1; del += $2; n++; printf "  +%-5d -%-5d %s\n", $1, $2, $3 }
       END { printf "production code: +%d -%d, net %+d in %d files\n", add, del, add - del, n }'
