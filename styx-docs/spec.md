# styx

STYX is a document language designed to replaced YAML, TOML, JSON, etc. for documents authored
by humans.

## Value types

STYX values are one of:

  * Scalar
  * Object
  * Sequence
  
## Scalars
  
## Objects

There are several object forms in STYX.

### Block objects

> r[object.block.delimiters]
> Block objects MUST start with `{` and end with `}`:
> 
> ```styx
> {
>   key value
>   key value
> }
> ```

> r[object.block.separators]
> In block objects, keys and values MUST separated by spaces, and key-value pairs MUST be separated by newlines (`\n`) or commas (`,`):
> 
> ```styx
> // this is fine, too!
> {
>   key value, key value
> }
> ```
> 
> ```styx
> { key value, key value } // and so is this
> ```

> r[object.block.separators.trailing]
>
> Trailing commas in a block object MUST be treated as a syntax error:
>
> ```styx
> {
>   key value, // <- ERROR: expected another key
> }
> ```

> r[object.key.bare]
> Bare keys MUST only contain /[A-Za-z0-9-_]/.
>
> Any key that contains a space, a unicode character, etc., must be double-quoted.
>
