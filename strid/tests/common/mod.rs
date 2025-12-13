macro_rules! assert_impl_all_with_lifetime {
    ($type:ty: $($trait:path),+ $(,)?) => {
        const _: fn() = || {
            // Only callable when `$type` implements all traits in `$($trait)+`.
            fn assert_impl_all<'a, 'b: 'a, T: ?Sized $(+ $trait)+>() {}
            assert_impl_all::<$type>();
        };
    };
}

macro_rules! assert_core_impls {
    ($owned:ty => $borrowed:ty) => {
        assert_impl_all_with_lifetime!(
            $owned:
            std::convert::From<String>,
            std::convert::From<&'a str>,
            std::borrow::Borrow<str>,
        );

        assert_impl_all_with_lifetime!(
            $borrowed:
            std::borrow::Borrow<str>,
        );

        assert_impl_all_with_lifetime!(
            &$borrowed:
            std::convert::From<&'a str>,
        );

        assert_core_impls!($owned => $borrowed where ValidationError = std::convert::Infallible);
    };
    ($owned:ty => $borrowed:ty where NormalizationError = $error:ty, ValidationError = $verror:ty) => {
        assert_core_impls!($owned => $borrowed where Error = ($error, $verror));
    };
    ($owned:ty => $borrowed:ty where ValidationError = $error:ty) => {
        assert_impl_all_with_lifetime!(
            $owned:
            std::borrow::Borrow<str>,
        );

        assert_impl_all_with_lifetime!(
            $borrowed:
            std::borrow::Borrow<str>,
        );

        assert_core_impls!($owned => $borrowed where Error = ($error, $error));
    };
    ($owned:ty => $borrowed:ty where Error = ($error:ty, $verror:ty)) => {
        assert_impl_all_with_lifetime!(
            $owned:
            std::clone::Clone,
            std::fmt::Debug,
            std::fmt::Display,
            std::hash::Hash,
            std::cmp::Eq,
            std::cmp::Ord,
            std::cmp::PartialEq,
            std::cmp::PartialEq<$borrowed>,
            std::cmp::PartialEq<&'a $borrowed>,
            std::cmp::PartialOrd,
            std::convert::AsRef<$borrowed>,
            std::convert::AsRef<str>,
            std::convert::From<&'a $borrowed>,
            std::convert::From<Box<$borrowed>>,
            std::convert::From<std::borrow::Cow<'a, $borrowed>>,
            std::convert::TryFrom<String, Error = $error>,
            std::convert::TryFrom<&'a str, Error = $error>,
            std::borrow::Borrow<$borrowed>,
            std::str::FromStr<Err = $error>,
            std::ops::Deref<Target = $borrowed>,
        );

        assert_impl_all_with_lifetime!(
            $borrowed:
            std::fmt::Debug,
            std::fmt::Display,
            std::hash::Hash,
            std::cmp::Eq,
            std::cmp::Ord,
            std::cmp::PartialEq,
            std::cmp::PartialEq<$owned>,
            std::cmp::PartialOrd,
            std::borrow::ToOwned<Owned = $owned>,
        );

        assert_impl_all_with_lifetime!(
            &$borrowed:
            std::fmt::Debug,
            std::fmt::Display,
            std::hash::Hash,
            std::cmp::Eq,
            std::cmp::Ord,
            std::cmp::PartialEq,
            std::cmp::PartialEq<$owned>,
            std::cmp::PartialOrd,
            std::convert::From<&'a std::borrow::Cow<'b, $borrowed>>,
            std::convert::TryFrom<&'a str, Error = $verror>,
        );

        assert_impl_all_with_lifetime!(
            std::borrow::Cow<'static, $borrowed>:
            std::convert::From<$owned>,
        );

        assert_impl_all_with_lifetime!(
            std::borrow::Cow<$borrowed>:
            std::convert::From<&'a $borrowed>,
        );

        static_assertions::assert_impl_all!(
            Box<$borrowed>:
            std::convert::From<$owned>,
        );
    };
}

mod fallible;
mod infallible;
mod normalized;
