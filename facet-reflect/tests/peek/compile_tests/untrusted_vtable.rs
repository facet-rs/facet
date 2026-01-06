// Soundness test for GitHub issue #1665
//
// Before the fix, PeekListLike::new(), PeekMap::new(), and PeekSet::new() were safe
// but accepted untrusted vtables, allowing UB when those vtables had malicious function pointers.
//
// After the fix, these constructors are unsafe, so this code should fail to compile.
//
// See: https://github.com/facet-rs/facet/issues/1665

use facet::Facet;
use facet_reflect::{ListLikeDef, Peek, PeekListLike};

fn main() -> eyre::Result<()> {
    // Try to create a PeekListLike using the constructor
    // This should fail to compile because PeekListLike::new is unsafe
    let values = vec![1, 2, 3];
    let peek = Peek::new(&values);

    // Extract the ListDef from the shape
    if let facet::Def::List(list_def) = peek.shape().def {
        // This line should fail to compile - new() is unsafe
        let _list_like = PeekListLike::new(peek, ListLikeDef::List(list_def));
    }

    Ok(())
}
