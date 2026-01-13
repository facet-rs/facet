# cf. https://github.com/casey/just

list:
    just --list

rust *args:
    cargo build --package subject-rust
    SUBJECT_CMD="./target/debug/subject-rust" cargo nextest run -p spec-tests {{ quote(args) }}

ts-typecheck:
    pnpm check

ts-codegen:
    cargo xtask codegen --typescript

ts *args:
    just ts-typecheck
    just ts-codegen
    SUBJECT_CMD="sh typescript/subject/subject-ts.sh" cargo nextest run -p spec-tests {{ quote(args) }}

swift *args:
    swift build -c release --package-path swift/subject
    SUBJECT_CMD="sh swift/subject/subject-swift.sh" cargo nextest run -p spec-tests {{ quote(args) }}

all *args:
    just rust {{ quote(args) }}
    just ts {{ quote(args) }}
    just swift {{ quote(args) }}
