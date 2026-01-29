# Just is a task runner, like Make but without the build system / dependency tracking part.
# docs: https://github.com/casey/just
#
# The `-ci` variants are ran in CI, they do command grouping on GitHub Actions, set consistent env vars etc.,
# but they require bash.
#
# The non`-ci` variants can be run locally without having bash installed.

set dotenv-load := true

default: list

list:
    just --list

precommit: gen

gen *args:
    cargo install --git https://github.com/facet-rs/facet-dev
    facet-dev generate -- {{ args }}

prepush:
    cargo install --git https://github.com/facet-rs/facet-dev
    facet-dev prepush

ci: precommit prepush docs msrv miri

nostd:
    rustup target add thumbv8m.main-none-eabihf

    # Run no_std + alloc checks (alloc is required for facet-core)
    cargo check --no-default-features --features alloc -p facet-core --target-dir target/nostd --target thumbv8m.main-none-eabihf
    cargo check --no-default-features --features alloc -p facet --target-dir target/nostd --target thumbv8m.main-none-eabihf
    cargo check --no-default-features --features alloc -p facet-reflect --target-dir target/nostd --target thumbv8m.main-none-eabihf

nostd-ci:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc

    # Set up target directory for no_std + alloc checks (alloc is required)
    export CARGO_TARGET_DIR=target/nostd

    # Run each check in its own group with the full command as the title
    cmd_group "cargo check --no-default-features --features alloc -p facet-core --target thumbv8m.main-none-eabihf"
    cmd_group "cargo check --no-default-features --features alloc -p facet --target thumbv8m.main-none-eabihf"
    cmd_group "cargo check --no-default-features --features alloc -p facet-reflect --target thumbv8m.main-none-eabihf"

clippy-ci:
    cargo clippy --workspace --all-features --all-targets --keep-going -- -D warnings --allow deprecated

clippy-all:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test *args:
    cargo nextest run {{ args }} < /dev/null

test-i686:
    rustup target add i686-unknown-linux-gnu
    cargo nextest run -p facet-value --target i686-unknown-linux-gnu --tests --lib < /dev/null

asan-facet-value:
    #!/usr/bin/env -S bash -euo pipefail
    rustup toolchain install nightly
    cargo +nightly test -Zsanitizer=address -p facet-value --lib --tests -- --test-threads=1

asan-facet-value-ci:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    rustup toolchain install nightly
    cmd_group "cargo +nightly test -Zsanitizer=address -p facet-value --lib --tests -- --test-threads=1"

valgrind *args:
    cargo nextest run --profile valgrind --features jit {{ args }}

fuzz-smoke-value:
    cargo fuzz run fuzz_value -- -runs=1000

fuzz-smoke-inline:
    cargo fuzz run fuzz_inline_string -- -runs=1000

