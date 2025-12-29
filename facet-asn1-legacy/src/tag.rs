//! Parsing of ASN.1 type tags (referred to simply as "tags" in the spec).
//! In ASN.1, these are always u8 in size.
//!
//! These are parsed from the `#[facet(type_tag = ...)]` paramter in Facet types.
//! Keywords UNIVERSAL, APPLICATION, and PRIVATE are supported, but specifying the
//! raw tag from 0-255 is also fine.

use core::{num::ParseIntError, str::FromStr};

const ASN1_CLASS_UNIVERSAL: u8 = 0b00 << 6;
const ASN1_CLASS_APPLICATION: u8 = 0b01 << 6;
pub(crate) const ASN1_CLASS_CONTEXT_SPECIFIC: u8 = 0b10 << 6;
const ASN1_CLASS_PRIVATE: u8 = 0b11 << 6;

#[derive(Debug)]
pub enum Asn1TagError {
    Unknown,
    Int(ParseIntError),
}

impl core::fmt::Display for Asn1TagError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Asn1TagError::Unknown => write!(f, "Type tag couldn't be parsed into a BER/DER tag"),
            Asn1TagError::Int(_) => write!(f, "Raw type tag couldn't be parsed into a u8"),
        }
    }
}

impl core::error::Error for Asn1TagError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Asn1TagError::Int(parse_int_error) => Some(parse_int_error),
            _ => None,
        }
    }
}

enum Asn1TypeClass {
    Universal,
    Application,
    ContextSpecific,
    Private,
}

#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Eq)]
enum Asn1Type {
    BOOLEAN,
    INTEGER,
    BIT_STRING,
    OCTET_STRING,
    NULL,
    OBJECT_IDENTIFIER,
    ObjectDescriptor,
    EXTERNAL,
    REAL,
    ENUMERATED,
    EMBEDDED_PDV,
    UTF8String,
    RELATIVE_OID,
    TIME,
    SEQUENCE,
    SET,
    NumericString,
    PrintableString,
    TeletexString,
    T61String,
    VideotexString,
    IA5String,
    UTCTime,
    GeneralizedTime,
    GraphicString,
    VisibleString,
    GeneralString,
    UniversalString,
    CHARACTER_STRING,
    BMPString,
    DATE,
    TIME_OF_DAY,
    DATE_TIME,
    DURATION,
    Raw(u8),
}

pub struct Asn1TypeTag {
    class: Asn1TypeClass,
    tag: Asn1Type,
}

