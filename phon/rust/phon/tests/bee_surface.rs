use facet::Facet;
use phon::api::{Codec, MethodJitShapeReport};

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

#[track_caller]
fn audit<'facet, T>(method: &str, phase: &str) -> MethodJitShapeReport
where
    T: Facet<'facet>,
{
    let codec = Codec::<T>::new().expect("Bee surface shape should lower");
    let shape = codec.jit_shape_report().scoped(method, phase);
    assert_eq!(shape.records.len(), 1);
    let shape_record = &shape.records[0];
    assert_eq!(shape_record.method, method);
    assert_eq!(shape_record.phase, phase);

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        assert!(shape_record.decode_native.is_some());
        assert!(shape_record.encode_native.is_some());
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        assert!(shape_record.decode_native.is_none());
        assert!(shape_record.encode_native.is_none());
    }

    let report = codec.jit_fallback_report().scoped(method, phase);
    for record in &report.records {
        assert_eq!(record.method, method);
        assert_eq!(record.phase, phase);
        assert!(record.direction == "decode" || record.direction == "encode");
        assert!(!record.path.is_empty());
        assert!(!record.reason.is_empty());
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert!(
        report.is_empty(),
        "{method} {phase} has native JIT fallbacks: {report:#?}"
    );

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    assert!(!report.is_empty());

    shape
}

#[track_caller]
fn assert_surface_shape_summary(report: &MethodJitShapeReport) {
    assert!(!report.is_empty());
    let summary = report.summary();
    assert_eq!(summary.root_count, report.records.len());
    assert!(summary.lowered.total.op_count > 0);
    assert!(summary.lowered.total.scalar_op_count > 0);

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        assert_eq!(summary.decode_native_count, summary.root_count);
        assert_eq!(summary.encode_native_count, summary.root_count);
        assert!(summary.decode_native.stencil_count >= summary.lowered.total.op_count);
        assert!(summary.encode_native.stencil_count >= summary.lowered.total.op_count);
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        assert_eq!(summary.decode_native_count, 0);
        assert_eq!(summary.encode_native_count, 0);
    }
}

macro_rules! audit_roots {
    ($($ty:ty, $method:literal, $phase:literal;)+) => {{
        let mut surface = MethodJitShapeReport::default();
        $(surface.extend(audit::<$ty>($method, $phase));)+
        surface
    }};
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

    let surface = audit_roots! {
        ModelArgs, "requiredDownloads", "args";
        Vec<RepoDownload>, "requiredDownloads", "response";
        LoadEngineArgs, "loadEngine", "args";
        Result<bool, BeeError>, "loadEngine", "response";
        CreateSessionArgs, "createSession", "args";
        Result<String, BeeError>, "createSession", "response";
        FeedArgs, "feed", "args";
        Result<Option<FeedResult>, BeeError>, "feed", "response";
        FinishSessionArgs, "finishSession", "args";
        Result<FeedResult, BeeError>, "finishSession", "response";
        SetLanguageArgs, "setLanguage", "args";
        Result<bool, BeeError>, "setLanguage", "response";
        TranscribeSamplesArgs, "transcribeSamples", "args";
        Result<String, BeeError>, "transcribeSamples", "response";
        EmptyArgs, "getStats", "args";
        EngineStats, "getStats", "response";
        CorrectLoadArgs, "correctLoad", "args";
        Result<bool, BeeError>, "correctLoad", "response";
        CorrectTeachArgs, "correctTeach", "args";
        Result<bool, BeeError>, "correctTeach", "response";
        EmptyArgs, "correctSave", "args";
        Result<bool, BeeError>, "correctSave", "response";
    };
    assert_surface_shape_summary(&surface);
}

#[test]
fn bee_ime_roots_are_auditable() {
    let surface = audit_roots! {
        SetMarkedTextArgs, "setMarkedText", "args";
        bool, "setMarkedText", "response";
        SetPhaseArgs, "setPhase", "args";
        bool, "setPhase", "response";
        CommitTextArgs, "commitText", "args";
        bool, "commitText", "response";
        AdvanceTranscriptArgs, "advanceTranscript", "args";
        bool, "advanceTranscript", "response";
        EmptyArgs, "stopDictating", "args";
        bool, "stopDictating", "response";
        EmptyArgs, "imeHello", "args";
        String, "imeHello", "response";
        EmptyArgs, "imeAttach", "args";
        bool, "imeAttach", "response";
        EmptyArgs, "imeActivationRevoked", "args";
        bool, "imeActivationRevoked", "response";
        ImeContextLostArgs, "imeContextLost", "args";
        bool, "imeContextLost", "response";
        ImeKeyEventArgs, "imeKeyEvent", "args";
        bool, "imeKeyEvent", "response";
    };
    assert_surface_shape_summary(&surface);
}
