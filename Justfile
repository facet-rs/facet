# Just is a task runner, like Make but without the build system / dependency tracking part.
# docs: https://github.com/casey/just
#
# The `-ci` variants are ran in CI, they do command grouping on GitHub Actions, set consistent env vars etc.,
# but they require bash.
#
# The non`-ci` variants can be run locally without having bash installed.

default: precommit prepush

precommit: code-quality
prepush: clippy test

ci: precommit prepush docs msrv miri

nostd:
    # Run alloc but no-std checks with specified target directory
    cargo check --no-default-features -p facet-core --target-dir target/nostd
    cargo check --no-default-features -p facet --target-dir target/nostd
    cargo check --no-default-features -p facet-reflect --target-dir target/nostd

    # Run alloc but no-std checks with specified target directory
    cargo check --no-default-features --features alloc -p facet-core --target-dir target/nostd-w-alloc
    cargo check --no-default-features --features alloc -p facet --target-dir target/nostd-w-alloc
    cargo check --no-default-features --features alloc -p facet-reflect --target-dir target/nostd-w-alloc
    cargo check --no-default-features --features alloc -p facet-json --target-dir target/nostd-w-alloc

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

gen *args:
    cargo run -p facet-dev gen -- {{args}}

code-quality: gen
    just absolve

code-quality-ci:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    cmd_group "just gen --check"
    cmd_group "cargo fmt --check --all"
    cmd_group "just absolve"

miri *args:
    export RUSTUP_TOOLCHAIN=nightly-2025-04-05
    rustup toolchain install nightly-2025-04-05
    rustup +nightly-2025-04-05 component add miri rust-src
    cargo +nightly-2025-04-05 miri nextest run --target-dir target/miri -p facet-reflect {{args}}

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
        --build-arg BASE_IMAGE=rust:1.86-slim-bookworm \
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

docker-build-push-linux-amd64:
    #!/usr/bin/env -S bash -eu
    source .envrc
    echo -e "\033[1;34m🐳 Building and pushing Docker images for CI...\033[0m"

    # Set variables
    IMAGE_NAME="ghcr.io/facet-rs/facet-ci"
    TAG="$(date +%Y%m%d)-$(git rev-parse --short HEAD)"

    # Build tests image using stable Rust
    echo -e "\033[1;36m🔨 Building tests image with stable Rust...\033[0m"
    docker build \
        --platform linux/amd64 \
        --build-arg BASE_IMAGE=rust:1.86-slim-bookworm \
        --build-arg RUSTUP_TOOLCHAIN=1.86 \
        -t "${IMAGE_NAME}:${TAG}-amd64" \
        -t "${IMAGE_NAME}:latest-amd64" \
        -f Dockerfile \
        .

    # Build miri image using nightly Rust
    echo -e "\033[1;36m🔨 Building miri image with nightly Rust...\033[0m"
    docker build \
        --platform linux/amd64 \
        --build-arg BASE_IMAGE=rustlang/rust:nightly-slim \
        --build-arg RUSTUP_TOOLCHAIN=nightly \
        --build-arg ADDITIONAL_RUST_COMPONENTS="miri" \
        -t "${IMAGE_NAME}:${TAG}-miri-amd64" \
        -t "${IMAGE_NAME}:latest-miri-amd64" \
        -f Dockerfile \
        .

    # Push all tags
    echo -e "\033[1;35m🚀 Pushing all image tags...\033[0m"
    docker push "${IMAGE_NAME}:${TAG}-amd64"
    docker push "${IMAGE_NAME}:latest-amd64"
    docker push "${IMAGE_NAME}:${TAG}-miri-amd64"
    docker push "${IMAGE_NAME}:latest-miri-amd64"
