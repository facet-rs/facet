use facet::Facet;
use phon::api::{Codec, MethodJitFallbackReport};

#[derive(Debug, Clone, Facet)]
struct RepoFile {
    name: String,
    url: String,
}

#[derive(Debug, Clone, Facet)]
struct RepoDownload {
    repo_id: String,
    local_dir: String,
    files: Vec<RepoFile>,
}

#[derive(Debug, Clone, Facet)]
#[repr(u8)]
enum BeeError {
    EngineNotLoaded,
    SessionNotFound { session_id: String },
    LoadFailed { message: String },
    TranscriptionError { message: String },
    CorrectionError { message: String },
    NotImplemented,
}

fn bee_error_payload_len(error: BeeError) -> usize {
    match error {
        BeeError::EngineNotLoaded | BeeError::NotImplemented => 0,
        BeeError::SessionNotFound { session_id } => session_id.len(),
        BeeError::LoadFailed { message }
        | BeeError::TranscriptionError { message }
        | BeeError::CorrectionError { message } => message.len(),
    }
}

#[derive(Debug, Clone, Facet)]
struct SessionConfig {
    language: String,
    chunk_duration: f32,
    vad_threshold: f32,
    rollback_tokens: u32,
    commit_token_count: u32,
}

#[derive(Debug, Clone, Facet)]
struct Confidence {
    mean_lp: f32,
    min_lp: f32,
    mean_m: f32,
    min_m: f32,
}

#[derive(Debug, Clone, Facet)]
struct AlignedWord {
    word: String,
    start: f64,
    end: f64,
    confidence: Confidence,
}

#[derive(Debug, Clone, Facet)]
struct CorrectionEdit {
    edit_id: String,
    span_start: u32,
    span_end: u32,
    original: String,
    replacement: String,
    term: String,
    alias_id: i32,
    ranker_prob: f64,
    gate_prob: f64,
}

#[derive(Debug, Clone, Facet)]
struct FeedResult {
    text: String,
    committed_utf16_len: u32,
    alignments: Vec<AlignedWord>,
    is_final: bool,
    detected_language: String,
    correction_edits: Vec<CorrectionEdit>,
    correction_session_id: String,
}

#[derive(Debug, Clone, Facet)]
struct EngineStats {
    cpu_percent: f32,
    gpu_percent: f32,
    vram_used_mb: f32,
    ram_used_mb: f32,
}

#[derive(Debug, Clone, Facet)]
struct EditResolution {
    edit_id: String,
    accepted: bool,
}

#[derive(Debug, Clone, Facet)]
#[repr(u8)]
enum ImePhase {
    Dictating,
    Finalizing,
}

#[derive(Debug, Clone, Facet)]
struct EmptyArgs {}

#[derive(Debug, Clone, Facet)]
struct ModelArgs {
    model: String,
}

#[derive(Debug, Clone, Facet)]
struct LoadEngineArgs {
    cache_dir: String,
    model: String,
}

#[derive(Debug, Clone, Facet)]
struct CreateSessionArgs {
    opts: SessionConfig,
}

#[derive(Debug, Clone, Facet)]
struct FeedArgs {
    session_id: String,
    samples: Vec<f32>,
}

#[derive(Debug, Clone, Facet)]
struct FinishSessionArgs {
    session_id: String,
}

#[derive(Debug, Clone, Facet)]
struct SetLanguageArgs {
    session_id: String,
    language: String,
}

#[derive(Debug, Clone, Facet)]
struct TranscribeSamplesArgs {
    samples: Vec<f32>,
}

#[derive(Debug, Clone, Facet)]
struct CorrectLoadArgs {
    dataset_dir: String,
    events_path: String,
    gate_threshold: f32,
    ranker_threshold: f32,
}

#[derive(Debug, Clone, Facet)]
struct CorrectTeachArgs {
    session_id: String,
    resolutions: Vec<EditResolution>,
}

#[derive(Debug, Clone, Facet)]
struct SetMarkedTextArgs {
    text: String,
    animation_budget_ms: u32,
}

#[derive(Debug, Clone, Facet)]
struct SetPhaseArgs {
    phase: ImePhase,
}

#[derive(Debug, Clone, Facet)]
struct CommitTextArgs {
    text: String,
}

#[derive(Debug, Clone, Facet)]
struct AdvanceTranscriptArgs {
    text: String,
    committed_len: u32,
    animation_budget_ms: u32,
}

#[derive(Debug, Clone, Facet)]
struct ImeKeyEventArgs {
    event_type: String,
    key_code: u32,
    characters: String,
}

