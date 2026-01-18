+++
title = "Go"
weight = 3
slug = "go"
insert_anchor_links = "heading"
+++

Native Go implementation.

## Installation

```bash
go get github.com/bearcove/styx/implementations/styx-go
```

## Usage

```go
import styx "github.com/bearcove/styx/implementations/styx-go"

doc, err := styx.Parse(`name "Alice"
age 30`)
```

## Requirements

Go 1.22+

## Source

[implementations/styx-go](https://github.com/bearcove/styx/tree/main/implementations/styx-go)
