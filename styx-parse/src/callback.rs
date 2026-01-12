//! Callback trait for event-based parsing.

use crate::Event;

/// Trait for receiving parse events.
pub trait ParseCallback<'src> {
    /// Called for each event. Return `false` to stop parsing early.
    fn event(&mut self, event: Event<'src>) -> bool;
}

/// Convenience implementation: collect all events into a Vec.
impl<'src> ParseCallback<'src> for Vec<Event<'src>> {
    fn event(&mut self, event: Event<'src>) -> bool {
        self.push(event);
        true
    }
}

/// A callback that discards all events.
pub struct Discard;

impl<'src> ParseCallback<'src> for Discard {
    fn event(&mut self, _event: Event<'src>) -> bool {
        true
    }
}

/// A callback that counts events.
pub struct Counter {
    pub count: usize,
}

impl Counter {
    pub fn new() -> Self {
        Self { count: 0 }
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

impl<'src> ParseCallback<'src> for Counter {
    fn event(&mut self, _event: Event<'src>) -> bool {
        self.count += 1;
        true
    }
}
