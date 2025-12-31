#!/usr/bin/env bash
set -euo pipefail

log() {
	echo "[post-create] $*"
}

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
	SUDO="sudo"
fi

# Ensure nightly toolchain is present and set as default
if ! rustup toolchain list | grep -q '^nightly'; then
	log "Installing nightly toolchain"
	rustup toolchain install nightly --profile minimal
fi
log "Setting default toolchain to nightly"
rustup default nightly

# Required components on nightly
for component in rustfmt clippy; do
	log "Ensuring $component on nightly"
	rustup component add --toolchain nightly "$component"
done

# rust-analyzer is sometimes missing on nightly; fall back to upstream binary
install_rust_analyzer() {
	if rustup component add --toolchain nightly rust-analyzer; then
		return 0
	fi
	log "Nightly rust-analyzer unavailable; installing upstream binary"
	tmpdir="$(mktemp -d)"
	trap 'rm -rf "$tmpdir"' EXIT
	curl -fsSL https://github.com/rust-lang/rust-analyzer/releases/latest/download/rust-analyzer-x86_64-unknown-linux-gnu.gz \
		| gzip -d >"$tmpdir/rust-analyzer"
	chmod +x "$tmpdir/rust-analyzer"
	$SUDO install -m 755 "$tmpdir/rust-analyzer" /usr/local/bin/rust-analyzer
}

log "Ensuring rust-analyzer availability"
install_rust_analyzer

packages=(libudev-dev libxcb-shape0-dev libxcb-xfixes0-dev)
missing_pkgs=()
for pkg in "${packages[@]}"; do
	if ! dpkg -s "$pkg" >/dev/null 2>&1; then
		missing_pkgs+=("$pkg")
	fi
done

if [ ${#missing_pkgs[@]} -gt 0 ]; then
	log "Installing apt packages: ${missing_pkgs[*]}"
	$SUDO apt-get update -y
	$SUDO apt-get install -y --no-install-recommends "${missing_pkgs[@]}"
	$SUDO rm -rf /var/lib/apt/lists/*
else
	log "All apt packages already present"
fi
