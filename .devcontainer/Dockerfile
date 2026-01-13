FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Install ggez runtime dependencies
# These are the libraries ggez needs for graphics, audio, and input
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y --no-install-recommends \
    # Required for ggez graphics (via gfx-rs/wgpu)
    libudev-dev \
    # X11 dependencies for window management
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libxkbcommon-dev \
    # Audio dependencies (via rodio)
    libasound2-dev \
    # Additional graphics dependencies
    pkg-config \
    libssl-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Switch to vscode user for any user-level setup
USER vscode

# Pre-install nightly toolchain if you need it
# Remove these lines if you only need stable Rust
RUN rustup toolchain install nightly --profile minimal \
    && rustup default nightly