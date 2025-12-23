//! Packed -ies exception list built from a word list + manual overrides.

use core::cmp::Ordering;

const DATA: &[u8] = include_bytes!("../data/ie_exceptions.bin");
const MAGIC: &[u8; 4] = b"IEF2";
const MAX_WORD_LEN: usize = 128;

pub fn contains(word: &str) -> bool {
    let data = DATA;
    if data.len() < 6 || &data[..4] != MAGIC {
        return false;
    }

    let block_size = data[4] as usize;
    let flags = data[5];
    let mut cursor = 6usize;
    let count = match read_varint_u32(data, &mut cursor) {
        Some(value) => value as usize,
        None => return false,
    };
    let num_blocks = match read_varint_u32(data, &mut cursor) {
        Some(value) => value as usize,
        None => return false,
    };
    if block_size == 0 || num_blocks == 0 || count == 0 {
        return false;
    }

    let index_start = cursor;
    let data_start = match index_end(data, index_start, num_blocks, flags) {
        Some(end) => end,
        None => return false,
    };
    if data_start > data.len() {
        return false;
    }

    let target = word.as_bytes();
    let block = match find_block(data, target, index_start, data_start, flags, num_blocks) {
        Some(block) => block,
        None => return false,
    };

    scan_block(
        data,
        target,
        block,
        count,
        block_size,
        index_start,
        data_start,
        flags,
    )
}

fn find_block(
    data: &[u8],
    target: &[u8],
    index_start: usize,
    data_start: usize,
    flags: u8,
    num_blocks: usize,
) -> Option<usize> {
    let mut lo = 0usize;
    let mut hi = num_blocks;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let offset = read_offset(data, index_start, mid, flags)? as usize;
        let word = read_first_word(data, data_start + offset)?;
        match cmp_bytes(word, target) {
            Ordering::Greater => hi = mid,
            _ => lo = mid + 1,
        }
    }
    if lo == 0 { None } else { Some(lo - 1) }
}

fn scan_block(
    data: &[u8],
    target: &[u8],
    block: usize,
    count: usize,
    block_size: usize,
    index_start: usize,
    data_start: usize,
    flags: u8,
) -> bool {
    let offset = match read_offset(data, index_start, block, flags) {
        Some(offset) => offset,
        None => return false,
    };
    let next_offset = if block + 1 < (count + block_size - 1) / block_size {
        read_offset(data, index_start, block + 1, flags)
    } else {
        Some((data.len() - data_start) as u32)
    };
    let end = match next_offset {
        Some(next) => data_start + next as usize,
        None => return false,
    };
    if data_start + offset as usize > end {
        return false;
    }

    let mut cursor = data_start + offset as usize;
    let mut buf = [0u8; MAX_WORD_LEN];
    let mut len = match read_word_into(data, &mut cursor, &mut buf) {
        Some(len) => len,
        None => return false,
    };

    match cmp_bytes(&buf[..len], target) {
        Ordering::Equal => return true,
        Ordering::Greater => return false,
        Ordering::Less => {}
    }

    let remaining = count.saturating_sub(block * block_size + 1);
    let to_scan = block_size.saturating_sub(1).min(remaining);
    for _ in 0..to_scan {
        let prefix = match read_byte(data, &mut cursor) {
            Some(prefix) => prefix as usize,
            None => return false,
        };
        let suffix_len = match read_byte(data, &mut cursor) {
            Some(suffix_len) => suffix_len as usize,
            None => return false,
        };
        if prefix > len || prefix + suffix_len > buf.len() {
            return false;
        }
        let suffix_end = cursor + suffix_len;
        if suffix_end > end || suffix_end > data.len() {
            return false;
        }
        buf[prefix..prefix + suffix_len].copy_from_slice(&data[cursor..suffix_end]);
        cursor = suffix_end;
        len = prefix + suffix_len;

        match cmp_bytes(&buf[..len], target) {
            Ordering::Equal => return true,
            Ordering::Greater => return false,
            Ordering::Less => {}
        }
    }

    false
}

fn read_first_word(data: &[u8], offset: usize) -> Option<&[u8]> {
    if offset >= data.len() {
        return None;
    }
    let len = data[offset] as usize;
    let start = offset + 1;
    let end = start + len;
    if end > data.len() {
        return None;
    }
    Some(&data[start..end])
}

fn read_word_into(data: &[u8], cursor: &mut usize, buf: &mut [u8]) -> Option<usize> {
    let len = read_byte(data, cursor)? as usize;
    if len > buf.len() {
        return None;
    }
    let end = cursor.checked_add(len)?;
    if end > data.len() {
        return None;
    }
    buf[..len].copy_from_slice(&data[*cursor..end]);
    *cursor = end;
    Some(len)
}

fn read_offset(data: &[u8], index_start: usize, idx: usize, flags: u8) -> Option<u32> {
    if flags & 0x1 == 0x1 {
        let offset = index_start + idx.checked_mul(2)?;
        if offset + 2 > data.len() {
            return None;
        }
        let bytes = [data[offset], data[offset + 1]];
        return Some(u16::from_le_bytes(bytes) as u32);
    }
    let mut cursor = index_start;
    let mut current = 0u32;
    for _ in 0..=idx {
        let delta = read_varint_u32(data, &mut cursor)?;
        current = current.saturating_add(delta);
    }
    Some(current)
}

fn read_varint_u32(data: &[u8], cursor: &mut usize) -> Option<u32> {
    let mut value = 0u32;
    let mut shift = 0u32;
    loop {
        let byte = *data.get(*cursor)?;
        *cursor += 1;
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if shift >= 32 {
            return None;
        }
    }
}

fn read_byte(data: &[u8], cursor: &mut usize) -> Option<u8> {
    if *cursor >= data.len() {
        return None;
    }
    let b = data[*cursor];
    *cursor += 1;
    Some(b)
}

fn index_end(data: &[u8], index_start: usize, num_blocks: usize, flags: u8) -> Option<usize> {
    if flags & 0x1 == 0x1 {
        return index_start.checked_add(num_blocks.saturating_mul(2));
    }
    let mut cursor = index_start;
    for _ in 0..num_blocks {
        read_varint_u32(data, &mut cursor)?;
    }
    Some(cursor)
}

fn cmp_bytes(a: &[u8], b: &[u8]) -> Ordering {
    let len = a.len().min(b.len());
    for i in 0..len {
        match a[i].cmp(&b[i]) {
            Ordering::Equal => {}
            ord => return ord,
        }
    }
    a.len().cmp(&b.len())
}

#[cfg(test)]
mod tests {
    use super::contains;

    #[test]
    fn test_contains() {
        assert!(contains("cookies"));
        assert!(contains("movies"));
        assert!(contains("pies"));
        assert!(contains("ties"));
        assert!(contains("brownies"));
        assert!(contains("rookies"));
        assert!(contains("selfies"));
        assert!(!contains("categories"));
    }
}
