use facet_core::Facet;
use facet_deserialize::{DeserError, DeserErrorKind, Span};
use facet_reflect::Wip;

use crate::tokenizer::{Token, TokenError, TokenErrorKind, Tokenizer};

/// Deserialize JSON from a given string
pub fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input str,
) -> Result<T, DeserError<'input>> {
    let input = input.as_bytes();
    from_slice(input)
}

/// Deserialize JSON from a given byte slice
pub fn from_slice<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input [u8],
) -> Result<T, DeserError<'input>> {
    let wip = Wip::alloc_shape(T::SHAPE).map_err(|e| {
        DeserError::new_reflect(e, input, Span { start: 0, len: 0 }, "json-recursive")
    })?;
    let mut tokens = Tokenizer::new(input);
    let wip = from_slice_into_wip::<T>(&mut tokens, wip, 0)?;
    let built = wip.build().map_err(|e| {
        DeserError::new_reflect(e, input, Span { start: 0, len: 0 }, "json-recursive")
    })?;
    built
        .materialize()
        .map_err(|e| DeserError::new_reflect(e, input, Span { start: 0, len: 0 }, "json-recursive"))
}

fn from_slice_into_wip<'input: 'facet, 'facet, T: Facet<'facet>>(
    tokens: &mut Tokenizer<'input>,
    mut wip: Wip<'facet>,
    recursion_depth: usize,
) -> Result<Wip<'facet>, DeserError<'input>> {
    let next_token = tokens
        .next_token()
        .map_err(|err| convert_token_error(err, tokens))?;

    match (next_token.node, wip.shape().def) {
        (Token::LBrace,) => wip = from_object::<T>(tokens, wip, recursion_depth)?,
        Token::LBracket => wip = from_array::<T>(tokens, wip, recursion_depth)?,
        (Token::F64(f),) => wip = wip.put(f).unwrap(),
        _ => panic!(),
    }

    Ok(wip)
}

fn from_object<'input: 'facet, 'facet, T: Facet<'facet>>(
    tokens: &mut Tokenizer<'input>,
    mut wip: Wip<'facet>,
    recursion_depth: usize,
) -> Result<Wip<'facet>, DeserError<'input>> {
    loop {
        match tokens
            .next_token()
            .map_err(|err| convert_token_error(err, tokens))?
            .node
        {
            Token::RBrace => return Ok(wip),
            Token::String(key) => {
                wip = wip.field_named(&key).unwrap();
                let Token::Colon = tokens.next_token()?.node else {
                    return Err(DeserError::UnexpectedToken(tokens.next_token()?));
                };
                let value = from_slice_into_wip(tokens)?;
                object.insert(key, value);
            }
            _ => return Err(DeserError::UnexpectedToken(tokens.next_token()?)),
        }
    }
}

fn from_array<'input: 'facet, 'facet, T: Facet<'facet>>(
    tokens: &mut Tokenizer<'input>,
    mut wip: Wip<'facet>,
    recursion_depth: usize,
) -> Result<Wip<'facet>, DeserError<'input>> {
    let wip = wip.begin_pushback().unwrap();
    loop {
        match tokens
            .next_token()
            .map_err(|err| convert_token_error(err, tokens))?
            .node
        {
            Token::RBracket => {
                wip = wip.pop().unwrap();
                return Ok(wip);
            }
            Token::Value(value) => wip = wip.push(value),
        }
    }
}

fn convert_token_error<'input>(
    err: TokenError,
    tokens: &mut Tokenizer<'input>,
) -> DeserError<'input> {
    DeserError {
        input: tokens.get_input().into(),
        span: err.span,
        kind: match err.kind {
            TokenErrorKind::UnexpectedCharacter(c) => DeserErrorKind::UnexpectedChar {
                got: c,
                wanted: "valid JSON character",
            },
            TokenErrorKind::UnexpectedEof(why) => DeserErrorKind::UnexpectedEof { wanted: why },
            TokenErrorKind::InvalidUtf8(s) => DeserErrorKind::InvalidUtf8(s),
            TokenErrorKind::NumberOutOfRange(number) => DeserErrorKind::NumberOutOfRange(number),
        },
        source_id: "facet-json-recursive",
    }
}
