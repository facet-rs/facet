#![doc = include_str!("../README.md")]

pub use styx_tokenizer::{Span, Token, TokenKind, Tokenizer};

mod events;
pub use events::{Event, ParseErrorKind, ScalarKind};

mod lexer;
pub use lexer::{Lexeme, Lexer};

mod parser;
pub use parser::Parser;
