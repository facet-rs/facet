//! Tracks which marker traits a type implements at runtime.

crate::bitflags! {
    /// Bitflags tracking implementation of Rust's marker/auto traits.
    pub struct MarkerTraits: u8 {
        /// Type implements `Copy`
        const COPY = 0b0000_0001;
        /// Type implements `Send`
        const SEND = 0b0000_0010;
        /// Type implements `Sync`
        const SYNC = 0b0000_0100;
        /// Type implements `Eq` (not just `PartialEq`)
        const EQ = 0b0000_1000;
        /// Type implements `Unpin`
        const UNPIN = 0b0001_0000;
        /// Type implements `UnwindSafe`
        const UNWIND_SAFE = 0b0010_0000;
        /// Type implements `RefUnwindSafe`
        const REF_UNWIND_SAFE = 0b0100_0000;
    }
}
