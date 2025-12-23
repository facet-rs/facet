//! Packed -ies exception list built from a word list + manual overrides.

use core::cmp::Ordering;

const DATA: &[u8] = include_bytes!("../data/ie_exceptions.bin");
const HEADER_LEN: usize = 16;
const MAGIC: &[u8; 4] = b"IEFC";
const MAX_WORD_LEN: usize = 128;

pub fn contains(word: &str) -> bool {
    let data = DATA;
    if data.len() < HEADER_LEN || &data[..4] != MAGIC {
        return false;
    }

    let count = read_u32(data, 4) as usize;
    let block_size = data[8] as usize;
    let num_blocks = read_u32(data, 12) as usize;
    if block_size == 0 || num_blocks == 0 || count == 0 {
        return false;
    }

    let index_start = HEADER_LEN;
    let index_len = num_blocks.saturating_mul(4);
    let data_start = index_start + index_len;
    if data_start > data.len() {
        return false;
    }

    let target = word.as_bytes();
    let block = match find_block(data, target, index_start, data_start, num_blocks) {
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
    )
}

fn find_block(
    data: &[u8],
    target: &[u8],
    index_start: usize,
    data_start: usize,
    num_blocks: usize,
) -> Option<usize> {
    let mut lo = 0usize;
    let mut hi = num_blocks;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let offset = read_offset(data, index_start, mid)? as usize;
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
) -> bool {
    let offset = match read_offset(data, index_start, block) {
        Some(offset) => offset,
        None => return false,
    };
    let next_offset = if block + 1 < (count + block_size - 1) / block_size {
        read_offset(data, index_start, block + 1)
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

fn read_offset(data: &[u8], index_start: usize, idx: usize) -> Option<u32> {
    let offset = index_start + idx.checked_mul(4)?;
    Some(read_u32(data, offset))
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    if offset + 4 > data.len() {
        return 0;
    }
    let bytes = [
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ];
    u32::from_le_bytes(bytes)
}

fn read_byte(data: &[u8], cursor: &mut usize) -> Option<u8> {
    if *cursor >= data.len() {
        return None;
    }
    let b = data[*cursor];
    *cursor += 1;
    Some(b)
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
