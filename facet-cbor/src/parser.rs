extern crate alloc;

use facet_format::{FormatParser, ParseError, ParseEvent, SavePoint};

pub struct CborParser<'de> {
    _marker: core::marker::PhantomData<&'de ()>,
}

impl<'de> CborParser<'de> {
    pub fn new(_input: &'de [u8]) -> Self {
        todo!()
    }
}

impl<'de> FormatParser<'de> for CborParser<'de> {
    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        todo!()
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        todo!()
    }

    fn skip_value(&mut self) -> Result<(), ParseError> {
        todo!()
    }

    fn save(&mut self) -> SavePoint {
        todo!()
    }

    fn restore(&mut self, _save_point: SavePoint) {
        todo!()
    }
}
