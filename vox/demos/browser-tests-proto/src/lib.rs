//! Shared service definitions used by the rapace browser tests.

use rapace::prelude::*;

/// Request payload describing a list of integers.
#[derive(Clone, Debug, facet::Facet)]
pub struct NumbersRequest {
    pub values: Vec<i32>,
}

/// Statistical summary returned by `summarize_numbers`.
#[derive(Clone, Debug, facet::Facet)]
pub struct NumbersSummary {
    pub sum: i64,
    pub mean: f64,
    pub min: i32,
    pub max: i32,
}

/// Request payload for transforming a phrase.
#[derive(Clone, Debug, facet::Facet)]
pub struct PhraseRequest {
    pub phrase: String,
    pub shout: bool,
}

/// Response payload for the phrase transformation.
#[derive(Clone, Debug, facet::Facet)]
pub struct PhraseResponse {
    pub title: String,
    pub original_len: u32,
}

/// Streaming item produced by the countdown RPC.
#[derive(Clone, Debug, facet::Facet)]
pub struct CountEvent {
    pub value: u32,
    pub remaining: u32,
}

/// Service exercised by the browser tests.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait BrowserDemo {
    /// Compute aggregate statistics over an arbitrary sequence of integers.
    async fn summarize_numbers(&self, input: NumbersRequest) -> NumbersSummary;

    /// Transform a phrase and optionally "shout" it.
    async fn transform_phrase(&self, request: PhraseRequest) -> PhraseResponse;

    /// Stream a countdown starting from `start` down to zero.
    async fn countdown(&self, start: u32) -> Streaming<CountEvent>;
}
