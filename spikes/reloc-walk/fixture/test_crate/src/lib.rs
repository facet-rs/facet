pub fn local_double(input: u64) -> u64 {
    input.wrapping_mul(2)
}

pub fn local_word_score(input: &str) -> usize {
    input.bytes().map(usize::from).sum::<usize>() ^ input.len()
}

pub fn local_table_pick(index: usize) -> u64 {
    const TABLE: [u64; 4] = [3, 5, 8, 13];
    TABLE[index % TABLE.len()]
}
