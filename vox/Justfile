# cf. https://github.com/casey/just

list:
    just --list

rust *args:
    cargo build --package subject-rust
    SUBJECT_CMD="./target/debug/subject-rust" cargo nextest run -p spec-tests {{ quote(args) }}

ts-typecheck:
    npx -y @typescript/native-preview -p typescript/tsconfig.json --noEmit

ts-codegen:
    cargo xtask codegen --typescript

ts *args:
    just ts-typecheck
    just ts-codegen
    SUBJECT_CMD="node --experimental-strip-types typescript/subject/subject.ts" cargo nextest run -p spec-tests {{ quote(args) }}

swift *args:
    swift build --package-path swift/subject
    SUBJECT_CMD="sh swift/subject/subject-swift.sh" cargo nextest run -p spec-tests {{ quote(args) }}

go *args:
    cd go && go build -o subject/subject-go ./subject
    SUBJECT_CMD="./go/subject/subject-go" cargo nextest run -p spec-tests {{ quote(args) }}

java *args:
    sh java/subject/build.sh
    SUBJECT_CMD="sh java/subject/subject-java.sh" cargo nextest run -p spec-tests {{ quote(args) }}

python *args:
    SUBJECT_CMD="python3 python/subject/subject.py" cargo nextest run -p spec-tests {{ quote(args) }}

all *args:
    just rust {{ quote(args) }}
    just ts {{ quote(args) }}
    just swift {{ quote(args) }}
    just go {{ quote(args) }}
    just java {{ quote(args) }}
    just python {{ quote(args) }}