impl FromStr for Asn1TypeTag {
    type Err = Asn1TagError;

    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        let mut class: Option<Asn1TypeClass> = None;
        let mut tag: Option<Asn1Type> = None;
        let mut words = s.split_whitespace();
        while let Some(word) = words.next() {
            match word {
                "UNIVERSAL" => {
                    class = Some(Asn1TypeClass::Universal);
                }
                "APPLICATION" => {
                    class = Some(Asn1TypeClass::Application);
                }
                "CONTEXT" => {
                    if words.next() == Some("SPECIFIC") {
                        class = Some(Asn1TypeClass::ContextSpecific);
                    } else {
                        return Err(Asn1TagError::Unknown);
                    }
                }
                "PRIVATE" => {
                    class = Some(Asn1TypeClass::Private);
                }
                "BOOLEAN" => {
                    tag = Some(Asn1Type::BOOLEAN);
                    break;
                }
                "INTEGER" => {
                    tag = Some(Asn1Type::INTEGER);
                    break;
                }
                "BIT" => {
                    if words.next() == Some("STRING") {
                        tag = Some(Asn1Type::BIT_STRING);
                        break;
                    } else {
                        return Err(Asn1TagError::Unknown);
                    }
                }
                "OCTET" => {
                    if words.next() == Some("STRING") {
                        tag = Some(Asn1Type::OCTET_STRING);
                        break;
                    } else {
                        return Err(Asn1TagError::Unknown);
                    }
                }
                "NULL" => {
                    tag = Some(Asn1Type::NULL);
                    break;
                }
                "OBJECT" => {
                    if words.next() == Some("IDENTIFIER") {
                        tag = Some(Asn1Type::OBJECT_IDENTIFIER);
                        break;
                    } else {
                        return Err(Asn1TagError::Unknown);
                    }
                }
                "ObjectDescriptor" => {
                    tag = Some(Asn1Type::ObjectDescriptor);
                    break;
                }
                "EXTERNAL" => {
                    tag = Some(Asn1Type::EXTERNAL);
                    break;
                }
                "REAL" => {
                    tag = Some(Asn1Type::REAL);
                    break;
                }
                "ENUMERATED" => {
                    tag = Some(Asn1Type::ENUMERATED);
                    break;
                }
                "EMBEDDED" => {
                    if words.next() == Some("PDV") {
                        tag = Some(Asn1Type::EMBEDDED_PDV);
                        break;
                    } else {
                        return Err(Asn1TagError::Unknown);
                    }
                }
                "UTF8String" => {
                    tag = Some(Asn1Type::UTF8String);
                    break;
                }
                "RELATIVE-OID" => {
                    tag = Some(Asn1Type::RELATIVE_OID);
                    break;
                }
                "TIME" => {
                    tag = Some(Asn1Type::TIME);
                    break;
                }
                "SEQUENCE" => {
                    tag = Some(Asn1Type::SEQUENCE);
                    break;
                }
                "SET" => {
                    tag = Some(Asn1Type::SET);
                    break;
                }
                "NumericString" => {
                    tag = Some(Asn1Type::NumericString);
                    break;
                }
                "PrintableString" => {
                    tag = Some(Asn1Type::PrintableString);
                    break;
                }
                "TeletexString" => {
                    tag = Some(Asn1Type::TeletexString);
                    break;
                }
                "T61String" => {
                    tag = Some(Asn1Type::T61String);
                    break;
                }
                "VideotexString" => {
                    tag = Some(Asn1Type::VideotexString);
                    break;
                }
                "IA5String" => {
                    tag = Some(Asn1Type::IA5String);
                    break;
                }
                "UTCTime" => {
                    tag = Some(Asn1Type::UTCTime);
                    break;
                }
                "GeneralizedTime" => {
                    tag = Some(Asn1Type::GeneralizedTime);
                    break;
                }
                "GraphicString" => {
                    tag = Some(Asn1Type::GraphicString);
                    break;
                }
                "VisibleString" => {
                    tag = Some(Asn1Type::VisibleString);
                    break;
                }
                "GeneralString" => {
                    tag = Some(Asn1Type::GeneralString);
                    break;
                }
                "UniversalString" => {
                    tag = Some(Asn1Type::UniversalString);
                    break;
                }
                "CHARACTER STRING" => {
                    tag = Some(Asn1Type::CHARACTER_STRING);
                    break;
                }
                "BMPString" => {
                    tag = Some(Asn1Type::BMPString);
                    break;
                }
                "DATE" => {
                    tag = Some(Asn1Type::DATE);
                    break;
                }
                "TIME-OF-DAY" => {
                    tag = Some(Asn1Type::TIME_OF_DAY);
                    break;
                }
                "DATE-TIME" => {
                    tag = Some(Asn1Type::DATE_TIME);
                    break;
                }
                "DURATION" => {
                    tag = Some(Asn1Type::DURATION);
                    break;
                }
                raw => {
                    if class.is_none() {
                        class = Some(Asn1TypeClass::ContextSpecific);
                    }
                    let raw = u8::from_str(raw).map_err(Asn1TagError::Int)?;
                    tag = Some(Asn1Type::Raw(raw));
                    break;
                }
            }
        }
        let class = class.unwrap_or(Asn1TypeClass::Universal);
        if let Some(tag) = tag {
            Ok(Self { class, tag })
        } else {
            Err(Asn1TagError::Unknown)
        }
    }
}

impl Asn1TypeTag {
    pub(crate) fn ber(&self) -> u8 {
        let class = match self.class {
            Asn1TypeClass::Universal => ASN1_CLASS_UNIVERSAL,
            Asn1TypeClass::Application => ASN1_CLASS_APPLICATION,
            Asn1TypeClass::ContextSpecific => ASN1_CLASS_CONTEXT_SPECIFIC,
            Asn1TypeClass::Private => ASN1_CLASS_PRIVATE,
        };
        let value = match self.tag {
            Asn1Type::BOOLEAN => 1,
            Asn1Type::INTEGER => 2,
            Asn1Type::BIT_STRING => 3,
            Asn1Type::OCTET_STRING => 4,
            Asn1Type::NULL => 5,
            Asn1Type::OBJECT_IDENTIFIER => 6,
            Asn1Type::ObjectDescriptor => 7,
            Asn1Type::EXTERNAL => 8,
            Asn1Type::REAL => 9,
            Asn1Type::ENUMERATED => 10,
            Asn1Type::EMBEDDED_PDV => 11,
            Asn1Type::UTF8String => 12,
            Asn1Type::RELATIVE_OID => 13,
            Asn1Type::TIME => 14,
            Asn1Type::SEQUENCE => 16,
            Asn1Type::SET => 17,
            Asn1Type::NumericString => 18,
            Asn1Type::PrintableString => 19,
            Asn1Type::TeletexString | Asn1Type::T61String => 20,
            Asn1Type::VideotexString => 21,
            Asn1Type::IA5String => 22,
            Asn1Type::UTCTime => 23,
            Asn1Type::GeneralizedTime => 24,
            Asn1Type::GraphicString => 25,
            Asn1Type::VisibleString => 26,
            Asn1Type::GeneralString => 27,
            Asn1Type::UniversalString => 28,
            Asn1Type::CHARACTER_STRING => 29,
            Asn1Type::BMPString => 30,
            Asn1Type::DATE => 31,
            Asn1Type::TIME_OF_DAY => 32,
            Asn1Type::DATE_TIME => 33,
            Asn1Type::DURATION => 34,
            Asn1Type::Raw(r) => r,
        };
        value | class
    }
}
