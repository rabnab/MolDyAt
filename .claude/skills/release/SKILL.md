---
name: release
description: Procedure for cutting a MolDyAt release - choosing the version, bumping Cargo.toml and CITATION.cff, running the release checks, tagging, and verifying the binaries that release.yml publishes. Use when the user asks to release, tag a version, publish binaries, or prepare a release candidate.
---

# Release

Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed (or by `workflow_dispatch` with an existing tag). It builds the `dist` profile (portable `x86-64-v2`, not `native`) for `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc`, packages the `moldyat` binary with `README.md`, `LICENSE` and generated `THIRD_PARTY_LICENSES.md`, adds `.sha256` files, and creates the GitHub release with generated notes. Tags with a SemVer suffix (`v0.3.0-rc.1`) become pre-releases.

Tagging and pushing are outward-facing: confirm the version and the commit with the user before creating or pushing a tag.

## 1. Choose the version
SemVer on the workspace version (`[workspace.package] version` in the root `Cargo.toml`, inherited by both crates):
- patch: fixes only, no change to save files or headless output;
- minor: new features, compatible save files (old saves load) and `headless.v1` output;
- major (after 1.0): incompatible changes. Before 1.0, incompatible changes bump the minor version.
Mention save-format version bumps and `headless` schema changes in the release notes.

## 2. Prepare on a branch
```bash
git switch master && git pull --rebase
git switch -c feature/master/release-vX.Y.Z
```
- Set `version` in the root `Cargo.toml`.
- Set `version` (and `date-released`, if present) in `CITATION.cff`.
- `cargo update -w` so `Cargo.lock` records the new version (release builds use `--locked`).
- Check that README and docs mention no unreleased flags as missing, and no removed ones as present.

## 3. Check
```bash
TAG=vX.Y.Z scripts/check_release_files.sh   # lock file, tag = version, CITATION, licenses, MPL headers, notices
cargo deny check licenses
cargo fmt -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo ci-test-headless-regression
cargo ci-test-drift
cargo build --locked -p moldyat_wgpu --profile dist
target/dist/moldyat --version              # shows the version and the git commit
```
Open the PR, get it reviewed and merged with green CI (see the `plan-item-workflow` skill for the PR steps).

## 4. Tag and publish
After the merge, on the merge commit of `master`:
```bash
git switch master && git pull --rebase
git tag -a vX.Y.Z -m "MolDyAt vX.Y.Z"
git push origin vX.Y.Z
gh run watch $(gh run list --workflow release.yml --limit 1 --json databaseId -q '.[0].databaseId')
```
The workflow's first job reruns `check_release_files.sh` with the tag and fails if the tag and `Cargo.toml` disagree.

## 5. Verify
- `gh release view vX.Y.Z`: two archives (`.tar.gz` for Linux, `.zip` for Windows) and their `.sha256` files.
- Download one, check the checksum, run `moldyat --version` and a short headless run (`--headless --steps 100 --rng-seed 1`).
- Edit the generated notes if needed: highlights, save-format or headless schema changes, known issues.

## Fixing a release
- A broken build for an existing tag: fix on `master`, then rebuild with `gh workflow run release.yml -f tag=vX.Y.Z` only if the tag's commit is still right; the workflow re-uploads the assets with `--clobber`.
- A wrong commit or version: do not move a published tag. Release a new patch version instead.
