//! Channel retry compliance tests.

use spec_tests::harness::{
    SubjectSpec, SubjectTestTransport, run_async, run_subject_client_scenario_resumable,
};

const RETRY_PROBE_BREAK_AFTER: usize = 20;

fn stable_conduit_requested() -> bool {
    std::env::var("SPEC_CONDUIT").ok().as_deref() == Some("stable")
}

// r[verify retry.channel.recovery.non-idem]
// r[verify retry.channel.disconnect.closes]
pub fn run_channel_retry_non_idem_fails_closed(spec: SubjectSpec) {
    if spec.transport != SubjectTestTransport::Tcp || stable_conduit_requested() {
        return;
    }

    run_async(async {
        run_subject_client_scenario_resumable(
            spec,
            "channel_retry_non_idem",
            RETRY_PROBE_BREAK_AFTER,
        )
        .await
    })
    .unwrap();
}

// r[verify retry.channel.volatile.rebinding]
// r[verify retry.channel.volatile.rebinding.fresh]
pub fn run_channel_retry_idem_reruns_with_fresh_channels(spec: SubjectSpec) {
    if spec.transport != SubjectTestTransport::Tcp || stable_conduit_requested() {
        return;
    }

    run_async(async {
        run_subject_client_scenario_resumable(spec, "channel_retry_idem", RETRY_PROBE_BREAK_AFTER)
            .await
    })
    .unwrap();
}
