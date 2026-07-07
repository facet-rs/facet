/// A destination for canonical identity bytes.
pub(crate) trait Sink {
    fn put(&mut self, bytes: &[u8]);
}

impl Sink for blake3::Hasher {
    fn put(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }
}

impl Sink for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

pub(crate) fn write_u8<S: Sink>(out: &mut S, n: u8) {
    out.put(&[n]);
}

pub(crate) fn write_u32<S: Sink>(out: &mut S, n: u32) {
    out.put(&n.to_le_bytes());
}

pub(crate) fn write_u64<S: Sink>(out: &mut S, n: u64) {
    out.put(&n.to_le_bytes());
}

pub(crate) fn write_bool<S: Sink>(out: &mut S, b: bool) {
    write_u8(out, u8::from(b));
}

pub(crate) fn write_str<S: Sink>(out: &mut S, s: &str) {
    write_u32(out, s.len() as u32);
    out.put(s.as_bytes());
}
