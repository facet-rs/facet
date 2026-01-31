use facet::Facet;
use facet_reflect2::{Op, Partial};

// =============================================================================
// Basic array tests
// =============================================================================

#[test]
fn build_array_of_scalars() {
    let mut partial = Partial::alloc::<[u32; 3]>().unwrap();

    let mut a = 1u32;
    let mut b = 2u32;
    let mut c = 3u32;

    partial
        .apply(&[
            Op::set().at(0).imm(&mut a),
            Op::set().at(1).imm(&mut b),
            Op::set().at(2).imm(&mut c),
        ])
        .unwrap();

    let result: [u32; 3] = partial.build().unwrap();
    assert_eq!(result, [1, 2, 3]);
}

#[test]
fn build_array_with_default() {
    let mut partial = Partial::alloc::<[u32; 3]>().unwrap();

    partial
        .apply(&[
            Op::set().at(0).default(),
            Op::set().at(1).default(),
            Op::set().at(2).default(),
        ])
        .unwrap();

    let result: [u32; 3] = partial.build().unwrap();
    assert_eq!(result, [0, 0, 0]);
}

#[test]
fn build_array_of_strings() {
    let mut partial = Partial::alloc::<[String; 2]>().unwrap();

    let mut a = String::from("hello");
    let mut b = String::from("world");

    partial
        .apply(&[Op::set().at(0).imm(&mut a), Op::set().at(1).imm(&mut b)])
        .unwrap();

    // After apply(), the strings have been moved - must forget to avoid double-free
    std::mem::forget(a);
    std::mem::forget(b);

    let result: [String; 2] = partial.build().unwrap();
    assert_eq!(result, [String::from("hello"), String::from("world")]);
}

// =============================================================================
// Array of structs
// =============================================================================

#[derive(Debug, PartialEq, Facet, Clone)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn build_array_of_structs_with_imm() {
    let mut partial = Partial::alloc::<[Point; 2]>().unwrap();

    let mut p1 = Point { x: 1, y: 2 };
    let mut p2 = Point { x: 3, y: 4 };

    partial
        .apply(&[Op::set().at(0).imm(&mut p1), Op::set().at(1).imm(&mut p2)])
        .unwrap();

    let result: [Point; 2] = partial.build().unwrap();
    assert_eq!(result, [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]);
}

#[test]
fn build_array_of_structs_with_build() {
    let mut partial = Partial::alloc::<[Point; 2]>().unwrap();

    let mut x1 = 10i32;
    let mut y1 = 20i32;
    let mut x2 = 30i32;
    let mut y2 = 40i32;

    partial
        .apply(&[
            Op::set().at(0).build(), // enter Point[0]
            Op::set().at(0).imm(&mut x1),
            Op::set().at(1).imm(&mut y1),
            Op::end(),
            Op::set().at(1).build(), // enter Point[1]
            Op::set().at(0).imm(&mut x2),
            Op::set().at(1).imm(&mut y2),
            Op::end(),
        ])
        .unwrap();

    let result: [Point; 2] = partial.build().unwrap();
    assert_eq!(result, [Point { x: 10, y: 20 }, Point { x: 30, y: 40 }]);
}

// =============================================================================
// Array as struct field
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct Point3D {
    coords: [f32; 3],
}

#[test]
fn build_struct_with_array_field() {
    let mut partial = Partial::alloc::<Point3D>().unwrap();

    let mut x = 1.0f32;
    let mut y = 2.0f32;
    let mut z = 3.0f32;

    partial
        .apply(&[
            Op::set().at(0).build(), // enter coords array
            Op::set().at(0).imm(&mut x),
            Op::set().at(1).imm(&mut y),
            Op::set().at(2).imm(&mut z),
            Op::end(),
        ])
        .unwrap();

    let result: Point3D = partial.build().unwrap();
    assert_eq!(
        result,
        Point3D {
            coords: [1.0, 2.0, 3.0]
        }
    );
}

#[derive(Debug, PartialEq, Facet)]
struct Triangle {
    vertices: [Point; 3],
}

#[test]
fn build_struct_with_array_of_structs_field() {
    let mut partial = Partial::alloc::<Triangle>().unwrap();

    let mut v0 = Point { x: 0, y: 0 };
    let mut v1 = Point { x: 10, y: 0 };
    let mut v2 = Point { x: 5, y: 10 };

    partial
        .apply(&[
            Op::set().at(0).build(), // enter vertices array
            Op::set().at(0).imm(&mut v0),
            Op::set().at(1).imm(&mut v1),
            Op::set().at(2).imm(&mut v2),
            Op::end(),
        ])
        .unwrap();

    let result: Triangle = partial.build().unwrap();
    assert_eq!(
        result,
        Triangle {
            vertices: [
                Point { x: 0, y: 0 },
                Point { x: 10, y: 0 },
                Point { x: 5, y: 10 }
            ]
        }
    );
}