test-ci *args:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;33m🏃 Running all but doc-tests with nextest...\033[0m"
    cmd_group "cargo nextest run --features ci {{ args }} < /dev/null"

    echo -e "\033[1;36m📚 Running documentation tests...\033[0m"
    cmd_group "cargo test --features ci --doc {{ args }}"

doc-tests *args:
    cargo test --doc {{ args }}

doc-tests-ci *args:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;36m📚 Running documentation tests...\033[0m"
    cmd_group "cargo test --doc {{ args }}"

miri *args:
    #!/usr/bin/env -S bash -euo pipefail
    export RUSTUP_TOOLCHAIN=nightly-2026-01-28
    export MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-env-forward=NEXTEST"
    rustup toolchain install "${RUSTUP_TOOLCHAIN}"
    rustup "+${RUSTUP_TOOLCHAIN}" component add miri rust-src
    cargo "+${RUSTUP_TOOLCHAIN}" miri nextest run --target-dir target/miri -p facet-reflect -p facet-core -p facet-value {{ args }}

miri-json *args:
    #!/usr/bin/env -S bash -euo pipefail
    export RUSTUP_TOOLCHAIN=nightly-2026-01-28
    export MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-env-forward=NEXTEST"
    rustup toolchain install "${RUSTUP_TOOLCHAIN}"
    rustup "+${RUSTUP_TOOLCHAIN}" component add miri rust-src
    cargo "+${RUSTUP_TOOLCHAIN}" miri nextest run --target-dir target/miri -p facet-json -E 'not test(/jit/)' {{ args }}

miri-ci *args:
    #!/usr/bin/env -S bash -euxo pipefail
    source .envrc
    echo -e "\033[1;31m🧪 Running tests under Miri with strict provenance...\033[0m"

    export CARGO_TARGET_DIR=target/miri
    export MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-env-forward=NEXTEST"
    cmd_group "cargo miri nextest run -p facet-reflect -p facet-core -p facet-value {{ args }}"

absolve:
    ./facet-dev/absolve.sh

ship:
    #!/usr/bin/env -S bash -euo pipefail
    # Refuse to run if not on main branch or not up to date with origin/main
    branch="$(git rev-parse --abbrev-ref HEAD)"
    if [[ "$branch" != "main" ]]; then
    echo -e "\033[1;31m❌ Refusing to run: not on 'main' branch (current: $branch)\033[0m"
    exit 1
    fi
    git fetch origin main
    local_rev="$(git rev-parse HEAD)"
    remote_rev="$(git rev-parse origin/main)"
    if [[ "$local_rev" != "$remote_rev" ]]; then
    echo -e "\033[1;31m❌ Refusing to run: local main branch is not up to date with origin/main\033[0m"
    echo -e "Local HEAD:  $local_rev"
    echo -e "Origin HEAD: $remote_rev"
    echo -e "Please pull/rebase to update."
    exit 1
    fi
    release-plz update
    git add .
    git commit -m "Upgrades" || true
    git push
    just publish

publish:
    release-plz release --backend github --git-token $(gh auth token)

docsrs *args:
    #!/usr/bin/env -S bash -eux
    source .envrc
    export RUSTDOCFLAGS="--cfg docsrs"
    cargo +nightly doc {{ args }}

msrv:
    # Check default features compile on MSRV
    cargo hack check --rust-version --workspace --locked --ignore-private --keep-going
    # Check all features compile on MSRV
    cargo hack check --rust-version --workspace --locked --ignore-private --keep-going --all-features

msrv-power:
    cargo hack check --feature-powerset --locked --rust-version --ignore-private --workspace --all-targets --keep-going --exclude-no-default-features -

docs:
    cargo doc --workspace --all-features --no-deps --document-private-items --keep-going

lockfile:
    cargo update --workspace --locked

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
        --push \
        --platform linux/amd64 \
        --build-arg BASE_IMAGE=rust:1.91-slim-trixie \
        --build-arg RUSTUP_TOOLCHAIN=1.91 \
        -t "${IMAGE_NAME}:${TAG}-amd64" \
        -t "${IMAGE_NAME}:latest-amd64" \
        -f Dockerfile \
        .

    # Build miri image using nightly Rust
    echo -e "\033[1;36m🔨 Building miri image with nightly Rust...\033[0m"
    docker build \
    --push \
        --platform linux/amd64 \
        --build-arg BASE_IMAGE=rustlang/rust:nightly-slim \
        --build-arg RUSTUP_TOOLCHAIN=nightly \
        --build-arg ADDITIONAL_RUST_COMPONENTS="miri" \
        -t "${IMAGE_NAME}:${TAG}-miri-amd64" \
        -t "${IMAGE_NAME}:latest-miri-amd64" \
        -f Dockerfile \
        .

docker-build-push-linux-arm64:
    #!/usr/bin/env -S bash -eu
    source .envrc
    echo -e "\033[1;34m🐳 Building and pushing Docker images for CI (arm64)...\033[0m"

    # Set variables
    IMAGE_NAME="ghcr.io/facet-rs/facet-ci"
    TAG="$(date +%Y%m%d)-$(git rev-parse --short HEAD)"

    # Build tests image using stable Rust
    echo -e "\033[1;36m🔨 Building tests image with stable Rust (arm64)...\033[0m"
    docker build \
        --push \
        --platform linux/arm64 \
        --build-arg BASE_IMAGE=rust:1.91-slim-trixie \
        --build-arg RUSTUP_TOOLCHAIN=1.91 \
        -t "${IMAGE_NAME}:${TAG}-arm64" \
        -t "${IMAGE_NAME}:latest-arm64" \
        -f Dockerfile \
        .

    # Build miri image using nightly Rust
    echo -e "\033[1;36m🔨 Building miri image with nightly Rust (arm64)...\033[0m"
    docker build \
        --push \
        --platform linux/arm64 \
        --build-arg BASE_IMAGE=rustlang/rust:nightly-slim \
        --build-arg RUSTUP_TOOLCHAIN=nightly \
        --build-arg ADDITIONAL_RUST_COMPONENTS="miri" \
        -t "${IMAGE_NAME}:${TAG}-miri-arm64" \
        -t "${IMAGE_NAME}:latest-miri-arm64" \
        -f Dockerfile \
        .
