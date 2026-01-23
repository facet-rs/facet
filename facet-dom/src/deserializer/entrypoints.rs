//! Deserializer construction and main entry points.
//!
//! This module contains the public API for creating deserializers and deserializing values.
//! These are separated from the implementation details for easy auditing.

use facet_core::Facet;
use facet_reflect::{HeapValue, Partial};

use super::DomDeserializer;
use crate::DomParser;
use crate::error::DomDeserializeError;

impl<'de, P> DomDeserializer<'de, true, P>
where
    P: DomParser<'de>,
{
    /// Create a new DOM deserializer that can borrow strings from input.
    pub fn new(parser: P) -> Self {
        Self {
            parser,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'de, P> DomDeserializer<'de, false, P>
where
    P: DomParser<'de>,
{
    /// Create a new DOM deserializer that produces owned strings.
    pub fn new_owned(parser: P) -> Self {
        Self {
            parser,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'de, P> DomDeserializer<'de, true, P>
where
    P: DomParser<'de>,
{
    /// Deserialize a value of type `T`, allowing borrowed strings from input.
    pub fn deserialize<T>(&mut self) -> Result<T, DomDeserializeError<P::Error>>
    where
        T: Facet<'de>,
    {
        let wip: Partial<'de, true> =
            Partial::alloc::<T>().map_err(DomDeserializeError::Reflect)?;
        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'de, true> =
            partial.build().map_err(DomDeserializeError::Reflect)?;
        heap_value
            .materialize::<T>()
            .map_err(DomDeserializeError::Reflect)
    }
}

impl<'de, P> DomDeserializer<'de, false, P>
where
    P: DomParser<'de>,
{
    /// Deserialize a value of type `T` into an owned type.
    pub fn deserialize<T>(&mut self) -> Result<T, DomDeserializeError<P::Error>>
    where
        T: Facet<'static>,
    {
        // SAFETY: When BORROW=false, no references into the input are stored.
        // The Partial only contains owned data (String, Vec, etc.), so the
        // lifetime parameter is purely phantom. We transmute from 'static to 'de
        // to satisfy the type system, but the actual data has no lifetime dependency.
        #[allow(unsafe_code)]
        let wip: Partial<'de, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'de, false>>(
                Partial::alloc_owned::<T>().map_err(DomDeserializeError::Reflect)?,
            )
        };
        let partial = self.deserialize_into(wip)?;
        // SAFETY: Same reasoning - with BORROW=false, HeapValue contains only
        // owned data. The 'de lifetime is phantom and we can safely transmute
        // back to 'static since T: Facet<'static>.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'de, false>, HeapValue<'static, false>>(
                partial.build().map_err(DomDeserializeError::Reflect)?,
            )
        };
        heap_value
            .materialize::<T>()
            .map_err(DomDeserializeError::Reflect)
    }
}
