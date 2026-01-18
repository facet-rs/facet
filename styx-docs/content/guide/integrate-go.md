+++
title = "Integrate with Go"
weight = 5
slug = "integrate-go"
insert_anchor_links = "heading"
+++

Add Styx to your Go application. The Go implementation is a native parser with no dependencies.

## Requirements

Go 1.21 or later.

## Installation

```bash
go get github.com/bearcove/styx/implementations/styx-go
```

Or add to your `go.mod`:

```
require github.com/bearcove/styx/implementations/styx-go v0.1.0
```

## Basic usage

```go
package main

import (
    "fmt"
    "log"

    styx "github.com/bearcove/styx/implementations/styx-go"
)

func main() {
    input := `
        host localhost
        port 8080
        debug true
    `

    doc, err := styx.Parse(input)
    if err != nil {
        log.Fatal(err)
    }

    for _, entry := range doc.Entries {
        key := entry.Key.Scalar.Text
        value := entry.Value.Scalar.Text
        fmt.Printf("%s = %s\n", key, value)
    }
}
```

## Working with the AST

The parser returns a typed AST:

```go
package main

import (
    "fmt"
    "log"

    styx "github.com/bearcove/styx/implementations/styx-go"
)

func main() {
    input := `
        name "Alice"
        tags (admin user)
        settings {
            theme dark
        }
    `

    doc, err := styx.Parse(input)
    if err != nil {
        log.Fatal(err)
    }

    for _, entry := range doc.Entries {
        key := entry.Key.Scalar.Text
        value := entry.Value

        switch value.PayloadKind {
        case styx.PayloadScalar:
            fmt.Printf("%s = %s\n", key, value.Scalar.Text)
        case styx.PayloadSequence:
            var items []string
            for _, item := range value.Sequence.Items {
                items = append(items, item.Scalar.Text)
            }
            fmt.Printf("%s = %v\n", key, items)
        case styx.PayloadObject:
            fmt.Printf("%s = <object with %d entries>\n", key, len(value.Object.Entries))
        case styx.PayloadNone:
            fmt.Printf("%s = <unit>\n", key)
        }
    }
}
```

## Error handling

Parse errors include source location information:

```go
doc, err := styx.Parse(`key "unterminated string`)
if err != nil {
    if parseErr, ok := err.(*styx.ParseError); ok {
        fmt.Printf("Error: %s\n", parseErr.Message)
        fmt.Printf("Location: bytes %d-%d\n", parseErr.Span.Start, parseErr.Span.End)
    }
}
```

## Source

[implementations/styx-go](https://github.com/bearcove/styx/tree/main/implementations/styx-go)
