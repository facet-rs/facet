use std::{
    ops::{Index, IndexMut},
    ptr::NonNull,
};

use facet::{Type, TypeParam};
use facet_core::{Facet, PtrConst, PtrMut, PtrUninit, Shape, ValueVTable};
use facet_reflect::Peek;
use facet_testhelpers::test;

#[derive(Clone, PartialEq, Eq)]
pub struct Mat<T> {
    flat: Vec<T>,
    nrows: usize,
    ncols: usize,
}

impl<T> Index<(usize, usize)> for Mat<T> {
    type Output = T;

    fn index(&self, (row, col): (usize, usize)) -> &Self::Output {
        &self.flat[row + self.nrows * col]
    }
}
impl<T> IndexMut<(usize, usize)> for Mat<T> {
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut Self::Output {
        &mut self.flat[row + self.nrows * col]
    }
}

impl<T> Mat<T> {
    pub fn new(nrows: usize, ncols: usize, value: T) -> Self
    where
        T: Clone,
    {
        Self {
            flat: vec![value; nrows * ncols],
            nrows,
            ncols,
        }
    }

    pub fn nrows(&self) -> usize {
        self.nrows
    }
    pub fn ncols(&self) -> usize {
        self.ncols
    }
    pub fn as_ptr(&self) -> *const T {
        self.flat.as_ptr()
    }
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.flat.as_mut_ptr()
    }
    pub fn row_stride(&self) -> isize {
        size_of::<T>() as isize
    }
    pub fn col_stride(&self) -> isize {
        (self.nrows * size_of::<T>()) as isize
    }
}

unsafe impl<'facet, T: Facet<'facet>> Facet<'facet> for Mat<T> {
    const SHAPE: &'static Shape = &Shape {
        id: Shape::id_of::<Mat<T>>(),
        layout: Shape::layout_of::<Mat<T>>(),
        def: facet_core::Def::NdArray(facet_core::NdArrayDef {
            vtable: &facet_core::NdArrayVTable {
                count: |ptr| {
                    let p = unsafe { ptr.get::<Self>() };
                    p.nrows() * p.ncols()
                },
                n_dim: |_| 2,
                dim: |ptr, i| {
                    let p = unsafe { ptr.get::<Self>() };
                    match i {
                        0 => Some(p.nrows()),
                        1 => Some(p.ncols()),
                        _ => None,
                    }
                },
                get: |ptr, index| {
                    let p = unsafe { ptr.get::<Self>() };
                    let i = index % p.nrows();
                    let index = index / p.nrows();
                    let j = index % p.ncols();
                    let index = index / p.ncols();

                    if index != 0 {
                        return None;
                    }

                    Some(PtrConst::new(NonNull::from(&p[(i, j)])))
                },
                get_mut: Some(|ptr, index| {
                    let p = unsafe { ptr.as_mut::<Self>() };
                    let i = index % p.nrows();
                    let index = index / p.nrows();
                    let j = index % p.ncols();
                    let index = index / p.ncols();

                    if index != 0 {
                        return None;
                    }

                    Some(PtrMut::new(NonNull::from(&mut p[(i, j)])))
                }),
                byte_stride: Some(|ptr, i| {
                    let p = unsafe { ptr.get::<Self>() };
                    match i {
                        0 => Some(p.row_stride()),
                        1 => Some(p.col_stride()),
                        _ => None,
                    }
                }),
                as_ptr: Some(|ptr| {
                    PtrConst::new(unsafe {
                        NonNull::new_unchecked(ptr.get::<Self>().as_ptr() as *mut T)
                    })
                }),
                as_mut_ptr: Some(|ptr| {
                    PtrMut::new(unsafe {
                        NonNull::new_unchecked(ptr.as_mut::<Self>().as_mut_ptr())
                    })
                }),
            },
            t: T::SHAPE,
        }),
        ty: Type::User(facet::UserType::Opaque),
        type_identifier: "Mat",
        type_tag: Some("Mat"),
        type_params: &[TypeParam {
            name: "T",
            shape: T::SHAPE,
        }],
        vtable: ValueVTable::builder(|f, opts| {
            f.write_str("Mat<")?;
            match opts.for_children() {
                Some(opts) => (T::SHAPE.vtable.type_name)(f, opts)?,
                None => f.write_str("…")?,
            }
            f.write_str(">")
        })
        .drop_in_place(Some(|p| unsafe {
            let ptr = p.as_ptr::<Self>() as *mut Self;
            drop(ptr.read());
            PtrUninit::new(NonNull::new_unchecked(ptr))
        }))
        .build(),
        doc: &[],
        attributes: &[],
        inner: None,
        proxy: None,
        variance: facet_core::Variance::Invariant,
    };
}

#[test]
fn ndarray_test() {
    let m = 4;
    let n = 3;
    let mat = Mat::new(m, n, 0.0_f64);
    let peek = Peek::new(&mat);
    let ndarray = peek.into_ndarray().unwrap();

    assert_eq!(ndarray.n_dim(), 2);
    assert_eq!(ndarray.dim(0), Some(m));
    assert_eq!(ndarray.dim(1), Some(n));
    assert_eq!(ndarray.count(), m * n);

    for i in 0..ndarray.count() {
        let item = ndarray.get(i).unwrap();
        let value: &f64 = item.get().unwrap();
        assert_eq!(*value, 0.0);
    }
}
