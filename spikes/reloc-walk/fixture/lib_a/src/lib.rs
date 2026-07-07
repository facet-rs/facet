#![allow(clippy::identity_op)]

#[inline(never)]
pub fn hash_stuff(input: u64) -> u64 {
    // HASH_STUFF_BODY_START
    let rotated = input.rotate_left(7);
    rotated ^ 0x9e37_79b9_7f4a_7c15
    // HASH_STUFF_BODY_END
}

#[inline(never)]
pub fn stable_mix(input: u64) -> u64 {
    input.wrapping_mul(0x1000_0000_01b3).rotate_right(11)
}

#[inline(never)]
pub fn hash_pipeline(input: u64) -> u64 {
    hash_stuff(input).wrapping_add(stable_mix(input ^ 0x55aa))
}

pub fn generic_fold<T>(input: T) -> u64
where
    T: Copy + Into<u64>,
{
    // GENERIC_FOLD_BODY_START
    let value = input.into();
    value.wrapping_mul(41).rotate_left(3) ^ 0xfeed_face_cafe_babe
    // GENERIC_FOLD_BODY_END
}
