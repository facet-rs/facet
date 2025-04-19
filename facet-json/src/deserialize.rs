use facet_core::{Def, ScalarAffinity};
use facet_reflect::{HeapValue, Wip};

use alloc::string::{String, ToString};

mod error;
pub use error::*;

mod tokenizer;
use tokenizer::{Spanned, Token, TokenizeError, Tokenizer};

/// Deserializes a JSON string into a value of type `T` that implements `Facet`.
pub fn from_slice_wip<'input, 'a>(
    mut wip: Wip<'a>,
    input: &'input [u8],
) -> Result<HeapValue<'a>, JsonParseErrorWithContext<'input>> {
    let mut tokens = Tokenizer::new(input, wip.path());
    wip = parse_value(&mut tokens, wip, WhyContext::TopLevel, input)?;
    match tokens.next_token() {
        Ok(sp) if matches!(sp.node, Token::EOF) => Ok(wip.build().unwrap()),
        Ok(sp) => Err(JsonParseErrorWithContext::new(
            JsonErrorKind::UnexpectedCharacter(token_char(&sp.node)),
            input,
            sp.span.start(),
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
    if let Def::Option(_) = wip.shape().def {
        wip = wip.push_some().unwrap();
    }
    let sp = tokens
        .next_token()
        .map_err(|e| JsonParseErrorWithContext::new(e.kind, input, e.pos, wip.path().to_string()))?;
    match sp.node {
        Token::LBrace => parse_object(tokens, wip, input),
        Token::LBracket => parse_array(tokens, wip, input),
        Token::String(s) => {
            if matches!(context, WhyContext::ObjectKey) {
                wip.field_named(&s).map_err(|e| {
                    JsonParseErrorWithContext::new(
                        JsonErrorKind::ReflectError(e),
                        input,
                        sp.span.start(),
                        wip.path(),
                    )
                })?;
                tokens.expect(Token::Colon, input, &wip.path())?;
                parse_value(tokens, wip, WhyContext::ObjectValue, input)
            } else {
                wip = wip.parse(&s).unwrap();
                Ok(wip)
            }
        }
        Token::Number(n) => {
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
                                sp.span.start(),
                                wip.path(),
                            ));
                        }
                    }
                }
                return Err(JsonParseErrorWithContext::new(
                    JsonErrorKind::NumberOutOfRange(n),
                    input,
                    sp.span.start(),
                    wip.path(),
                ));
            }
            Ok(wip)
        }
        Token::True => {
            wip = wip.put::<bool>(true).unwrap();
            Ok(wip)
        }
        Token::False => {
            wip = wip.put::<bool>(false).unwrap();
            Ok(wip)
        }
        Token::Null => {
            wip = wip.pop_some_push_none().unwrap();
            Ok(wip)
        }
        _ => Err(JsonParseErrorWithContext::new(
            JsonErrorKind::UnexpectedCharacter(token_char(&sp.node)),
            input,
            sp.span.start(),
            wip.path(),
        )),
    }
}

fn parse_object<'input, 'a>(
    tokens: &mut Tokenizer<'input>,
    mut wip: Wip<'a>,
    input: &'input [u8],
) -> Result<Wip<'a>, JsonParseErrorWithContext<'input>> {
    loop {
        let look = tokens
            .peek_token()
            .map_err(|e| JsonParseErrorWithContext::new(JsonErrorKind::from(e), input, e.pos, tokens.path.clone()))?;
        if matches!(look.node, Token::RBrace) {
            tokens.next_token()?;
            break;
        }
        wip = parse_value(tokens, wip, WhyContext::ObjectKey, input)?;
        wip = parse_value(tokens, wip, WhyContext::ObjectValue, input)?;
        let look = tokens
            .peek_token()
            .map_err(|e| JsonParseErrorWithContext::new(e.kind, input, e.pos, wip.path()))?;
        match look.node {
            Token::Comma => {
                tokens.next_token()?;
                continue;
            }
            Token::RBrace => {
                tokens.next_token()?;
                break;
            }
            _ => {
                return Err(JsonParseErrorWithContext::new(
                    JsonErrorKind::UnexpectedCharacter(token_char(&look.node)),
                    input,
                    look.span.start(),
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
    wip = wip.begin_pushback().unwrap();
    let mut first = true;
    loop {
        let look = tokens
            .peek_token()
            .map_err(|e| JsonParseErrorWithContext::new(e.kind, input, e.pos, wip.path()))?;
        if matches!(look.node, Token::RBracket) {
            tokens.next_token()?;
            break;
        }
        if !first {
            tokens.expect(Token::Comma, input, wip.path())?;
        }
        first = false;
        wip = wip.push().unwrap();
        wip = parse_value(tokens, wip, WhyContext::ArrayElement, input)?;
    }
    Ok(wip)
}

/// Returns representative char for JSON error reporting
fn token_char(t: &Token) -> char {
    match t {
        Token::LBrace => '{',
        Token::RBrace => '}',
        Token::LBracket => '[',
        Token::RBracket => ']',
        Token::Colon => ':',
        Token::Comma => ',',
        Token::String(_) => '"',
        Token::Number(_) => '0',
        Token::True => 't',
        Token::False => 'f',
        Token::Null => 'n',
        Token::EOF => '$',
    }
}

trait TokenizerExt<'input> {
    fn peek_token(&mut self) -> Result<Spanned<Token>, TokenizeError>;
    fn expect(
        &mut self,
        expected: Token,
        input: &'input [u8],
        path: &str,
    ) -> Result<(), JsonParseErrorWithContext<'input>>;
}

impl<'input> TokenizerExt<'input> for Tokenizer<'input> {
    fn peek_token(&mut self) -> Result<Spanned<Token>, TokenizeError> {
        let save = self.clone();
        let sp = self.next_token();
        *self = save;
        sp
    }
    fn expect(
        &mut self,
        expected: Token,
        input: &'input [u8],
        path: &str,
    ) -> Result<(), JsonParseErrorWithContext<'input>> {
        let sp = self
            .next_token()
            .map_err(|e| JsonParseErrorWithContext::new(e.kind, input, e.pos, path.to_string()))?;
        if sp.node == expected {
            Ok(())
        } else {
            Err(JsonParseErrorWithContext::new(
                JsonErrorKind::UnexpectedCharacter(token_char(&sp.node)),
                input,
                sp.span.start(),
                path,
            ))
        }
    }
}
       }
    }
}
