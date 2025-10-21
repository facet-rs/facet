use std::{
    ops::{Index, IndexMut},
    ptr::NonNull,
};

use facet::{Type, TypeParam};
use facet_core::{Facet, MarkerTraits, PtrConst, PtrMut, PtrUninit, Shape, ValueVTable};
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
    const SHAPE: &'static Shape = &Shape::builder_for_sized::<Mat<T>>()
        .def(facet_core::Def::NdArray(facet_core::NdArrayDef {
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
        }))
        .ty(Type::User(facet::UserType::Opaque))
        .type_identifier("Mat")
        .type_tag("Mat")
        .type_params(&[TypeParam {
            name: "T",
            shape: T::SHAPE,
        }])
        .vtable(ValueVTable {
            type_name: |f, opts| {
                f.write_str("Mat<")?;
                match opts.for_children() {
                    Some(opts) => (T::SHAPE.vtable.type_name)(f, opts)?,
                    None => f.write_str("…")?,
                }
                f.write_str(">")
            },
            marker_traits: T::SHAPE.vtable.marker_traits.difference(MarkerTraits::COPY),
            drop_in_place: {
                Some(|p| unsafe {
                    let ptr = p.as_ptr::<Self>() as *mut Self;
                    drop(ptr.read());
                    PtrUninit::new(NonNull::new_unchecked(ptr))
                })
            },
            invariants: None,
            display: None,
            debug: None,
            default_in_place: None,
            clone_into: None,
            partial_eq: None,
            partial_ord: None,
            ord: None,
            hash: None,
            parse: None,
            try_from: None,
            try_into_inner: None,
            try_borrow_inner: None,
        })
        .build();
}

#[test]
fn ndarray_test() {
    let m = 4;
    let n = 5;
    let mut mat = Mat::new(m, n, 0.0);
    for i in 0..m {
        for j in 0..n {
            mat[(i, j)] = (i + 2 * j) as f64;
        }
    }

    let ptr = mat.as_ptr();
    let mat = Peek::new(&mat).into_ndarray().unwrap();
    assert_eq!(mat.count(), m * n);
    assert_eq!(mat.n_dim(), 2);
    assert_eq!(mat.dim(0), Some(m));
    assert_eq!(mat.dim(1), Some(n));
    assert_eq!(mat.byte_stride(0).unwrap(), Some(size_of::<f64>() as isize));
    assert_eq!(
        mat.byte_stride(1).unwrap(),
        Some((m * size_of::<f64>()) as isize)
    );
    assert_eq!(mat.get(0).unwrap().get::<f64>().unwrap(), &0.0);
    assert_eq!(mat.get(1).unwrap().get::<f64>().unwrap(), &1.0);
    assert_eq!(mat.get(2).unwrap().get::<f64>().unwrap(), &2.0);
    assert_eq!(mat.get(3).unwrap().get::<f64>().unwrap(), &3.0);
    assert_eq!(mat.get(4).unwrap().get::<f64>().unwrap(), &2.0);
    assert_eq!(mat.get(5).unwrap().get::<f64>().unwrap(), &3.0);
    assert_eq!(mat.get(6).unwrap().get::<f64>().unwrap(), &4.0);
    assert_eq!(mat.get(7).unwrap().get::<f64>().unwrap(), &5.0);
    assert_eq!(
        mat.as_ptr().unwrap(),
        PtrConst::new(NonNull::new(ptr as *mut ()).unwrap())
    );
}
