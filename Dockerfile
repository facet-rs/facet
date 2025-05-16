# Allow selecting base image at build time
ARG BASE_IMAGE=rust:1.87-slim-bookworm
FROM ${BASE_IMAGE} AS builder

ARG RUSTUP_TOOLCHAIN
ENV RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN}

ENV CI=true

# Accept additional Rust components as a build argument
ARG ADDITIONAL_RUST_COMPONENTS=""
ENV ADDITIONAL_RUST_COMPONENTS=${ADDITIONAL_RUST_COMPONENTS}

# Install Rust components
RUN rustup component add rust-src clippy rustfmt llvm-tools-preview ${ADDITIONAL_RUST_COMPONENTS}

# Set the default toolchain based on build arg and configure rust components
RUN apt-get update && apt-get install -y curl libssl-dev pkg-config && \
    # Install cargo-binstall using curl
    curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash && \
    # Clean up - remove curl and apt cache
    apt-get purge -y curl && \
    apt-get autoremove -y && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Install tools using cargo-binstall
RUN cargo binstall -y just cargo-nextest cargo-llvm-cov

# Add the thumbv8m.main-none-eabihf Rust target
RUN rustup target add thumbv8m.main-none-eabihf

# Set environment variables
ENV CARGO_INCREMENTAL=0
ENV CLICOLOR=1
ENV FORCE_COLOR=1
ENV CLICOLOR_FORCE=1

# Create a work directory
WORKDIR /app

CMD ["bash"]

LABEL org.opencontainers.image.source="https://github.com/facet-rs/facet"
