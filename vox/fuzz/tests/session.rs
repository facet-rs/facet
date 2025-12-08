//! Bolero fuzzer for Session credit and lifecycle invariants.
//!
//! Properties tested:
//! - Credit accounting is accurate (deduct on send, add on grant)
//! - Lifecycle state machine transitions are valid
//! - Cancellation is sticky and drops frames
//! - Control channel (0) is exempt from credit checks

use bolero::check;
use rapace_fuzz::session_model::{execute_and_verify, SessionOp};

fn main() {
    check!()
        .with_type::<Vec<SessionOpInput>>()
        .for_each(|ops| {
            let ops: Vec<SessionOp> = ops.iter().map(|op| op.to_session_op()).collect();

            if let Err(e) = execute_and_verify(&ops) {
                panic!("Invariant violated: {}", e);
            }
        });
}

/// Fuzz-friendly input type for session operations.
#[derive(Debug, Clone, bolero::TypeGenerator)]
enum SessionOpInput {
    Send { channel_id: u8, payload_len: u16, eos: bool },
    Recv { channel_id: u8, eos: bool },
    GrantCredits { channel_id: u8, bytes: u16 },
    Cancel { channel_id: u8 },
}

impl SessionOpInput {
    fn to_session_op(&self) -> SessionOp {
        match self {
            SessionOpInput::Send { channel_id, payload_len, eos } => SessionOp::Send {
                channel_id: *channel_id,
                payload_len: *payload_len,
                eos: *eos,
            },
            SessionOpInput::Recv { channel_id, eos } => SessionOp::Recv {
                channel_id: *channel_id,
                eos: *eos,
            },
            SessionOpInput::GrantCredits { channel_id, bytes } => SessionOp::GrantCredits {
                channel_id: *channel_id,
                bytes: *bytes,
            },
            SessionOpInput::Cancel { channel_id } => SessionOp::Cancel {
                channel_id: *channel_id,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use rapace_fuzz::session_model::{execute_and_verify, SessionOp};

    #[test]
    fn test_basic_flow() {
        let ops = vec![
            SessionOp::Send { channel_id: 1, payload_len: 1000, eos: false },
            SessionOp::GrantCredits { channel_id: 1, bytes: 500 },
            SessionOp::Send { channel_id: 1, payload_len: 500, eos: true },
            SessionOp::Recv { channel_id: 1, eos: true },
        ];
        execute_and_verify(&ops).unwrap();
    }

    #[test]
    fn test_credit_exhaustion() {
        let mut ops = Vec::new();
        // Exhaust credits with many small sends
        for _ in 0..100 {
            ops.push(SessionOp::Send { channel_id: 1, payload_len: 1000, eos: false });
        }
        // This should work - some sends will fail due to insufficient credits
        execute_and_verify(&ops).unwrap();
    }

    #[test]
    fn test_cancel_flow() {
        let ops = vec![
            SessionOp::Send { channel_id: 1, payload_len: 1000, eos: false },
            SessionOp::Cancel { channel_id: 1 },
            SessionOp::Send { channel_id: 1, payload_len: 1000, eos: false }, // Should be dropped
            SessionOp::Recv { channel_id: 1, eos: false }, // Should be dropped
        ];
        execute_and_verify(&ops).unwrap();
    }

    #[test]
    fn test_lifecycle_transitions() {
        // Test all valid lifecycle paths

        // Path 1: Open -> HalfClosedLocal -> Closed
        let ops1 = vec![
            SessionOp::Send { channel_id: 1, payload_len: 0, eos: true },
            SessionOp::Recv { channel_id: 1, eos: true },
        ];
        execute_and_verify(&ops1).unwrap();

        // Path 2: Open -> HalfClosedRemote -> Closed
        let ops2 = vec![
            SessionOp::Recv { channel_id: 2, eos: true },
            SessionOp::Send { channel_id: 2, payload_len: 0, eos: true },
        ];
        execute_and_verify(&ops2).unwrap();
    }

    #[test]
    fn test_control_channel_exempt() {
        // Control channel (0) should work even with huge payloads
        let ops = vec![
            SessionOp::Send { channel_id: 0, payload_len: 65535, eos: false },
            SessionOp::Send { channel_id: 0, payload_len: 65535, eos: false },
            SessionOp::Send { channel_id: 0, payload_len: 65535, eos: false },
        ];
        execute_and_verify(&ops).unwrap();
    }
}
