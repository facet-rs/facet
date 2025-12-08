#![no_main]

use libfuzzer_sys::fuzz_target;
use rapace::frame::{RawDescriptor, DescriptorLimits};
use rapace::layout::MsgDescHot;

fuzz_target!(|data: &[u8]| {
    if data.len() >= std::mem::size_of::<MsgDescHot>() {
        // Create a descriptor from raw bytes
        let desc: MsgDescHot = unsafe {
            std::ptr::read_unaligned(data.as_ptr() as *const MsgDescHot)
        };

        let raw = RawDescriptor::new(desc);
        let limits = DescriptorLimits::default();

        // Should never panic, only return Err
        let _ = raw.validate_inline_only(&limits);
    }
});
