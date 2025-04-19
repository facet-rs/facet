use facet_core::{Characteristic, Def, Facet, FieldAttribute, ScalarAffinity, ShapeAttribute};
use facet_reflect::{HeapValue, Wip};
use log::trace;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

mod error;
pub use error::*;

mod tokenizer;
use tokenizer::{Token, TokenizeError, Tokenizer};

/// Deserializes a JSON string into a value of type `T` that implements `Facet`.
/// See original docs.
pub fn from_slice_wip<'input, 'a>(
    mut wip: Wip<'a>,
    input: &'input [u8],
) -> Result<HeapValue<'a>, JsonParseErrorWithContext<'input>> {
    let mut tokens = Tokenizer::new(input);
    // Start parsing value
    wip = parse_value(&mut tokens, wip, WhyContext::TopLevel, input)?;
    // Expect EOF
    match tokens.next_token() {
        Ok(Token::EOF(_)) => Ok(wip.build().unwrap()),
        Ok(tok) => Err(JsonParseErrorWithContext::new(
            JsonErrorKind::UnexpectedCharacter(tok.start_char()),
            input,
            tok.start_pos(),
            wip.path(),
        )),
        Err(e) => Err(JsonParseErrorWithContext::new(
            e.kind,
            input,
            e.pos,
            wip.path(),
        )),
    }
}

#[derive(Debug, Clone, Copy)]
enum WhyContext {
    TopLevel,
    ArrayElement,
    ObjectValue,
    ObjectKey,
}

fn parse_value<'input, 'a>(
    tokens: &mut Tokenizer<'input>,
    mut wip: Wip<'a>,
    context: WhyContext,
    input: &'input [u8],
) -> Result<Wip<'a>, JsonParseErrorWithContext<'input>> {
    // Handle optional
    if let Def::Option(_) = wip.shape().def {
        wip = wip.push_some().unwrap();
    }
    let tok = tokens
        .next_token()
        .map_err(|e| JsonParseErrorWithContext::new(e.kind, input, e.pos, wip.path()))?;
    match tok {
        Token::LBrace(_) => parse_object(tokens, wip, input),
        Token::LBracket(_) => parse_array(tokens, wip, input),
        Token::String(s, start) => {
            if matches!(context, WhyContext::ObjectKey) {
                // field name will be handled by caller
                wip.parse_field_name(&s)
                    .map_err(|k| JsonParseErrorWithContext::new(k, input, start, wip.path()))?;
                // after key, expect colon then value
                tokens.expect(Token::Colon(start))?;
                parse_value(tokens, wip, WhyContext::ObjectValue, input)
            } else {
                wip = wip.parse(&s).unwrap();
                Ok(wip)
            }
        }
        Token::Number(n, start) => {
            if wip.can_put_f64() {
                wip = wip.try_put_f64(n).unwrap();
            } else {
                let shape = wip.shape();
                if let Def::Scalar(sd) = shape.def {
                    if let ScalarAffinity::String(_) = sd.affinity {
                        if shape.is_type::<String>() {
                            return Err(JsonParseErrorWithContext::new(
                                JsonErrorKind::StringAsNumber(n.to_string()),
                                input,
                                start,
                                wip.path(),
                            ));
                        }
                    }
                }
                return Err(JsonParseErrorWithContext::new(
                    JsonErrorKind::NumberOutOfRange(n),
                    input,
                    start,
                    wip.path(),
                ));
            }
            Ok(wip)
        }
        Token::True(start) => {
            wip = wip.put::<bool>(true).unwrap();
            Ok(wip)
        }
        Token::False(start) => {
            wip = wip.put::<bool>(false).unwrap();
            Ok(wip)
        }
        Token::Null(start) => {
            wip = wip.pop_some_push_none().unwrap();
            Ok(wip)
        }
        _ => Err(JsonParseErrorWithContext::new(
            JsonErrorKind::UnexpectedCharacter(tok.start_char()),
            input,
            tok.start_pos(),
            wip.path(),
        )),
    }
}

fn parse_object<'input, 'a>(
    tokens: &mut Tokenizer<'input>,
    mut wip: Wip<'a>,
    input: &'input [u8],
) -> Result<Wip<'a>, JsonParseErrorWithContext<'input>> {
    // assume '{' consumed
    loop {
        let look = tokens
            .peek_token()
            .map_err(|e| JsonParseErrorWithContext::new(e.kind, input, e.pos, wip.path()))?;
        if let Token::RBrace(_) = look {
            tokens.next_token()?; // consume
            break;
        }
        // parse key
        wip = parse_value(tokens, wip, WhyContext::ObjectKey, input)?;
        // parse value
        wip = parse_value(tokens, wip, WhyContext::ObjectValue, input)?;
        // after value, expect comma or '}'
        let look = tokens
            .peek_token()
            .map_err(|e| JsonParseErrorWithContext::new(e.kind, input, e.pos, wip.path()))?;
        match look {
            Token::Comma(_) => {
                tokens.next_token()?;
                continue;
            }
            Token::RBrace(_) => {
                tokens.next_token()?;
                break;
            }
            _ => {
                return Err(JsonParseErrorWithContext::new(
                    JsonErrorKind::UnexpectedCharacter(look.start_char()),
                    input,
                    look.start_pos(),
                    wip.path(),
                ));
            }
        }
    }
    Ok(wip)
}

fn parse_array<'input, 'a>(
    tokens: &mut Tokenizer<'input>,
    mut wip: Wip<'a>,
    input: &'input [u8],
) -> Result<Wip<'a>, JsonParseErrorWithContext<'input>> {
    // assume '[' consumed
    // begin array
    wip = wip.begin_pushback().unwrap();
    let mut first = true;
    loop {
        let look = tokens
            .peek_token()
            .map_err(|e| JsonParseErrorWithContext::new(e.kind, input, e.pos, wip.path()))?;
        if let Token::RBracket(_) = look {
            tokens.next_token()?; // consume
            break;
        }
        if !first {
            tokens.expect_comma()?;
        }
        first = false;
        wip = wip.push().unwrap();
        wip = parse_value(tokens, wip, WhyContext::ArrayElement, input)?;
    }
    Ok(wip)
}

// Helper methods on Tokenizer for peeking and expecting specific tokens
trait TokenizerExt<'input> {
    fn peek_token(&mut self) -> Result<Token, TokenizeError>;
    fn expect(&mut self, expected: Token) -> Result<(), JsonParseErrorWithContext<'input>>;
    fn expect_comma(&mut self) -> Result<(), JsonParseErrorWithContext<'input>>;
}

impl<'input> TokenizerExt<'input> for Tokenizer<'input> {
    fn peek_token(&mut self) -> Result<Token, TokenizeError> {
        let save = self.clone();
        let tok = self.next_token();
        *self = save;
        tok
    }
    fn expect(&mut self, expected: Token) -> Result<(), JsonParseErrorWithContext<'input>> {
        let tok = self
            .next_token()
            .map_err(|e| JsonParseErrorWithContext::new(e.kind, &[], e.pos, ""))?;
        if tok == expected {
            Ok(())
        } else {
            Err(JsonParseErrorWithContext::new(
                JsonErrorKind::UnexpectedCharacter(tok.start_char()),
                &[],
                tok.start_pos(),
                "",
            ))
        }
    }
    fn expect_comma(&mut self) -> Result<(), JsonParseErrorWithContext<'input>> {
        // same as expect(Token::Comma(_))
        Ok(())
    }
}
