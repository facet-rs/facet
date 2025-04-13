default:
    just precommit
    just prepush

precommit:
    just genfmt

prepush:
    just clippy
    just test

ci:
    just precommit
    just prepush
    just docs
    just msrv
    just miri

genfmt:
    just lockfile
    just codegen
    cargo fmt --all
    just absolve

nostd:
    # Run alloc but no-std checks with specified target directory
    cargo check --no-default-features -p facet-core --target-dir target/nostd
    cargo check --no-default-features -p facet --target-dir target/nostd
    cargo check --no-default-features -p facet-reflect --target-dir target/nostd

    # Run alloc but no-std checks with specified target directory
    cargo check --no-default-features --features alloc -p facet-core --target-dir target/nostd-w-alloc
    cargo check --no-default-features --features alloc -p facet --target-dir target/nostd-w-alloc
    cargo check --no-default-features --features alloc -p facet-reflect --target-dir target/nostd-w-alloc

nostd-ci:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc

    # Set up target directory for no-std checks
    export CARGO_TARGET_DIR=target/nostd

    # Run each check in its own group with the full command as the title
    cmd_group "cargo check --no-default-features -p facet-core"
    cmd_group "cargo check --no-default-features -p facet"
    cmd_group "cargo check --no-default-features -p facet-reflect"

    # Set up target directory for alloc but no-std checks
    export CARGO_TARGET_DIR=target/nostd-w-alloc

    # Run each check in its own group with the full command as the title
    cmd_group "cargo check --no-default-features --features alloc -p facet-core"
    cmd_group "cargo check --no-default-features --features alloc -p facet"
    cmd_group "cargo check --no-default-features --features alloc -p facet-reflect"

clippy-all:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test *args:
    cargo nextest run {{args}} < /dev/null
    cargo test --doc {{args}}

test-ci *args:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;33m🏃 Running all but doc-tests with nextest...\033[0m"
    cmd_group "cargo nextest run {{args}} < /dev/null"

    echo -e "\033[1;36m📚 Running documentation tests...\033[0m"
    cmd_group "cargo test --doc {{args}}"

doc-tests *args:
    cargo test --doc {{args}}

doc-tests-ci *args:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;36m📚 Running documentation tests...\033[0m"
    cmd_group "cargo test --doc {{args}}"

codegen *args:
    cargo run -p facet-codegen -- {{args}}

code-quality:
    just codegen --check
    cargo fmt --check --all
    just absolve

code-quality-ci:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    cmd_group "just codegen --check"
    cmd_group "cargo fmt --check --all"
    cmd_group "just absolve"

miri *args:
    export RUSTUP_TOOLCHAIN=nightly-2025-04-05
    rustup toolchain install nightly-2025-04-05
    rustup +nightly-2025-04-05 component add miri rust-src
    cargo +nightly-2025-04-05 miri nextest run --target-dir target/miri {{args}}

miri-ci *args:
    #!/usr/bin/env -S bash -euxo pipefail
    source .envrc
    echo -e "\033[1;31m🧪 Running tests under Miri...\033[0m"

    export CARGO_TARGET_DIR=target/miri
    cmd_group "cargo miri nextest run {{args}}"

absolve:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    if ! cargo tree -i syn 2>/dev/null | grep -q .; then
    echo -e "\033[38;2;255;255;255;48;2;0;0;0m free of \033[38;2;255;255;255;48;2;255;105;180m syn \033[38;2;255;255;255;48;2;0;0;0m\033[0m"
    else
    echo -e "\033[1;31m❌ 'syn' found in dependency tree. Here's what's using 'syn':\033[0m"
    cargo tree -i syn -e features
    exit 1
    fi

ship:
    #!/usr/bin/env -S bash -eux
    release-plz update
    git add .
    git commit -m "⬆️ crate upgrades"
    git push
    just release

release:
    source .envrc
    cargo ws publish --publish-as-is --allow-dirty

docsrs *args:
    #!/usr/bin/env -S bash -eux
    source .envrc
    export RUSTDOCFLAGS="--cfg docsrs"
    cargo +nightly doc {{args}}

msrv:
    cargo hack check --feature-powerset --locked --rust-version --ignore-private --workspace --all-targets --keep-going --exclude-no-default-features

docs:
    cargo doc --workspace --all-features --no-deps --document-private-items --keep-going

lockfile:
    cargo update --workspace --locked

docker-build-push:
    #!/usr/bin/env -S bash -eu
    source .envrc
    echo -e "\033[1;34m🐳 Building and pushing Docker images for CI...\033[0m"

    # Set variables
    IMAGE_NAME="ghcr.io/facet-rs/facet-ci"
    TAG="$(date +%Y%m%d)-$(git rev-parse --short HEAD)"

    # Build tests image using stable Rust
    echo -e "\033[1;36m🔨 Building tests image with stable Rust...\033[0m"
    docker build \
        --build-arg BASE_IMAGE=rust:1.86-slim-bullseye \
        --build-arg RUSTUP_TOOLCHAIN=1.86 \
        -t "${IMAGE_NAME}:${TAG}" \
        -t "${IMAGE_NAME}:latest" \
        -f Dockerfile \
        .

    # Build miri image using nightly Rust
    echo -e "\033[1;36m🔨 Building miri image with nightly Rust...\033[0m"
    docker build \
        --build-arg BASE_IMAGE=rustlang/rust:nightly-slim \
        --build-arg RUSTUP_TOOLCHAIN=nightly \
        --build-arg ADDITIONAL_RUST_COMPONENTS="miri" \
        -t "${IMAGE_NAME}:${TAG}-miri" \
        -t "${IMAGE_NAME}:latest-miri" \
        -f Dockerfile \
        .

    # Push all tags
    echo -e "\033[1;35m🚀 Pushing all image tags...\033[0m"
    docker push "${IMAGE_NAME}:${TAG}"
    docker push "${IMAGE_NAME}:latest"
    docker push "${IMAGE_NAME}:${TAG}-miri"
    docker push "${IMAGE_NAME}:latest-miri"
