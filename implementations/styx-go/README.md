# styx-go

Native Go parser for the [Styx configuration language](https://github.com/bearcove/styx).

## Installation

```bash
go get github.com/bearcove/styx/implementations/styx-go
```

## Usage

```go
package main

import (
    "fmt"
    styx "github.com/bearcove/styx/implementations/styx-go"
)

func main() {
    doc, err := styx.Parse(`
name "My App"
version "1.0.0"
server {
    host localhost
    port 8080
}
`)
    if err != nil {
        panic(err)
    }

    for _, entry := range doc.Entries {
        fmt.Printf("Key: %v\n", entry.Key)
    }
}
```

## Development

```bash
# Run tests
go test ./...

# Run linter
go vet ./...

# Run compliance tests
go build ./cmd/styx-compliance
./styx-compliance ../../compliance/corpus | diff -u ../../compliance/golden.sexp -
```

## License

MIT
