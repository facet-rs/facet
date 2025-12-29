#![cfg(all(feature = "parse", feature = "display", feature = "serde"))]
#![allow(dead_code)]

#[derive(Copy, Clone)]
pub(crate) struct Encoder;

impl toml_test_harness::Encoder for Encoder {
    fn name(&self) -> &str {
        "toml"
    }

    fn encode(
        &self,
        data: toml_test_harness::DecodedValue,
    ) -> Result<String, toml_test_harness::Error> {
        let value = from_decoded(&data)?;
        let toml::Value::Table(document) = value else {
            return Err(toml_test_harness::Error::new("no root table"));
        };
        let s = toml::to_string(&document).map_err(toml_test_harness::Error::new)?;
        Ok(s)
    }
}

#[derive(Copy, Clone)]
pub(crate) struct EncoderPretty;

impl toml_test_harness::Encoder for EncoderPretty {
    fn name(&self) -> &str {
        "toml"
    }

    fn encode(
        &self,
        data: toml_test_harness::DecodedValue,
    ) -> Result<String, toml_test_harness::Error> {
        let value = from_decoded(&data)?;
        let toml::Value::Table(document) = value else {
            return Err(toml_test_harness::Error::new("no root table"));
        };
        let s = toml::to_string_pretty(&document).map_err(toml_test_harness::Error::new)?;
        Ok(s)
    }
}

fn from_decoded(
    decoded: &toml_test_harness::DecodedValue,
) -> Result<toml::Value, toml_test_harness::Error> {
    let value = match decoded {
        toml_test_harness::DecodedValue::Scalar(value) => from_decoded_scalar(value)?,
        toml_test_harness::DecodedValue::Table(value) => toml::Value::Table(from_table(value)?),
        toml_test_harness::DecodedValue::Array(value) => toml::Value::Array(from_array(value)?),
    };
    Ok(value)
}

fn from_decoded_scalar(
    decoded: &toml_test_harness::DecodedScalar,
) -> Result<toml::Value, toml_test_harness::Error> {
    match decoded {
        toml_test_harness::DecodedScalar::String(value) => Ok(toml::Value::String(value.clone())),
        toml_test_harness::DecodedScalar::Integer(value) => value
            .parse::<i64>()
            .map_err(toml_test_harness::Error::new)
            .map(toml::Value::Integer),
        toml_test_harness::DecodedScalar::Float(value) => value
            .parse::<f64>()
            .map_err(toml_test_harness::Error::new)
            .map(toml::Value::Float),
        toml_test_harness::DecodedScalar::Bool(value) => value
            .parse::<bool>()
            .map_err(toml_test_harness::Error::new)
            .map(toml::Value::Boolean),
        toml_test_harness::DecodedScalar::Datetime(value) => value
            .parse::<toml::value::Datetime>()
            .map_err(toml_test_harness::Error::new)
            .map(toml::Value::Datetime),
        toml_test_harness::DecodedScalar::DatetimeLocal(value) => value
            .parse::<toml::value::Datetime>()
            .map_err(toml_test_harness::Error::new)
            .map(toml::Value::Datetime),
        toml_test_harness::DecodedScalar::DateLocal(value) => value
            .parse::<toml::value::Datetime>()
            .map_err(toml_test_harness::Error::new)
            .map(toml::Value::Datetime),
        toml_test_harness::DecodedScalar::TimeLocal(value) => value
            .parse::<toml::value::Datetime>()
            .map_err(toml_test_harness::Error::new)
            .map(toml::Value::Datetime),
    }
}

fn from_table(
    decoded: &std::collections::HashMap<String, toml_test_harness::DecodedValue>,
) -> Result<toml::value::Table, toml_test_harness::Error> {
    decoded
        .iter()
        .map(|(k, v)| {
            let v = from_decoded(v)?;
            Ok((k.to_owned(), v))
        })
        .collect()
}

fn from_array(
    decoded: &[toml_test_harness::DecodedValue],
) -> Result<toml::value::Array, toml_test_harness::Error> {
    decoded.iter().map(from_decoded).collect()
}
