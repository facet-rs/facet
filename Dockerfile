# Allow selecting base image at build time
ARG BASE_IMAGE=rust:1.86-slim-bullseye
FROM ${BASE_IMAGE} AS builder

ARG RUSTUP_TOOLCHAIN
ENV RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN}

ENV CI=true

# Accept additional Rust components as a build argument
ARG ADDITIONAL_RUST_COMPONENTS=""
ENV ADDITIONAL_RUST_COMPONENTS=${ADDITIONAL_RUST_COMPONENTS}

# Set the default toolchain based on build arg and configure rust components
RUN apt-get update && apt-get install -y curl && \
    rustup component add rust-src clippy rustfmt ${ADDITIONAL_RUST_COMPONENTS} && \
    # Install cargo-binstall, then use it to install just and cargo-nextest
    curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash && \
    cargo binstall -y just cargo-nextest && \
    # Clean up - remove curl and apt cache
    apt-get purge -y curl && \
    apt-get autoremove -y && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Set environment variables
ENV CARGO_INCREMENTAL=0

# Create a work directory
WORKDIR /app

CMD ["bash"]

LABEL org.opencontainers.image.source="https://github.com/facet-rs/facet"
