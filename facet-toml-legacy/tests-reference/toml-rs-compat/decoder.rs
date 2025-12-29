#![cfg(all(feature = "parse", feature = "display", feature = "serde"))]
#![allow(dead_code)]

#[derive(Copy, Clone)]
pub(crate) struct Decoder;

impl toml_test_harness::Decoder for Decoder {
    fn name(&self) -> &str {
        "toml"
    }

    fn decode(
        &self,
        data: &[u8],
    ) -> Result<toml_test_harness::DecodedValue, toml_test_harness::Error> {
        use itertools::Itertools as _;
        use serde::de::Deserialize as _;

        let data = std::str::from_utf8(data).map_err(toml_test_harness::Error::new)?;
        let (table, errors) = toml::de::DeTable::parse_recoverable(data);
        if !errors.is_empty() {
            let errors = errors.into_iter().join("\n---\n");
            let error = toml_test_harness::Error::new(errors);
            return Err(error);
        }
        let document =
            toml::Table::deserialize(toml::de::Deserializer::from(table)).map_err(|mut err| {
                err.set_input(Some(data));
                toml_test_harness::Error::new(err)
            })?;
        let value = toml::Value::Table(document);
        value_to_decoded(&value)
    }
}

fn value_to_decoded(
    value: &toml::Value,
) -> Result<toml_test_harness::DecodedValue, toml_test_harness::Error> {
    match value {
        toml::Value::Integer(v) => Ok(toml_test_harness::DecodedValue::Scalar(
            toml_test_harness::DecodedScalar::from(*v),
        )),
        toml::Value::String(v) => Ok(toml_test_harness::DecodedValue::Scalar(
            toml_test_harness::DecodedScalar::from(v),
        )),
        toml::Value::Float(v) => Ok(toml_test_harness::DecodedValue::Scalar(
            toml_test_harness::DecodedScalar::from(*v),
        )),
        toml::Value::Datetime(v) => {
            let value = v.to_string();
            let value = match (v.date.is_some(), v.time.is_some(), v.offset.is_some()) {
                (true, true, true) => toml_test_harness::DecodedScalar::Datetime(value),
                (true, true, false) => toml_test_harness::DecodedScalar::DatetimeLocal(value),
                (true, false, false) => toml_test_harness::DecodedScalar::DateLocal(value),
                (false, true, false) => toml_test_harness::DecodedScalar::TimeLocal(value),
                _ => unreachable!("Unsupported case"),
            };
            Ok(toml_test_harness::DecodedValue::Scalar(value))
        }
        toml::Value::Boolean(v) => Ok(toml_test_harness::DecodedValue::Scalar(
            toml_test_harness::DecodedScalar::from(*v),
        )),
        toml::Value::Array(v) => {
            let v: Result<_, toml_test_harness::Error> = v.iter().map(value_to_decoded).collect();
            Ok(toml_test_harness::DecodedValue::Array(v?))
        }
        toml::Value::Table(v) => table_to_decoded(v),
    }
}

fn table_to_decoded(
    value: &toml::value::Table,
) -> Result<toml_test_harness::DecodedValue, toml_test_harness::Error> {
    let table: Result<_, toml_test_harness::Error> = value
        .iter()
        .map(|(k, v)| {
            let k = k.to_owned();
            let v = value_to_decoded(v)?;
            Ok((k, v))
        })
        .collect();
    Ok(toml_test_harness::DecodedValue::Table(table?))
}
