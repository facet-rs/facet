#![no_main]

use libfuzzer_sys::fuzz_target;
use rapace::header::MsgHeader;

fuzz_target!(|data: &[u8]| {
    // Should never panic, only return Err
    let _ = MsgHeader::decode_from(data);
});
