# Just is a task runner, like Make but without the build system / dependency tracking part.
# docs: https://github.com/casey/just
#
# The `-ci` variants are ran in CI, they do command grouping on GitHub Actions, set consistent env vars etc.,
# but they require bash.
#
# The non`-ci` variants can be run locally without having bash installed.

set dotenv-load

default: precommit prepush

precommit: gen

gen *args:
    cargo run -p facet-dev generate -- {{args}}

prepush:
    cargo run -p facet-dev prepush

ci: precommit prepush docs msrv miri

nostd:
    rustup target add thumbv8m.main-none-eabihf

    # Run alloc but no-std checks with specified target directory
    cargo check --no-default-features -p facet-core --target-dir target/nostd --target thumbv8m.main-none-eabihf
    cargo check --no-default-features -p facet --target-dir target/nostd --target thumbv8m.main-none-eabihf
    cargo check --no-default-features -p facet-reflect --target-dir target/nostd --target thumbv8m.main-none-eabihf

    # Run alloc but no-std checks with specified target directory
    cargo check --no-default-features --features alloc -p facet-core --target-dir target/nostd-w-alloc --target thumbv8m.main-none-eabihf
    cargo check --no-default-features --features alloc -p facet --target-dir target/nostd-w-alloc --target thumbv8m.main-none-eabihf
    cargo check --no-default-features --features alloc -p facet-reflect --target-dir target/nostd-w-alloc --target thumbv8m.main-none-eabihf
    cargo check --no-default-features --features alloc -p facet-json --target-dir target/nostd-w-alloc --target thumbv8m.main-none-eabihf

nostd-ci:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc

    # Set up target directory for no-std checks
    export CARGO_TARGET_DIR=target/nostd

    # Run each check in its own group with the full command as the title
    cmd_group "cargo check --no-default-features -p facet-core --target thumbv8m.main-none-eabihf"
    cmd_group "cargo check --no-default-features -p facet --target thumbv8m.main-none-eabihf"
    cmd_group "cargo check --no-default-features -p facet-reflect --target thumbv8m.main-none-eabihf"

    # Set up target directory for alloc but no-std checks
    export CARGO_TARGET_DIR=target/nostd-w-alloc

    # Run each check in its own group with the full command as the title
    cmd_group "cargo check --no-default-features --features alloc -p facet-core --target thumbv8m.main-none-eabihf"
    cmd_group "cargo check --no-default-features --features alloc -p facet --target thumbv8m.main-none-eabihf"
    cmd_group "cargo check --no-default-features --features alloc -p facet-reflect --target thumbv8m.main-none-eabihf"

clippy-all:
    cargo clippy --workspace --all-targets --all-features --target-dir target/clippy-all-features -- -D warnings

clippy:
    cargo clippy --workspace --all-targets --target-dir target/clippy -- -D warnings

test *args:
    cargo nextest run {{args}} < /dev/null

test-ci *args:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;33m🏃 Running all but doc-tests with nextest...\033[0m"
    cmd_group "cargo nextest run {{args}} < /dev/null"

    echo -e "\033[1;36m📚 Running documentation tests...\033[0m"
    cmd_group "cargo test --features slow-tests --doc {{args}}"

doc-tests *args:
    cargo test --doc {{args}}

doc-tests-ci *args:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    echo -e "\033[1;36m📚 Running documentation tests...\033[0m"
    cmd_group "cargo test --doc {{args}}"

code-quality-ci:
    #!/usr/bin/env -S bash -euo pipefail
    source .envrc
    cmd_group "just gen --check"
    cmd_group "cargo fmt --check --all"
    cmd_group "just absolve"

miri *args:
    export RUSTUP_TOOLCHAIN=nightly-2025-05-11
    rustup toolchain install nightly-2025-05-11
    rustup +nightly-2025-05-11 component add miri rust-src
    cargo +nightly-2025-05-11 miri nextest run --target-dir target/miri -p facet-reflect -p facet-core -p facet-json {{args}}

miri-ci *args:
    #!/usr/bin/env -S bash -euxo pipefail
    source .envrc
    echo -e "\033[1;31m🧪 Running tests under Miri...\033[0m"

    export CARGO_TARGET_DIR=target/miri
    cmd_group "cargo miri nextest run {{args}}"

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
    release-plz release --backend github --git-token $PUBLISH_GITHUB_TOKEN

docsrs *args:
    #!/usr/bin/env -S bash -eux
    source .envrc
    export RUSTDOCFLAGS="--cfg docsrs"
    cargo +nightly doc {{args}}

msrv:
    cargo hack check --each-feature --locked --rust-version --ignore-private --workspace --keep-going --exclude-no-default-features --target-dir target/msrv

msrv-power:
    cargo hack check --feature-powerset --locked --rust-version --ignore-private --workspace --all-targets --keep-going --exclude-no-default-features --target-dir target/msrv

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
        --push \
        --build-arg BASE_IMAGE=rust:1.87-slim-bookworm \
        --build-arg RUSTUP_TOOLCHAIN=1.87 \
        -t "${IMAGE_NAME}:${TAG}" \
        -t "${IMAGE_NAME}:latest" \
        -f Dockerfile \
        .

    # Build miri image using nightly Rust
    echo -e "\033[1;36m🔨 Building miri image with nightly Rust...\033[0m"
    docker build \
        --push \
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
        --push \
        --platform linux/amd64 \
        --build-arg BASE_IMAGE=rust:1.87-slim-bookworm \
        --build-arg RUSTUP_TOOLCHAIN=1.87 \
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

    # # Push all tags
    # echo -e "\033[1;35m🚀 Pushing all image tags...\033[0m"
    # docker push "${IMAGE_NAME}:${TAG}-amd64"
    # docker push "${IMAGE_NAME}:latest-amd64"
    # docker push "${IMAGE_NAME}:${TAG}-miri-amd64"
    # docker push "${IMAGE_NAME}:latest-miri-amd64"