#[derive(Debug, Clone, Facet)]
struct ImeContextLostArgs {
    had_marked_text: bool,
}

fn report_for<'facet, T>(method: &str, phase: &str) -> MethodJitFallbackReport
where
    T: Facet<'facet>,
{
    Codec::<T>::new()
        .expect("Bee surface shape should lower")
        .jit_fallback_report()
        .scoped(method, phase)
}

#[track_caller]
fn audit<'facet, T>(method: &str, phase: &str)
where
    T: Facet<'facet>,
{
    let report = report_for::<T>(method, phase);
    for record in &report.records {
        assert_eq!(record.method, method);
        assert_eq!(record.phase, phase);
        assert!(record.direction == "decode" || record.direction == "encode");
        assert!(!record.path.is_empty());
        assert!(!record.reason.is_empty());
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    assert!(
        report.is_empty(),
        "{method} {phase} has native JIT fallbacks: {report:#?}"
    );

    #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
    assert!(!report.is_empty());
}

#[test]
fn bee_engine_roots_are_auditable() {
    assert_eq!(bee_error_payload_len(BeeError::EngineNotLoaded), 0);
    assert_eq!(bee_error_payload_len(BeeError::NotImplemented), 0);
    assert_eq!(
        bee_error_payload_len(BeeError::SessionNotFound {
            session_id: "s".to_string()
        }),
        1
    );
    assert_eq!(
        bee_error_payload_len(BeeError::LoadFailed {
            message: "load".to_string()
        }),
        4
    );
    assert_eq!(
        bee_error_payload_len(BeeError::TranscriptionError {
            message: "asr".to_string()
        }),
        3
    );
    assert_eq!(
        bee_error_payload_len(BeeError::CorrectionError {
            message: "corr".to_string()
        }),
        4
    );

    audit::<ModelArgs>("requiredDownloads", "args");
    audit::<Vec<RepoDownload>>("requiredDownloads", "response");

    audit::<LoadEngineArgs>("loadEngine", "args");
    audit::<Result<bool, BeeError>>("loadEngine", "response");

    audit::<CreateSessionArgs>("createSession", "args");
    audit::<Result<String, BeeError>>("createSession", "response");

    audit::<FeedArgs>("feed", "args");
    audit::<Result<Option<FeedResult>, BeeError>>("feed", "response");

    audit::<FinishSessionArgs>("finishSession", "args");
    audit::<Result<FeedResult, BeeError>>("finishSession", "response");

    audit::<SetLanguageArgs>("setLanguage", "args");
    audit::<Result<bool, BeeError>>("setLanguage", "response");

    audit::<TranscribeSamplesArgs>("transcribeSamples", "args");
    audit::<Result<String, BeeError>>("transcribeSamples", "response");

    audit::<EmptyArgs>("getStats", "args");
    audit::<EngineStats>("getStats", "response");

    audit::<CorrectLoadArgs>("correctLoad", "args");
    audit::<Result<bool, BeeError>>("correctLoad", "response");

    audit::<CorrectTeachArgs>("correctTeach", "args");
    audit::<Result<bool, BeeError>>("correctTeach", "response");

    audit::<EmptyArgs>("correctSave", "args");
    audit::<Result<bool, BeeError>>("correctSave", "response");
}

#[test]
fn bee_ime_roots_are_auditable() {
    audit::<SetMarkedTextArgs>("setMarkedText", "args");
    audit::<bool>("setMarkedText", "response");

    audit::<SetPhaseArgs>("setPhase", "args");
    audit::<bool>("setPhase", "response");

    audit::<CommitTextArgs>("commitText", "args");
    audit::<bool>("commitText", "response");

    audit::<AdvanceTranscriptArgs>("advanceTranscript", "args");
    audit::<bool>("advanceTranscript", "response");

    audit::<EmptyArgs>("stopDictating", "args");
    audit::<bool>("stopDictating", "response");

    audit::<EmptyArgs>("imeHello", "args");
    audit::<String>("imeHello", "response");

    audit::<EmptyArgs>("imeAttach", "args");
    audit::<bool>("imeAttach", "response");

    audit::<EmptyArgs>("imeActivationRevoked", "args");
    audit::<bool>("imeActivationRevoked", "response");

    audit::<ImeContextLostArgs>("imeContextLost", "args");
    audit::<bool>("imeContextLost", "response");

    audit::<ImeKeyEventArgs>("imeKeyEvent", "args");
    audit::<bool>("imeKeyEvent", "response");
}
