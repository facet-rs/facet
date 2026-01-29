//! Soundness test for GitHub issue #1573
//!
//! Before the fix, Attr could store non-Sync data like Rc<T>, but Attr itself
//! was Sync, allowing data races when accessed from multiple threads.
//!
//! After the fix, Attr::new requires T: Sync, so this code should fail to compile.

use std::rc::Rc;
use std::sync::LazyLock;

use facet::Attr;

static RC: LazyLock<Rc<i32>> = LazyLock::new(|| Rc::new(0));

fn main() {
    // Rc is not Sync, so this should fail to compile
    let _attr = Attr::new(None, "test", &*RC);
}
