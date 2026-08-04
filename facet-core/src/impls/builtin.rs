use crate::{Facet, Opaque, OpaqueBorrow, Shape, VarianceDesc};

// Opaque<T> is a lifetime boundary; require 'static to prevent lifetime laundering
// through reflection. See issue #1563 for details.
unsafe impl<'facet, T: 'static> Facet<'facet> for Opaque<T> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Opaque<T>>("Opaque")
            .variance(VarianceDesc::INVARIANT)
            .build()
    };
}

// OpaqueBorrow<'facet, T> is used by derive-generated field-level `#[facet(opaque)]`
// wrappers so borrowed fields can stay tied to the active Facet lifetime.
unsafe impl<'facet, T: 'facet> Facet<'facet> for OpaqueBorrow<'facet, T> {
    const SHAPE: &'static Shape = &const {
        // MUST be built for `OpaqueBorrow<'facet, T>`, *not* `Opaque<T>`.
        //
        // `ConstTypeId` erases lifetimes by design, so `Opaque<&'a mut String>` and
        // `Opaque<&'static mut String>` are one and the same `ShapeId`. What keeps
        // `Poke::get_mut::<U>()` honest is not the id on its own — it is the id
        // *together with* the `U: Facet<'facet>` bound. `OpaqueBorrow<'x, T>` only
        // implements `Facet<'facet>` for `'facet == 'x`, so the bound pins the
        // requested lifetime to the one the value actually has. `Opaque<T>`
        // implements `Facet<'facet>` for *every* `'facet` (it is `T: 'static`
        // instead), so it offers no such pin.
        //
        // Handing `OpaqueBorrow` `Opaque`'s id therefore let safe code ask a
        // `Poke<'_, 'a>` over an `OpaqueBorrow<'a, &'a mut String>` for an
        // `&mut Opaque<&'static mut String>`: the ids compare equal, the bound is
        // vacuous, and both are `#[repr(transparent)]` over the same layout. That is
        // issue #1563 verbatim — fixed in bdf1dcfa6, reopened by cd254d928 (#2087).
        //
        // The identifier is "OpaqueBorrow" and not "Opaque" so that a rejection
        // can name what it rejected. While both wrappers answered to "Opaque",
        // the (correct) refusal to interchange them rendered as the useless
        // `Wrong shape: expected Opaque, but got Opaque`.
        Shape::builder_for_sized::<OpaqueBorrow<'facet, T>>("OpaqueBorrow")
            .variance(VarianceDesc::INVARIANT)
            .build()
    };
}
