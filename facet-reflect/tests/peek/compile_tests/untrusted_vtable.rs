// Soundness test for GitHub issues #1665, #1684, #1685
//
// Before the fix, PeekListLike::new(), PeekMap::new(), PeekSet::new(), PeekList::new(),
// and PeekNdArray::new() were safe but accepted untrusted vtables, allowing UB when
// those vtables had malicious function pointers.
//
// After the fix, these constructors are unsafe, so this code should fail to compile.
//
// See: https://github.com/facet-rs/facet/issues/1665
// See: https://github.com/facet-rs/facet/issues/1684
// See: https://github.com/facet-rs/facet/issues/1685

use facet::Facet;
use facet_reflect::{ListLikeDef, Peek, PeekList, PeekListLike, PeekNdArray};

fn main() -> eyre::Result<()> {
    let values = vec![1, 2, 3];
    let peek = Peek::new(&values);

    // Extract the ListDef from the shape
    if let facet::Def::List(list_def) = peek.shape().def {
        // These lines should fail to compile - new() is unsafe
        let _list_like = PeekListLike::new(peek, ListLikeDef::List(list_def));
        let _list = PeekList::new(peek, list_def);
    }

    // Test PeekNdArray as well (issue #1685)
    if let facet::Def::NdArray(ndarray_def) = peek.shape().def {
        // This line should fail to compile - new() is unsafe
        let _ndarray = PeekNdArray::new(peek, ndarray_def);
    }

    Ok(())
}
