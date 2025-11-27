#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod diff;
mod display;
mod sequences;

pub use diff::Diff;
pub use diff::FacetDiff;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
