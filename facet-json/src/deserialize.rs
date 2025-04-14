use facet_core::Facet;
use facet_reflect::Wip;

/// A JSON parse error, with context. Never would've guessed huh.
#[derive(Debug)]
pub struct JsonParseErrorWithContext<'input> {
    input: &'input [u8],
}

impl core::fmt::Display for JsonParseErrorWithContext<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "JSON parse error: {}",
            core::str::from_utf8(self.input).unwrap_or("invalid UTF-8")
        )
    }
}

/// Deserializes a JSON string into a value of type `T` that implements `Facet`.
///
/// This function takes a JSON string representation and converts it into a Rust
/// value of the specified type `T`. The type must implement the `Facet` trait
/// to provide the necessary type information for deserialization.
pub fn from_str<T: Facet>(json: &str) -> Result<T, JsonParseErrorWithContext<'_>> {
    from_slice(json.as_bytes())
}

/// Deserialize JSON from a slice
pub fn from_slice<T: Facet>(json: &[u8]) -> Result<T, JsonParseErrorWithContext<'_>> {
    let wip = Wip::alloc::<T>();
    let wip = from_slice_wip(wip, json)?;
    let heap_value = wip.build().unwrap();
    Ok(heap_value.materialize::<T>().unwrap())
}

/// Deserialize a JSON string into a Wip object.
pub fn from_slice_wip<'input, 'a>(
    _wip: Wip<'a>,
    _input: &'input [u8],
) -> Result<Wip<'a>, JsonParseErrorWithContext<'input>> {
    todo!()

    // loop {
    //     let frame = wip.frames_count();
    // }
}
