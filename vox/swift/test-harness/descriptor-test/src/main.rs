//! Test harness for MsgDescHot binary layout interop with Swift.
//!
//! Usage:
//!   descriptor-test write  - Write a test descriptor to stdout
//!   descriptor-test read   - Read a descriptor from stdin and validate
//!   descriptor-test round  - Round-trip: write to stdout, read back from stdin

use rapace_core::{FrameFlags, MsgDescHot};
use std::io::{self, Read, Write};

/// Creates a test descriptor with known values for validation.
fn create_test_descriptor() -> MsgDescHot {
    let mut desc = MsgDescHot::new();

    // Identity
    desc.msg_id = 0x123456789ABCDEF0;
    desc.channel_id = 0x12345678;
    desc.method_id = 0xDEADBEEF;

    // Payload location
    desc.payload_slot = 42;
    desc.payload_generation = 7;
    desc.payload_offset = 1024;
    desc.payload_len = 256;

    // Flow control & timing
    desc.flags = FrameFlags::DATA | FrameFlags::EOS;
    desc.credit_grant = 100;
    desc.deadline_ns = 0xFEDCBA9876543210;

    // Inline payload (won't be used since payload_slot != u32::MAX)
    desc.inline_payload = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
        0x0F,
    ];

    desc
}

/// Creates a test descriptor with inline payload.
fn create_inline_test_descriptor() -> MsgDescHot {
    let mut desc = MsgDescHot::new();

    desc.msg_id = 1;
    desc.channel_id = 5;
    desc.method_id = 100;
    desc.payload_slot = u32::MAX; // Inline
    desc.payload_generation = 0;
    desc.payload_offset = 0;
    desc.payload_len = 12; // 12 bytes of inline payload
    desc.flags = FrameFlags::DATA;
    desc.credit_grant = 50;
    desc.deadline_ns = u64::MAX; // No deadline

    // "Hello World!" in bytes
    desc.inline_payload = [
        0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x20, 0x57, 0x6F, 0x72, 0x6C, 0x64, 0x21, 0x00, 0x00, 0x00,
        0x00,
    ];

    desc
}

/// Write descriptor as raw bytes to stdout.
fn write_descriptor(desc: &MsgDescHot) {
    let bytes: &[u8; 64] = unsafe { std::mem::transmute(desc) };
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(bytes).expect("Failed to write to stdout");
    handle.flush().expect("Failed to flush stdout");
}

/// Read descriptor from stdin and return it.
fn read_descriptor() -> MsgDescHot {
    let mut bytes = [0u8; 64];
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    handle
        .read_exact(&mut bytes)
        .expect("Failed to read 64 bytes from stdin");

    unsafe { std::mem::transmute(bytes) }
}

/// Print descriptor fields for debugging.
fn print_descriptor(desc: &MsgDescHot, prefix: &str) {
    eprintln!("{prefix}MsgDescHot {{");
    eprintln!("{prefix}  msg_id: 0x{:016X}", desc.msg_id);
    eprintln!("{prefix}  channel_id: 0x{:08X}", desc.channel_id);
    eprintln!("{prefix}  method_id: 0x{:08X}", desc.method_id);
    eprintln!("{prefix}  payload_slot: {}", desc.payload_slot);
    eprintln!("{prefix}  payload_generation: {}", desc.payload_generation);
    eprintln!("{prefix}  payload_offset: {}", desc.payload_offset);
    eprintln!("{prefix}  payload_len: {}", desc.payload_len);
    eprintln!("{prefix}  flags: {:?} (0x{:08X})", desc.flags, desc.flags.bits());
    eprintln!("{prefix}  credit_grant: {}", desc.credit_grant);
    eprintln!("{prefix}  deadline_ns: 0x{:016X}", desc.deadline_ns);
    eprintln!("{prefix}  inline_payload: {:02X?}", desc.inline_payload);
    eprintln!("{prefix}  is_inline: {}", desc.is_inline());
    eprintln!("{prefix}}}");
}

