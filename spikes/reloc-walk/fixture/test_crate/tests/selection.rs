use test_crate::{local_double, local_table_pick, local_word_score};

#[test]
fn local_arithmetic_is_isolated() {
    assert_eq!(local_double(21), 42);
}

#[test]
fn local_string_is_isolated() {
    assert_eq!(local_word_score("facet"), 518);
}

#[test]
fn local_table_is_isolated() {
    assert_eq!(local_table_pick(5), 5);
}

#[test]
fn hash_direct_uses_lib_a() {
    assert_eq!(lib_a::hash_stuff(7), 0x9e37_79b9_7f4a_7c15 ^ 896);
}

#[test]
fn hash_pipeline_uses_lib_a() {
    let expected = lib_a::hash_stuff(9).wrapping_add(lib_a::stable_mix(9 ^ 0x55aa));
    assert_eq!(lib_a::hash_pipeline(9), expected);
}

#[test]
fn generic_instantiation_uses_lib_a() {
    assert_eq!(
        lib_a::generic_fold(11_u64),
        11_u64.wrapping_mul(41).rotate_left(3) ^ 0xfeed_face_cafe_babe
    );
}
