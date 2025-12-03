#![no_main]

use arbitrary::Arbitrary;
use facet_value::{format::format_value, VString, Value};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug, Clone)]
enum MutOp {
    Append(u8),
    Truncate(u8),
    Clear,
    CloneCheck,
    FormatRoundTrip,
}

fuzz_target!(|ops: Vec<MutOp>| {
    let mut value = Value::from("");
    for op in ops.into_iter().take(64) {
        match op {
            MutOp::Append(byte) => {
                mutate_string(&mut value, |s| {
                    let ch = (b'a' + (byte % 26)) as char;
                    s.push(ch);
                });
            }
            MutOp::Truncate(len) => {
                mutate_string(&mut value, |s| {
                    if !s.is_empty() {
                        let target = (len as usize) % (s.len() + 1);
                        s.truncate(target);
                    }
                });
            }
            MutOp::Clear => {
                mutate_string(&mut value, |s| s.clear());
            }
            MutOp::CloneCheck => {
                let cloned = value.clone();
                assert_eq!(cloned, value);
                assert_eq!(
                    cloned.is_inline_string(),
                    value.is_inline_string(),
                    "clone should preserve inline flag"
                );
            }
            MutOp::FormatRoundTrip => {
                let formatted = format_value(&value);
                let expected = value.as_string().unwrap().as_str().to_owned();
                assert!(
                    formatted.contains(&expected),
                    "formatted value should contain payload"
                );
            }
        }

        assert_string_layout(&value);
    }
});

fn mutate_string(value: &mut Value, f: impl FnOnce(&mut String)) {
    let slot = value.as_string_mut().expect("value should remain a string");
    let mut owned = slot.to_string();
    f(&mut owned);
    *slot = VString::new(&owned);
}

fn assert_string_layout(value: &Value) {
    let string = value.as_string().expect("value remains a string");
    let should_inline = string.len() <= VString::INLINE_LEN_MAX;
    assert_eq!(
        should_inline,
        value.is_inline_string(),
        "string length {} should align with inline status",
        string.len()
    );
}
