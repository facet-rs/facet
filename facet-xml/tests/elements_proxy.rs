use facet::Facet;
use facet_testhelpers::test;
use facet_xml as xml;

/// A proxy type that formats values as binary strings (for XML).
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct BinaryString(pub String);

// XML proxy conversion: u32 <-> binary string
impl TryFrom<BinaryString> for u32 {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: BinaryString) -> Result<Self, Self::Error> {
        u32::from_str_radix(proxy.0.trim_start_matches("0b"), 2)
    }
}

impl From<&u32> for BinaryString {
    fn from(v: &u32) -> Self {
        BinaryString(format!("0b{:b}", v))
    }
}

impl TryFrom<BinaryString> for SomeInteger {
    type Error = std::num::ParseIntError;
    fn try_from(value: BinaryString) -> Result<Self, Self::Error> {
        Ok(Self(value.try_into()?))
    }
}

impl From<&SomeInteger> for BinaryString {
    fn from(value: &SomeInteger) -> Self {
        (&value.0).into()
    }
}

#[derive(Debug, Facet)]
#[facet(transparent, xml::proxy = BinaryString)]
struct SomeInteger(u32);

#[derive(Debug, Facet)]
struct Container {
    #[facet(xml::elements)]
    elements: Vec<SomeInteger>,
}

#[test]
fn elements_collection_uses_proxy() {
    let proxy_used: SomeInteger = facet_xml::from_str(
        r#"
<someInteger>101</someInteger>"#,
    )
    .unwrap();
    // works
    assert_eq!(proxy_used.0, 5);

    let xml = r#"
<container>
<someInteger>101</someInteger>
<someInteger>111</someInteger>
<someInteger>1</someInteger>
</container>"#;

    // parses empty due to
    let value: Container = facet_xml::from_str(xml).unwrap();
    let a = &value.elements[0];
    let b = &value.elements[1];
    let c = &value.elements[2];
    assert_eq!(a.0, 5);
    assert_eq!(b.0, 7);
    assert_eq!(c.0, 1);
}
