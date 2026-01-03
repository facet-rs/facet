//! Soundness test for GitHub issue #1573
//!
//! Before the fix, Attr could store non-Sync data like Rc<T>, but Attr itself
//! was Sync, allowing data races when accessed from multiple threads.
//!
//! After the fix, Attr::new requires T: Sync, so this code should fail to compile.

use std::rc::Rc;

use facet::Attr;

fn main() {
    // Rc is not Sync, so this should fail to compile
    let rc: &'static Rc<i32> = Box::leak(Box::new(Rc::new(0)));
    let _attr = Attr::new(None, "test", rc);
}
