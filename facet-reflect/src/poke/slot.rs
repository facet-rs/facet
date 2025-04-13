use facet_core::{Facet, FieldError};

use crate::ReflectError;

use super::{PokeStructUninit, PokeValueUninit};

#[derive(Debug)]
/// Represents the parent container during the initialization process.
///
/// This enum tracks what kind of structure we're building within, so we can
/// navigate back up the initialization hierarchy when completing a field.
pub enum Parent<'mem> {
    /// An uninitialized struct that we're in the process of building.
    StructUninit(PokeStructUninit<'mem>),
    /// A struct field that itself contains a struct we're initializing.
    StructSlot(Box<StructSlot<'mem>>),
}

impl<'mem> Parent<'mem> {
    /// Assumes that the field is initialized, and returns the parent.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the field is initialized.
    unsafe fn assume_field_init(self, index: usize) -> Parent<'mem> {
        match self {
            Parent::StructUninit(mut storage) => {
                storage.iset.set(index);
                Parent::StructUninit(storage)
            }
            Parent::StructSlot(mut storage) => {
                storage.storage.iset.set(index);
                Parent::StructSlot(storage)
            }
        }
    }

    /// Returns the parent, assuming it's a PokeStructUninit
    pub fn into_struct_uninit(self) -> PokeStructUninit<'mem> {
        if let Parent::StructUninit(storage) = self {
            storage
        } else {
            panic!()
        }
    }

    /// Returns the parent, assuming it's a StructSlot
    pub fn into_struct_slot(self) -> Box<StructSlot<'mem>> {
        if let Parent::StructSlot(storage) = self {
            storage
        } else {
            panic!()
        }
    }
}

/// The memory location for a struct or enum field.
///
/// Setting it will mark it as initialized, and will allow us to resume access to the parent.
///
/// Maybe that slot is also a struct itself, in which case we'll need to nest deeper.
#[derive(Debug)]
pub struct Slot<'mem> {
    pub(crate) parent: Parent<'mem>,
    pub(crate) value: PokeValueUninit<'mem>,
    pub(crate) index: usize,
}

impl<'mem> Slot<'mem> {
    /// Assign this field, get back the parent with the field marked as initialized.
    pub fn set<T: Facet>(self, t: T) -> Result<Parent<'mem>, ReflectError> {
        let Self {
            value,
            parent,
            index,
        } = self;
        value.put(t)?;
        let parent = unsafe { parent.assume_field_init(index) };
        Ok(parent)
    }

    #[inline(always)]
    unsafe fn assume_init(self) -> Parent<'mem> {
        let Self { parent, index, .. } = self;
        unsafe { parent.assume_field_init(index) }
    }

    /// Assume this is a struct
    pub fn into_struct(self) -> Result<StructSlot<'mem>, ReflectError> {
        let Slot {
            parent,
            value,
            index,
        } = self;

        let data = value.data;
        let shape = value.shape();

        match value.into_struct() {
            Ok(storage) => Ok(StructSlot {
                slot: Slot {
                    parent,
                    value: PokeValueUninit { data, shape },
                    index,
                },
                storage,
            }),
            Err(_) => Err(ReflectError::WasNotAStruct),
        }
    }
}

/// A partially-initialized struct within a slot
#[derive(Debug)]
pub struct StructSlot<'mem> {
    /// the thing that we'll need to mark as initialized when we're done.
    slot: Slot<'mem>,

    /// what we're actually initializing
    storage: PokeStructUninit<'mem>,
}

impl<'mem> StructSlot<'mem> {
    /// Gets a slot for a given field, by name
    pub fn field_by_name(self, name: &str) -> Result<Slot<'mem>, FieldError> {
        let index = self
            .storage
            .def
            .fields
            .iter()
            .position(|f| f.name == name)
            .ok_or(FieldError::NoSuchField)?;

        self.field(index)
    }

    /// Gets a slot for a given field, by index
    pub fn field(self, index: usize) -> Result<Slot<'mem>, FieldError> {
        let field = self
            .storage
            .def
            .fields
            .get(index)
            .ok_or(FieldError::NoSuchField)?;
        let value = PokeValueUninit {
            data: unsafe { self.storage.value.data.field_uninit_at(field.offset) },
            shape: field.shape,
        };

        Ok(Slot {
            parent: Parent::StructSlot(Box::new(self)),
            value,
            index,
        })
    }

    /// Finish this struct
    pub fn finish(self) -> Result<Parent<'mem>, ReflectError> {
        self.storage.build_in_place()?;
        unsafe { Ok(self.slot.assume_init()) }
    }
}
