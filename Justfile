quickcheck:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    just veryquickcheck
    just clippy
    just test
    just nostd
    just doc-tests

veryquickcheck:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    if [[ -z "${CI:-}" ]]; then
        just codegen
        echo -e "\033[1;34m📝 Fixing code formatting...\033[0m"
        cargo fmt --all
    else
        just codegen-check
        just rustfmt
    fi
    just absolve

nostd:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;33m🧪 Checking without std...\033[0m"
    export CARGO_TARGET_DIR=target/nostd
    cargo check --no-default-features -p facet-core
    cargo check --no-default-features -p facet
    cargo check --no-default-features -p facet-peek
    cargo check --no-default-features --features alloc -p facet-core
    cargo check --no-default-features --features alloc -p facet
    cargo check --no-default-features --features alloc -p facet-peek
    cargo check --no-default-features --features alloc -p facet-poke

ci:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    just quickcheck
    just miri
    echo -e "\033[1;34m📝 Running cargo fmt in check mode...\033[0m"
    cargo fmt --all -- --check

rustfmt:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;34m📝 Checking code formatting...\033[0m"
    cargo fmt --all -- --check

clippy:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;35m🔍 Running Clippy on all targets...\033[0m"
    cargo clippy --all-targets -- -D warnings

test *args:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;33m🏃 Running all but doc-tests with nextest...\033[0m"
    cargo nextest run {{args}} < /dev/null
    echo -e "\033[1;33m✅ Good good!\033[0m"

doc-tests:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;36m📚 Running documentation tests...\033[0m"
    cargo test --doc

codegen *args:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    cargo run -p facet-codegen -- {{args}}

codegen-check:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    cargo run -p facet-codegen -- --check

rustfmt-fix:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;34m📝 Fixing code formatting...\033[0m"
    cargo fmt --all

miri *args:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;31m🧪 Running tests under Miri...\033[0m"

    export CARGO_TARGET_DIR=target/miri
    if [[ -z "${CI:-}" ]]; then
        export RUSTUP_TOOLCHAIN=nightly-2025-04-05
        rustup toolchain install
        rustup component add miri rust-src
    fi
    cargo miri nextest run {{args}}

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
    cargo ws publish --publish-as-is --allow-dirty

docsrs *args:
    #!/usr/bin/env -S bash -eux
    source .envrc
    RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc {{args}}

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
        --build-arg RUSTUP_TOOLCHAIN=stable \
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
