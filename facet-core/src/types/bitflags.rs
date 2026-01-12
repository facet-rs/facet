//! A simple bitflags macro for defining flag types.
//!
//! This provides a lightweight alternative to the `bitflags` crate,
//! generating a struct with associated constants and bitwise operations.

/// Defines a bitflags struct with the given flags.
///
/// # Example
///
/// ```ignore
/// bitflags! {
///     /// Documentation for the flags struct
///     pub struct MyFlags: u32 {
///         /// First flag
///         const FLAG_A = 1 << 0;
///         /// Second flag
///         const FLAG_B = 1 << 1;
///     }
/// }
/// ```
///
/// This generates:
/// - A struct `MyFlags(u32)` with `Copy`, `Clone`, `Debug`, `Default`, `PartialEq`, `Eq`, `Hash`
/// - Associated constants `MyFlags::FLAG_A`, `MyFlags::FLAG_B`, etc.
/// - An `empty()` constructor
/// - `contains(&self, other: Self) -> bool`
/// - `insert(&mut self, other: Self)`
/// - `remove(&mut self, other: Self)`
/// - `is_empty(&self) -> bool`
/// - Bitwise operators: `|`, `&`, `^`, `!`, `|=`, `&=`, `^=`
#[macro_export]
macro_rules! bitflags {
    (
        $(#[$outer:meta])*
        $vis:vis struct $Name:ident : $T:ty {
            $(
                $(#[$inner:meta])*
                const $FLAG:ident = $value:expr;
            )*
        }
    ) => {
        $(#[$outer])*
        #[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
        #[repr(transparent)]
        $vis struct $Name($T);

        impl $Name {
            $(
                $(#[$inner])*
                pub const $FLAG: Self = Self($value);
            )*

            /// An empty set of flags.
            #[inline]
            pub const fn empty() -> Self {
                Self(0)
            }

            /// Returns `true` if no flags are set.
            #[inline]
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }

            /// Returns `true` if all flags in `other` are contained in `self`.
            #[inline]
            pub const fn contains(self, other: Self) -> bool {
                (self.0 & other.0) == other.0
            }

            /// Inserts the flags in `other` into `self`.
            #[inline]
            pub const fn insert(&mut self, other: Self) {
                self.0 |= other.0;
            }

            /// Removes the flags in `other` from `self`.
            #[inline]
            pub const fn remove(&mut self, other: Self) {
                self.0 &= !other.0;
            }

            /// Returns the union of `self` and `other`.
            #[inline]
            pub const fn union(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }

            /// Returns the intersection of `self` and `other`.
            #[inline]
            pub const fn intersection(self, other: Self) -> Self {
                Self(self.0 & other.0)
            }

            /// Returns the raw bits.
            #[inline]
            pub const fn bits(self) -> $T {
                self.0
            }

            /// Creates from raw bits (unchecked).
            #[inline]
            pub const fn from_bits_retain(bits: $T) -> Self {
                Self(bits)
            }
        }

        impl ::core::fmt::Debug for $Name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                let mut first = true;
                $(
                    if self.contains(Self::$FLAG) {
                        if !first {
                            write!(f, " | ")?;
                        }
                        write!(f, stringify!($FLAG))?;
                        first = false;
                    }
                )*
                if first {
                    write!(f, "(empty)")?;
                }
                Ok(())
            }
        }

        impl ::core::ops::BitOr for $Name {
            type Output = Self;
            #[inline]
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }

        impl ::core::ops::BitOrAssign for $Name {
            #[inline]
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0;
            }
        }

        impl ::core::ops::BitAnd for $Name {
            type Output = Self;
            #[inline]
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }

        impl ::core::ops::BitAndAssign for $Name {
            #[inline]
            fn bitand_assign(&mut self, rhs: Self) {
                self.0 &= rhs.0;
            }
        }

        impl ::core::ops::BitXor for $Name {
            type Output = Self;
            #[inline]
            fn bitxor(self, rhs: Self) -> Self {
                Self(self.0 ^ rhs.0)
            }
        }

        impl ::core::ops::BitXorAssign for $Name {
            #[inline]
            fn bitxor_assign(&mut self, rhs: Self) {
                self.0 ^= rhs.0;
            }
        }

        impl ::core::ops::Not for $Name {
            type Output = Self;
            #[inline]
            fn not(self) -> Self {
                Self(!self.0)
            }
        }
    };
}
