# cf. https://github.com/casey/just

list:
    just --list

rust *args:
    cargo build --package subject-rust
    SUBJECT_CMD="./target/debug/subject-rust" cargo nextest run -p spec-tests {{ args }}

ts *args:
    SUBJECT_CMD="node typescript/subject/subject.js" cargo nextest run -p spec-tests {{ args }}

swift *args:
    SUBJECT_CMD="sh swift/subject/subject-swift.sh" cargo nextest run -p spec-tests {{ args }}

all *args:
    just rust {{ args }}
    just ts {{ args }}
    just swift {{ args }}
