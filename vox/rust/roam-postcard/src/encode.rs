pub fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

pub fn write_varint_signed(out: &mut Vec<u8>, value: i64) {
    let zigzag = ((value << 1) ^ (value >> 63)) as u64;
    write_varint(out, zigzag);
}

pub fn write_varint_u128(out: &mut Vec<u8>, mut value: u128) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

pub fn write_varint_signed_i128(out: &mut Vec<u8>, value: i128) {
    let zigzag = ((value << 1) ^ (value >> 127)) as u128;
    write_varint_u128(out, zigzag);
}
