mod basic;
mod list;
mod map;
mod scalar;
mod struct_;

/// Assert that the YAML used to serialize a value can be used to deserialize the value too.
#[macro_export]
macro_rules! assert_serialize {
    ($type:ty, $val:expr $(,)?) => {{
        use eyre::WrapErr as _;

        let value = $val;
        let serialized = facet_yaml::to_string(&value)?;
        let deserialized: $type = facet_yaml::from_str(&serialized)
            // Unfortunately we can't use the error as-is because it has a lifetime bound
            .map_err(|err| eyre::eyre!("{err}"))
            .wrap_err_with(|| format!("{value:?}"))
            .wrap_err(serialized)?;

        assert_eq!(deserialized, value);
    }};
}
