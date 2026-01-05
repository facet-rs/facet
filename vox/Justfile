# cf. https://github.com/casey/just

list:
    just --list

rust *args:
    cargo build --package subject-rust
    SUBJECT_CMD="./target/debug/subject-rust" cargo nextest run -p spec-tests {{ args }}

ts *args:
    SUBJECT_CMD="node typescript/subject/subject.js" cargo nextest run -p spec-tests {{ args }}

swift *args:
    swift build --package-path swift/subject
    SUBJECT_CMD="sh swift/subject/subject-swift.sh" cargo nextest run -p spec-tests {{ args }}

go *args:
    cd go && go build -o subject/subject-go ./subject
    SUBJECT_CMD="./go/subject/subject-go" cargo nextest run -p spec-tests {{ args }}

java *args:
    sh java/subject/build.sh
    SUBJECT_CMD="sh java/subject/subject-java.sh" cargo nextest run -p spec-tests {{ args }}

python *args:
    SUBJECT_CMD="python3 python/subject/subject.py" cargo nextest run -p spec-tests {{ args }}

all *args:
    just rust {{ args }}
    just ts {{ args }}
    just swift {{ args }}
    just go {{ args }}
    just java {{ args }}
    just python {{ args }}
