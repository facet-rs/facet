use facet::Facet;
use facet_json::DeserializeError;

pub(crate) fn from_str<T>(input: &str) -> Result<T, DeserializeError>
where
    T: Facet<'static>,
{
    facet_json::from_str(input)
}