/// Compare two descriptors and report differences.
fn compare_descriptors(expected: &MsgDescHot, actual: &MsgDescHot) -> bool {
    let mut match_ = true;

    macro_rules! check_field {
        ($field:ident) => {
            if expected.$field != actual.$field {
                eprintln!(
                    "MISMATCH {}: expected {:?}, got {:?}",
                    stringify!($field),
                    expected.$field,
                    actual.$field
                );
                match_ = false;
            }
        };
    }

    check_field!(msg_id);
    check_field!(channel_id);
    check_field!(method_id);
    check_field!(payload_slot);
    check_field!(payload_generation);
    check_field!(payload_offset);
    check_field!(payload_len);
    check_field!(credit_grant);
    check_field!(deadline_ns);

    if expected.flags.bits() != actual.flags.bits() {
        eprintln!(
            "MISMATCH flags: expected 0x{:08X}, got 0x{:08X}",
            expected.flags.bits(),
            actual.flags.bits()
        );
        match_ = false;
    }

    if expected.inline_payload != actual.inline_payload {
        eprintln!(
            "MISMATCH inline_payload: expected {:02X?}, got {:02X?}",
            expected.inline_payload, actual.inline_payload
        );
        match_ = false;
    }

    match_
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    match command {
        "write" => {
            // Write test descriptor to stdout
            let desc = create_test_descriptor();
            eprintln!("Writing test descriptor:");
            print_descriptor(&desc, "  ");
            write_descriptor(&desc);
        }
        "write-inline" => {
            // Write inline test descriptor to stdout
            let desc = create_inline_test_descriptor();
            eprintln!("Writing inline test descriptor:");
            print_descriptor(&desc, "  ");
            write_descriptor(&desc);
        }
        "read" => {
            // Read descriptor from stdin and print it
            let desc = read_descriptor();
            eprintln!("Read descriptor:");
            print_descriptor(&desc, "  ");
        }
        "validate" => {
            // Read descriptor from stdin and validate against expected
            let expected = create_test_descriptor();
            let actual = read_descriptor();

            eprintln!("Expected:");
            print_descriptor(&expected, "  ");
            eprintln!("Actual:");
            print_descriptor(&actual, "  ");

            if compare_descriptors(&expected, &actual) {
                eprintln!("OK: Descriptors match!");
                std::process::exit(0);
            } else {
                eprintln!("FAIL: Descriptors do not match!");
                std::process::exit(1);
            }
        }
        "validate-inline" => {
            // Read descriptor from stdin and validate against expected inline
            let expected = create_inline_test_descriptor();
            let actual = read_descriptor();

            eprintln!("Expected:");
            print_descriptor(&expected, "  ");
            eprintln!("Actual:");
            print_descriptor(&actual, "  ");

            if compare_descriptors(&expected, &actual) {
                eprintln!("OK: Descriptors match!");
                std::process::exit(0);
            } else {
                eprintln!("FAIL: Descriptors do not match!");
                std::process::exit(1);
            }
        }
        "round" => {
            // Round-trip test: Rust -> Swift -> Rust
            // First write our descriptor
            let original = create_test_descriptor();
            eprintln!("Original descriptor:");
            print_descriptor(&original, "  ");
            write_descriptor(&original);

            // Then read back what Swift sends
            let returned = read_descriptor();
            eprintln!("Returned descriptor:");
            print_descriptor(&returned, "  ");

            if compare_descriptors(&original, &returned) {
                eprintln!("OK: Round-trip successful!");
                std::process::exit(0);
            } else {
                eprintln!("FAIL: Round-trip mismatch!");
                std::process::exit(1);
            }
        }
        "hex" => {
            // Write test descriptor as hex to stderr for debugging
            let desc = create_test_descriptor();
            let bytes: &[u8; 64] = unsafe { std::mem::transmute(&desc) };
            eprintln!("Test descriptor as hex:");
            for (i, chunk) in bytes.chunks(16).enumerate() {
                eprint!("  {:02}: ", i * 16);
                for b in chunk {
                    eprint!("{:02X} ", b);
                }
                eprintln!();
            }
        }
        _ => {
            eprintln!("Usage: descriptor-test <command>");
            eprintln!();
            eprintln!("Commands:");
            eprintln!("  write          Write test descriptor to stdout");
            eprintln!("  write-inline   Write inline test descriptor to stdout");
            eprintln!("  read           Read descriptor from stdin and print it");
            eprintln!("  validate       Read from stdin and validate against test descriptor");
            eprintln!("  validate-inline  Read from stdin and validate against inline test");
            eprintln!("  round          Round-trip: write to stdout, read from stdin");
            eprintln!("  hex            Print test descriptor as hex");
            std::process::exit(1);
        }
    }
}
