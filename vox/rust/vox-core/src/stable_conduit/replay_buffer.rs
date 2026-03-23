use std::collections::VecDeque;

use facet::Facet;

/// Monotonically increasing sequence number for outbound frames.
#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
#[facet(transparent)]
pub(crate) struct PacketSeq(pub(crate) u32);

/// Acknowledgement: the highest seq the peer has received.
#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PacketAck {
    pub(crate) max_delivered: PacketSeq,
}

/// Buffer of un-acknowledged outbound frames for replay on reconnect.
///
/// Each entry is `(seq, encoded_item_bytes)`. The frame header is not stored —
/// it is regenerated on replay so the ack field reflects current state.
pub(crate) struct ReplayBuffer {
    entries: VecDeque<(PacketSeq, Vec<u8>)>,
}

impl ReplayBuffer {
    pub(crate) fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    /// Push a newly sent frame into the buffer.
    pub(crate) fn push(&mut self, seq: PacketSeq, item_bytes: Vec<u8>) {
        self.entries.push_back((seq, item_bytes));
    }

    /// Discard all entries with `seq <= ack.max_delivered`.
    pub(crate) fn trim(&mut self, ack: PacketAck) {
        while self
            .entries
            .front()
            .is_some_and(|(s, _)| *s <= ack.max_delivered)
        {
            self.entries.pop_front();
        }
    }

    /// Iterate buffered entries for replay, oldest first.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &(PacketSeq, Vec<u8>)> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(n: u32) -> PacketSeq {
        PacketSeq(n)
    }

    fn ack(n: u32) -> PacketAck {
        PacketAck {
            max_delivered: seq(n),
        }
    }

    #[test]
    fn push_and_iter() {
        let mut buf = ReplayBuffer::new();
        buf.push(seq(0), b"a".to_vec());
        buf.push(seq(1), b"b".to_vec());
        buf.push(seq(2), b"c".to_vec());

        let items: Vec<_> = buf.iter().map(|(s, b)| (s.0, b.clone())).collect();
        assert_eq!(
            items,
            vec![(0, b"a".to_vec()), (1, b"b".to_vec()), (2, b"c".to_vec())]
        );
    }

    #[test]
    fn trim_removes_acked_entries() {
        let mut buf = ReplayBuffer::new();
        for i in 0..5 {
            buf.push(seq(i), vec![i as u8]);
        }

        buf.trim(ack(2));

        assert_eq!(buf.iter().count(), 2);
        let seqs: Vec<_> = buf.iter().map(|(s, _)| s.0).collect();
        assert_eq!(seqs, vec![3, 4]);
    }

    #[test]
    fn trim_exact_boundary() {
        let mut buf = ReplayBuffer::new();
        buf.push(seq(10), b"x".to_vec());
        buf.push(seq(11), b"y".to_vec());

        // Ack exactly seq 10 — only seq 10 goes
        buf.trim(ack(10));

        assert_eq!(buf.iter().count(), 1);
        assert_eq!(buf.iter().next().unwrap().0, seq(11));
    }

    #[test]
    fn trim_noop_when_nothing_acked() {
        let mut buf = ReplayBuffer::new();
        buf.push(seq(5), b"a".to_vec());
        buf.push(seq(6), b"b".to_vec());

        // Ack is below all buffered seqs
        buf.trim(ack(4));

        assert_eq!(buf.iter().count(), 2);
    }

    #[test]
    fn trim_all() {
        let mut buf = ReplayBuffer::new();
        for i in 0..4 {
            buf.push(seq(i), vec![i as u8]);
        }

        buf.trim(ack(3));

        assert_eq!(buf.iter().count(), 0);
    }

    #[test]
    fn trim_empty_buffer_is_noop() {
        let mut buf = ReplayBuffer::new();
        buf.trim(ack(99)); // should not panic
        assert_eq!(buf.iter().count(), 0);
    }

    #[test]
    fn push_after_trim_continues_correctly() {
        let mut buf = ReplayBuffer::new();
        buf.push(seq(0), b"first".to_vec());
        buf.push(seq(1), b"second".to_vec());
        buf.trim(ack(1));
        assert_eq!(buf.iter().count(), 0);

        buf.push(seq(2), b"third".to_vec());
        assert_eq!(buf.iter().count(), 1);
        assert_eq!(buf.iter().next().unwrap().1, b"third");
    }
}
