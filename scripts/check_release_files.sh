#!/usr/bin/env bash
# Checks the licensing and release metadata files for consistency.
# Run from anywhere; used by CI (.github/workflows/ci.yml) and before releases.
#
#   scripts/check_release_files.sh            # all checks
#   TAG=v1.4.0 scripts/check_release_files.sh # also: tag matches Cargo.toml version
#
# cargo-about is needed for the THIRD_PARTY_LICENSES.md freshness check; without
# it that one check is skipped locally (CI installs it and fails if missing).
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

fail=0
err() { echo "::error::$*"; fail=1; }
ok() { echo "ok: $*"; }

version=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
license=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^license *= *"\(.*\)"/\1/p' Cargo.toml)
[ -n "$version" ] || err "no version in [workspace.package] of Cargo.toml"
[ -n "$license" ] || err "no license in [workspace.package] of Cargo.toml"

# 1. Cargo.lock matches the manifests (release builds use --locked).
if cargo metadata --locked --format-version 1 >/dev/null 2>&1; then
  ok "Cargo.lock is up to date"
else
  err "Cargo.lock is out of date: run 'cargo update -w' and commit it"
fi

# 2. Release tag matches the workspace version.
if [ -n "${TAG:-}" ]; then
  if [ "${TAG#v}" = "$version" ]; then ok "tag $TAG matches version $version"
  else err "tag $TAG does not match Cargo.toml version $version"; fi
fi

# 3. CITATION.cff agrees with Cargo.toml.
cff_version=$(sed -n 's/^version: *"\{0,1\}\([^"]*\)"\{0,1\} *$/\1/p' CITATION.cff)
cff_license=$(sed -n 's/^license: *\(.*[^ ]\) *$/\1/p' CITATION.cff)
[ "$cff_version" = "$version" ] && ok "CITATION.cff version $version" \
  || err "CITATION.cff version '$cff_version' != Cargo.toml version '$version'"
[ "$cff_license" = "$license" ] && ok "CITATION.cff license $license" \
  || err "CITATION.cff license '$cff_license' != Cargo.toml license '$license'"

# 4. Every crate inherits the workspace license.
for toml in core/Cargo.toml wgpu_renderer/Cargo.toml; do
  grep -q '^license\.workspace *= *true' "$toml" \
    || err "$toml must contain 'license.workspace = true'"
done

# 5. LICENSE file is the MPL-2.0 text (SPDX id MPL-2.0).
if [ "$license" = "MPL-2.0" ] && head -n 1 LICENSE | grep -q 'Mozilla Public License Version 2.0'; then
  ok "LICENSE is MPL-2.0"
else
  err "LICENSE does not match license '$license' in Cargo.toml"
fi

# 6. Every .rs and .wgsl file carries the MPL-2.0 notice in its first lines.
missing=$(git ls-files -z '*.rs' '*.wgsl' | xargs -0 -I{} sh -c \
  'head -n 3 "{}" | grep -q "Mozilla Public" || echo "{}"')
if [ -z "$missing" ]; then ok "all .rs/.wgsl files have the license header"
else err "missing MPL-2.0 header in:"$'\n'"$(head -n 20 <<<"$missing")"; fi

# 7. License allow lists of deny.toml and about.toml are identical.
extract() { tr -d '\n' <"$1" | grep -o "$2 *= *\[[^]]*\]" | grep -o '"[^"]*"' | sort; }
if [ "$(extract deny.toml allow)" = "$(extract about.toml accepted)" ] \
   && [ -n "$(extract deny.toml allow)" ]; then
  ok "deny.toml allow == about.toml accepted"
else
  err "deny.toml 'allow' and about.toml 'accepted' differ: keep them in sync"
fi

# 8. THIRD_PARTY_LICENSES.md is current (regenerate with cargo-about).
if command -v cargo-about >/dev/null 2>&1; then
  generated=$(mktemp)
  if cargo about generate about.hbs -o "$generated" >/dev/null 2>&1 \
     && sed -i 's/\r$//' "$generated" \
     && diff -u THIRD_PARTY_LICENSES.md "$generated" >"$generated.diff"; then
    ok "THIRD_PARTY_LICENSES.md is up to date"
  else
    head -n 40 "$generated.diff" 2>/dev/null
    err "THIRD_PARTY_LICENSES.md is stale: run" \
        "'cargo about generate about.hbs -o THIRD_PARTY_LICENSES.md && sed -i \"s/\\r\$//\" THIRD_PARTY_LICENSES.md'"
  fi
  rm -f "$generated" "$generated.diff"
else
  echo "skipped: cargo-about not installed (cargo install cargo-about --locked --features cli)"
  [ -n "${CI:-}" ] && err "cargo-about is required in CI"
fi

exit $fail
