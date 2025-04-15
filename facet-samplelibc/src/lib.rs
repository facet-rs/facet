#![allow(clippy::disallowed_names)]

use facet::{Facet, PtrMut, Shape};

unsafe extern "C" {
    pub unsafe fn get_library_message() -> *const std::ffi::c_char;
    pub unsafe fn get_foo() -> *mut Foo;
}

pub fn get_foo_and_shape() -> (PtrMut<'static>, &'static Shape) {
    (unsafe { PtrMut::new(get_foo()) }, Foo::SHAPE)
}

pub fn print_global_foo() {
    let foo = unsafe { get_foo() };
    let bar = unsafe { &(*foo).bar };
    let x = unsafe { (*foo).x };
    let y = unsafe { (*foo).y };

    println!("Foo: x={}, bar.a={}, bar.b={}, y={}", x, bar.a, bar.b, y);
}

#[derive(Facet, Debug)]
#[repr(C)]
pub struct Foo {
    pub x: i64,
    pub bar: Bar,
    pub y: i64,
}

#[derive(Facet, Debug)]
#[repr(C)]
pub struct Bar {
    pub a: i32,
    pub b: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message() {
        if !cfg!(miri) {
            let msg = unsafe { get_library_message() };
            let c_str = unsafe { std::ffi::CStr::from_ptr(msg) };
            let rust_str = c_str.to_string_lossy();
            println!("{}", rust_str);

            assert_eq!(rust_str, "IAMA C lib AMA")
        }
    }

    #[test]
    fn foo() {
        if !cfg!(miri) {
            print_global_foo();
        }
    }
}
