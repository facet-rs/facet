#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

// cf. facet-toml/facet-json for examples

use std::{
    any::type_name,
    fmt::{Debug, Display},
};

use facet_core::Facet;
use facet_reflect::Wip;
use kdl::{KdlDocument, KdlError, KdlNode};

// FIXME: Naming?
#[derive(Debug)]
enum State<T> {
    ExpectingNode,
    ProcessingNode(KdlNode),
    Success(T),
}

// QUESTION: Any interest in making something a bit like `strum` with `facet`? Always nice to have an easy way to get
// the names of enum variants as strings! This is just a hack for the time being...
impl<T> Display for State<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match self {
            State::ExpectingNode => "ExpectingNode",
            State::ProcessingNode(_) => "ProcessingNode",
            State::Success(_) => "Success",
        };

        write!(f, "{state}")
    }
}

pub fn from_str<'input, 'facet, T>(kdl: &'input str) -> Result<T, KdlError>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    log::trace!("Entering `from_str` function");

    let kdl: KdlDocument = dbg!(kdl.parse()?);
    log::trace!("KDL parsed");

    let mut wip = Wip::alloc::<T>().expect("failed to allocate");
    // QUESTION: Does `facet` provide anything like `type_name`? I think that's important for no-std support and it
    // would be nice to have something that's a bit more reliable than a "best-effort description"
    log::trace!("Allocated WIP for type {}", type_name::<T>());

    // TODO: This should depend on `wip`?
    let mut state = State::ExpectingNode;

    loop {
        log::trace!("Current state is: {state}");
        match state {
            State::ExpectingNode => todo!(),
            State::ProcessingNode(kdl_node) => todo!(),
            State::Success(result) => return Ok(result),
        }
    }
}
