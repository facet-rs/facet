use core::fmt;

#[derive(Debug, Clone)]
pub struct CborError;

impl fmt::Display for CborError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl std::error::Error for CborError {}