#[test]
fn build_struct_with_array_of_structs_field_via_build() {
    let mut partial = Partial::alloc::<Triangle>().unwrap();

    let mut x0 = 0i32;
    let mut y0 = 0i32;
    let mut x1 = 10i32;
    let mut y1 = 0i32;
    let mut x2 = 5i32;
    let mut y2 = 10i32;

    partial
        .apply(&[
            Op::set().at(0).build(), // enter vertices array
            // Build vertex 0
            Op::set().at(0).build(),
            Op::set().at(0).imm(&mut x0),
            Op::set().at(1).imm(&mut y0),
            Op::end(),
            // Build vertex 1
            Op::set().at(1).build(),
            Op::set().at(0).imm(&mut x1),
            Op::set().at(1).imm(&mut y1),
            Op::end(),
            // Build vertex 2
            Op::set().at(2).build(),
            Op::set().at(0).imm(&mut x2),
            Op::set().at(1).imm(&mut y2),
            Op::end(),
            Op::end(), // end array
        ])
        .unwrap();

    let result: Triangle = partial.build().unwrap();
    assert_eq!(
        result,
        Triangle {
            vertices: [
                Point { x: 0, y: 0 },
                Point { x: 10, y: 0 },
                Point { x: 5, y: 10 }
            ]
        }
    );
}

// =============================================================================
// Zero-length array
// =============================================================================

#[test]
fn build_empty_array() {
    let partial = Partial::alloc::<[u32; 0]>().unwrap();

    // No operations needed for empty array
    let result: [u32; 0] = partial.build().unwrap();
    assert_eq!(result, []);
}

// =============================================================================
// Zero-sized type (ZST) arrays
// =============================================================================

#[derive(Debug, PartialEq, Facet, Clone, Copy, Default)]
struct Zst;

#[test]
fn build_array_of_zst() {
    let mut partial = Partial::alloc::<[Zst; 3]>().unwrap();

    partial
        .apply(&[
            Op::set().at(0).default(),
            Op::set().at(1).default(),
            Op::set().at(2).default(),
        ])
        .unwrap();

    let result: [Zst; 3] = partial.build().unwrap();
    assert_eq!(result, [Zst, Zst, Zst]);
}

#[test]
fn build_array_of_zst_with_imm() {
    let mut partial = Partial::alloc::<[Zst; 2]>().unwrap();

    let mut a = Zst;
    let mut b = Zst;

    partial
        .apply(&[Op::set().at(0).imm(&mut a), Op::set().at(1).imm(&mut b)])
        .unwrap();

    let result: [Zst; 2] = partial.build().unwrap();
    assert_eq!(result, [Zst, Zst]);
}

// =============================================================================
// Single-element array
// =============================================================================

#[test]
fn build_single_element_array() {
    let mut partial = Partial::alloc::<[String; 1]>().unwrap();

    let mut s = String::from("only one");

    partial.apply(&[Op::set().at(0).imm(&mut s)]).unwrap();

    // After apply(), the string has been moved - must forget to avoid double-free
    std::mem::forget(s);

    let result: [String; 1] = partial.build().unwrap();
    assert_eq!(result, [String::from("only one")]);
}

// =============================================================================
// Nested arrays
// =============================================================================

#[test]
fn build_2d_array() {
    let mut partial = Partial::alloc::<[[u32; 2]; 2]>().unwrap();

    let mut row0 = [1u32, 2u32];
    let mut row1 = [3u32, 4u32];

    partial
        .apply(&[
            Op::set().at(0).imm(&mut row0),
            Op::set().at(1).imm(&mut row1),
        ])
        .unwrap();

    let result: [[u32; 2]; 2] = partial.build().unwrap();
    assert_eq!(result, [[1, 2], [3, 4]]);
}

#[test]
fn build_2d_array_element_by_element() {
    let mut partial = Partial::alloc::<[[u32; 2]; 2]>().unwrap();

    let mut a = 1u32;
    let mut b = 2u32;
    let mut c = 3u32;
    let mut d = 4u32;

    partial
        .apply(&[
            Op::set().at(0).build(), // enter row 0
            Op::set().at(0).imm(&mut a),
            Op::set().at(1).imm(&mut b),
            Op::end(),
            Op::set().at(1).build(), // enter row 1
            Op::set().at(0).imm(&mut c),
            Op::set().at(1).imm(&mut d),
            Op::end(),
        ])
        .unwrap();

    let result: [[u32; 2]; 2] = partial.build().unwrap();
    assert_eq!(result, [[1, 2], [3, 4]]);
}
