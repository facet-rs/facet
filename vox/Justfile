# cf. https://github.com/casey/just

list:
    just --list

spec *args:
    cargo build --package subject-rust
    SUBJECT_CMD="./target/debug/subject-rust" cargo nextest run -p spec-tests {{ args }}
