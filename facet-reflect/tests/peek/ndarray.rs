use std::ops::{Index, IndexMut};

use facet::{Type, TypeParam, UserType};
use facet_core::{
    DeclId, Def, Facet, NdArrayDef, NdArrayVTable, OxPtrMut, PtrConst, PtrMut, Shape, ShapeBuilder,
    VTableIndirect, VarianceDesc,
};
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

// Monomorphized functions for NdArrayVTable
unsafe fn mat_count<T>(ptr: PtrConst) -> usize {
    let p = unsafe { ptr.get::<Mat<T>>() };
    p.nrows() * p.ncols()
}

fn mat_n_dim(_ptr: PtrConst) -> usize {
    2
}

unsafe fn mat_dim<T>(ptr: PtrConst, i: usize) -> Option<usize> {
    let p = unsafe { ptr.get::<Mat<T>>() };
    match i {
        0 => Some(p.nrows()),
        1 => Some(p.ncols()),
        _ => None,
    }
}

unsafe fn mat_get<T>(ptr: PtrConst, index: usize) -> Option<PtrConst> {
    let p = unsafe { ptr.get::<Mat<T>>() };
    let i = index % p.nrows();
    let index = index / p.nrows();
    let j = index % p.ncols();
    let index = index / p.ncols();

    if index != 0 {
        return None;
    }

    Some(PtrConst::new(&p[(i, j)] as *const _))
}

unsafe fn mat_get_mut<T>(ptr: PtrMut, index: usize) -> Option<PtrMut> {
    let p = unsafe { ptr.as_mut::<Mat<T>>() };
    let i = index % p.nrows();
    let index = index / p.nrows();
    let j = index % p.ncols();
    let index = index / p.ncols();

    if index != 0 {
        return None;
    }

    Some(PtrMut::new(&mut p[(i, j)] as *mut _))
}

unsafe fn mat_byte_stride<T>(ptr: PtrConst, i: usize) -> Option<isize> {
    let p = unsafe { ptr.get::<Mat<T>>() };
    match i {
        0 => Some(p.row_stride()),
        1 => Some(p.col_stride()),
        _ => None,
    }
}

unsafe fn mat_as_ptr<T>(ptr: PtrConst) -> PtrConst {
    PtrConst::new(unsafe { ptr.get::<Mat<T>>().as_ptr() })
}

unsafe fn mat_as_mut_ptr<T>(ptr: PtrMut) -> PtrMut {
    PtrMut::new(unsafe { ptr.as_mut::<Mat<T>>().as_mut_ptr() })
}

// Monomorphized functions for VTableIndirect
unsafe fn mat_drop<T>(ox: OxPtrMut) {
    unsafe {
        let ptr = ox.ptr();
        core::ptr::drop_in_place(ptr.as_ptr::<Mat<T>>() as *mut Mat<T>);
    }
}

const fn build_ndarray_vtable<T>() -> NdArrayVTable {
    NdArrayVTable {
        count: mat_count::<T>,
        n_dim: mat_n_dim,
        dim: mat_dim::<T>,
        get: mat_get::<T>,
        get_mut: Some(mat_get_mut::<T>),
        byte_stride: Some(mat_byte_stride::<T>),
        as_ptr: Some(mat_as_ptr::<T>),
        as_mut_ptr: Some(mat_as_mut_ptr::<T>),
    }
}

const fn build_type_ops<T>() -> facet_core::TypeOpsIndirect {
    facet_core::TypeOpsIndirect {
        drop_in_place: mat_drop::<T>,
        default_in_place: None,
        clone_into: None,
        is_truthy: None,
    }
}

fn type_name_mat<T: Facet<'static>>(
    _shape: &'static facet_core::Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: facet_core::TypeNameOpts,
) -> core::fmt::Result {
    f.write_str("Mat<")?;
    match opts.for_children() {
        Some(opts) => T::SHAPE.write_type_name(f, opts)?,
        None => f.write_str("…")?,
    }
    f.write_str(">")
}

unsafe impl<T: Facet<'static>> Facet<'static> for Mat<T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Mat<T>>("Mat")
            .decl_id(DeclId::new(facet_core::decl_id_hash("Mat")))
            .type_name(type_name_mat::<T>)
            .ty(Type::User(UserType::Opaque))
            .def(Def::NdArray(NdArrayDef {
                vtable: &const { build_ndarray_vtable::<T>() },
                t: T::SHAPE,
            }))
            .type_tag("Mat")
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(&const { build_type_ops::<T>() })
            .variance(VarianceDesc::INVARIANT)
            .build()
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
