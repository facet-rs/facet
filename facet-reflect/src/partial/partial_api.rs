// This module contains the public-facing API for `Partial`

use ::alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};

use core::{marker::PhantomData, mem::ManuallyDrop, ptr::NonNull};

use crate::{
    Guard, HeapValue, Partial, Peek, ReflectError, ReflectErrorKind,
    partial::{
        DynamicObjectInsertState, DynamicValueState, Frame, FrameMode, FrameOwnership,
        MapInsertState, PartialState, Tracker, iset::ISet,
    },
    trace,
};
use facet_core::{
    ArrayType, Characteristic, Def, EnumRepr, EnumType, Facet, Field, KnownPointer, PtrConst,
    PtrMut, PtrUninit, SequenceType, Shape, StructType, Type, UserType, Variant,
};

mod alloc;
mod build;
mod eenum;
mod fields;
mod internal;
mod lists;
mod maps;
mod misc;
mod option;
mod ptr;
mod result;
mod set;
mod sets;
mod shorthands;
