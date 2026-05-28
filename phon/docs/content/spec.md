+++
title = "phon"
description = "Typed binary format and execution engine"
+++

# Base concepts

phon is a binary exchange format, allowing programs written in various languages
to serialize and deserialize values, and supporting schema evolution over time.

Its primary use case and the reasoning for most of its design is RPC (remote
procedure calling), where a request is sent over the wire (TCP, WebSocket, etc.)
to a peer, who sends back a response.

In JSON-RPC, for example, a request for method `ping` would look like this:

```json
{
  "jsonrpc": "2.0",
  "method": "ping",
  "params": { "data": "foobar" },
  "id": 1
}
```

...with a response that looks like this:

```json
{
  "jsonrpc": "2.0",
  "result": { "data": "foobar" },
  "id": 1
}
```

Both the request and the response are self-describing.

If a peer includes an extra field in one of the objects, the other peer is free
to ignore it, because JSON is a self-describing format, just like CBOR, MsgPack,
etc.

The upside to this is observability (you can intercept part of an exchange, and
know what they're talking about), and a certain amount of compatibility that you
get for free — along with better diagnostics, when two things end up being
incompatible. 

On the flip side, self-describing formats waste a lot of bytes, repeating the
same information over and over, when making frequent exchanges between two peers
who know very well which protocol they are speaking. 

Indeed, some folks encode Rust structs, for example, not as JSON objects, but as
JSON arrays:

```json
["2.0", "ping", ["foobar"], 1]
```

```json
["2.0", ["foobar"], 1]
```

This is much less wasteful, but we've lost observability and compatibility. Adding
or removing fields, reordering fields, changing the type of fields, are all
transforms that could lead to silent misinterpretation during decoding.

Some formats attempt to get the best of both worlds by prefixing records with a schema.

```json
{
  "type": "schema",
  "schema": {
    "type": "struct",
    "name": "Request",
    "fields": [
      { "name": "jsonrpc", "type": "string" },
      { "name": "method", "type": "string" },
      {
        "name": "params",
        "type": {
          "type": "struct",
          "name": "PingParams",
          "fields": [
            { "name": "data", "type": "string" }
          ]
        }
      },
      { "name": "id", "type": "u64" }
    ]
  }
}
```

```json
{
  "type": "value",
  "schema": "Request",
  "value": ["2.0", "ping", ["foobar"], 1]
}
```

The schema fully describes what appears on the wire, why, and in which order.

In the context of an RPC system, this lets a peer know ahead of time if the
message that is sent to them is going to be compatible with their conception of
what this message should be.

A system in which a peer has knowledge of its own schema and of the remote
schema enables forwards and backwards compatibility for a large number of schema
mutations without having to explicitly number fields.

It also makes implementing such a scheme challenging in terms of both
correctness and performance. 

First, there is a bootstrapping problem when it comes to schemas. Before we can
use the format, we have to send a schema. The schema itself must be serialized
using some format. It cannot be serialized using the format we're sending the
schema for, because that format is not defined yet. 

This creates the need for two separate formats, or at the very least, two
different modes for a format: a self-describing mode and a compact mode.

Second, there is an important distinction to be made between the representation
of a value of a certain type in memory for a given process in an application
coded with a given programming language, and the representation of that same
value on the wire: sent over TCP to appear, or over WebSocket, or shared between
two processes, over memory mappings. 

Because different languages want to represent types such as structs and classes,
and arrays and vectors, and maps and dictionaries, and sets and tuples, and
different things in memory, the temptation to borrow, as a struct, from the
buffer, for example, is entirely removed. 

Not only must we assume that the wire representation is completely different
from the runtime representation, we must also assume that the remote schema is
different from the local schema. By only implementing and only attempting to
optimize for the worst possible case, we ensure that performance is consistent
throughout, no matter the language pair, and amount of drift between two peers. 

Thirdly, the time and frequency at which schemas are sent matter. 

Sending all schemas ahead of time, upon connection establishment slash
handshake, would result in a huge spike in terms of bandwidth used and latency
at the beginning of a new connection.

Sending schemas, along with every message, would be redundant and largely negate
the benefits of using schemas at all. 

One possible strategy is to send schemas right before any message that would
actually need it. Aiming to send schemas at most once, but tolerating duplicates
in the case of concurrent calls. 

Lastly, one must consider how to recover performance in the face of
non-negotiable data mapping: once again, the wire representation will never
equal the runtime representation. Therefore, there is a deserialization step.

The wire representation is also unpredictable from one peer to the next.
Therefore, deserialization code cannot be compiled and optimized ahead of time.
This is once again non-negotiable and a fundamental consequence of phon's design

A naive implementation would compare the remote schema and the local schema
every time it needs to decode a value. 

A smarter implementation would generate a "decoder program" using the remote
schema, and a local descriptor (containing layout, offset, alignment information
for the runtime representation of a value in a given language in a given
process).

A smarter implementation still would translate that decoder program to machine
code using whatever just-in-time compilation technique feels appropriate.

# Type system

(TODO: describe which types phon can describe. I was thinking of "no IDL" but maybe
the Rust macros can actually.. generate phon IDL from Rust types instead? idk.)

# Schema identity

(TODO: describe how schema hashing works, to identify common schemas,
what goes into a schema identity etc.)

# Self-describing mode

(TODO: specify self-describing mode for all types)

# Compact mode

(TODO: don't forget alignment so we can borrow &[u32] etc. — maybe we should specify
alignment of entire messages? mhh.)
