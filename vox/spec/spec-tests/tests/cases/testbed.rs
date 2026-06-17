use std::collections::{BTreeMap, BTreeSet};

use facet_value::{VObject, VString, Value};
use spec_proto::{
    BridgeResponsiveImageInfo, Color, DibsAppliedMigration, DibsColumnInfo, DibsCreateRequest,
    DibsDeleteRequest, DibsFilter, DibsFilterOp, DibsForeignKeyInfo, DibsGetRequest,
    DibsIndexColumnInfo, DibsIndexInfo, DibsListRequest, DibsListResponse, DibsLogLevel,
    DibsMigrateRequest, DibsMigrateResult, DibsMigrationInfo, DibsMigrationLog,
    DibsMigrationStatusRequest, DibsRanMigration, DibsRow, DibsRowField, DibsSchemaInfo, DibsSort,
    DibsSortDir, DibsTableInfo, DibsUpdateRequest, DibsValue, DodecaAssetProcessingFixture,
    DodecaBuildMetadata, DodecaCodeExecutionConfig, DodecaCodeExecutionMetadata,
    DodecaCodeExecutionResult, DodecaCodeSample, DodecaCssResult, DodecaDecodedImage,
    DodecaDependencySource, DodecaDependencySpec, DodecaExecuteSamplesInput,
    DodecaExecuteSamplesOutput, DodecaExecutionResult, DodecaExecutionStatus,
    DodecaHtmlProcessInput, DodecaHtmlProcessResult, DodecaImageProcessorFixture,
    DodecaImageResult, DodecaInjection, DodecaMinifyOptions, DodecaMountLocalization,
    DodecaResizeInput, DodecaResolvedDependency, DodecaResponsiveImageInfo, DodecaRustConfig,
    DodecaSassResult, DodecaSearchFile, DodecaSearchIndexResult, DodecaSearchIndexerFixture,
    DodecaSearchPage, DodecaSvgoResult, DodecaTemplateCall, DodecaThumbhashInput,
    DodecaWikiLinkRef, EcosystemBridgePayload, HelixAdmissionSegmentId,
    HelixArDecodeEarlyExitReason, HelixAttentionSummaryBatch, HelixAttentionSupportSummary,
    HelixAudioAttendanceRow, HelixAudioClip, HelixAudioEncoderSupportRecord,
    HelixAudioRepresentationSpan, HelixAudioRepresentationVersion, HelixAudioSelfAttentionRow,
    HelixAudioTokenAdmissionProvenance, HelixAudioTokenId, HelixAudioTokenMergeProvenance,
    HelixAudioTokenProvenance, HelixAudioTokenRange, HelixChromeTraceEvent, HelixConvStemChunkId,
    HelixDecodeFact, HelixDecoderEvidenceFactCounts, HelixDecoderEvidenceKind,
    HelixDecoderEvidenceRecord, HelixDecoderEvidenceReport, HelixDecoderEvidenceVariantCounts,
    HelixEncoderFactsSnapshot, HelixEncoderFrontierLayer, HelixEncoderFrontierPoint,
    HelixEncoderFrontierSeries, HelixEncoderProvenanceReport, HelixEncoderProvenanceViolation,
    HelixEncoderProvenanceViolationKind, HelixLogicalPosition, HelixMelClip, HelixMelFrameRange,
    HelixNativeEncoderWindowId, HelixPieceEvalReference, HelixPieceEvalSnapshot, HelixPromptLayout,
    HelixPromptPrefillFact, HelixPulseAttentionHeatmap, HelixPulseAvailable, HelixPulseBundle,
    HelixPulseBundleFields, HelixPulseEvidenceSnapshot, HelixPulseRollup,
    HelixQueryRowAttentionRecord, HelixRefreshAttendanceRow, HelixRunInfo, HelixSchedulerPulseId,
    HelixStreamMeta, HelixStreamMetrics, HelixStreamingTraceEvent, HelixTextAttendanceRow,
    HelixTextAttentionSupportRecord, HelixTextTokenId, HelixTextTokenSnapshot,
    HelixTracePositionSpan, HelixTraceServiceSurface, HelixTranscriptToken, HelixVerifyDraftRow,
    HelixVerifyDraftStatus, HelixVerifyEvidenceDigest, HelixVerifyOutcome,
    HelixVerifyPredictionFact, HelixVerifySeedFact, HelixVerifySeedRow, HelixVerifySkippedReason,
    HotmealApplyPatchesResult, HotmealDomAttr, HotmealDomNode, HotmealLiveReloadEvent,
    HotmealPatchStep, LookupError, MathError, Message, Point, Rectangle, Shape, StaxFlameNode,
    StaxFlamegraphUpdate, StaxLinuxBrokerControlFixture, StaxLinuxDaemonStatus,
    StaxLinuxPerfSessionConfig, StaxLinuxPerfSessionError, StaxLinuxWakingFieldOffsets,
    StaxLiveFilter, StaxMacKdBuf, StaxMacKdBufBatch, StaxMacRecordSummary, StaxMacSessionConfig,
    StaxOffCpuBreakdown, StaxSymbolRef, StaxTimeRange, StaxViewParams, StyxEntry,
    StyxLspCapability, StyxLspCodeAction, StyxLspCodeActionKind, StyxLspCodeActionParams,
    StyxLspCompletionItem, StyxLspCompletionKind, StyxLspCompletionParams, StyxLspCursor,
    StyxLspDefinitionParams, StyxLspDiagnostic, StyxLspDiagnosticParams, StyxLspDiagnosticSeverity,
    StyxLspDocumentEdit, StyxLspGetDocumentParams, StyxLspGetSchemaParams, StyxLspGetSourceParams,
    StyxLspGetSubtreeParams, StyxLspHoverParams, StyxLspHoverResult, StyxLspInitializeParams,
    StyxLspInitializeResult, StyxLspInlayHint, StyxLspInlayHintKind, StyxLspInlayHintParams,
    StyxLspLocation, StyxLspOffsetToPositionParams, StyxLspPosition, StyxLspPositionToOffsetParams,
    StyxLspRange, StyxLspSchemaInfo, StyxLspTextEdit, StyxLspWorkspaceEdit, StyxObject,
    StyxPayload, StyxScalar, StyxScalarKind, StyxSequence, StyxSpan, StyxTag, StyxValue, Tag,
    TraceyApiConfig, TraceyApiSpecInfo, TraceyCodeRef, TraceyCoverageChange, TraceyDataUpdate,
    TraceyDeltaSummary, TraceyHealthResponse, TraceyHoverInfo, TraceyImplStatus,
    TraceyLspCodeAction, TraceyLspCodeLens, TraceyLspCompletionItem, TraceyLspDiagnostic,
    TraceyLspDocumentRequest, TraceyLspFileDiagnostics, TraceyLspInlayHint,
    TraceyLspInlayHintsRequest, TraceyLspLocation, TraceyLspPositionRequest,
    TraceyLspReferencesRequest, TraceyLspRenameRequest, TraceyLspSemanticToken, TraceyLspSymbol,
    TraceyLspTextEdit, TraceyPrepareRenameResult, TraceyReloadResponse, TraceyRuleCoverage,
    TraceyRuleId, TraceyRuleInfo, TraceyRuleRef, TraceySectionRules, TraceyStaleEntry,
    TraceyStaleRequest, TraceyStaleResponse, TraceyStatusResponse, TraceyUncoveredRequest,
    TraceyUncoveredResponse, TraceyUnmappedEntry, TraceyUnmappedRequest, TraceyUnmappedResponse,
    TraceyUnmappedUnit, TraceyUntestedRequest, TraceyUntestedResponse, TraceyValidateRequest,
    TraceyValidationError, TraceyValidationErrorCode, TraceyValidationResult, Tree,
};
use spec_proto::{
    DodecaBindMode, DodecaBuildProgress, DodecaCommandResult, DodecaConfirmResult, DodecaEventKind,
    DodecaFontResult, DodecaHtmlDiffInput, DodecaHtmlDiffOutcome, DodecaJsRewriteInput,
    DodecaJxlEncodeInput, DodecaJxlResult, DodecaLinkCheckInput, DodecaLinkCheckOutput,
    DodecaLinkCheckResult, DodecaLinkDiagnostics, DodecaLinkStatus, DodecaLogEvent, DodecaLogLevel,
    DodecaMinifyResult, DodecaReadyAck, DodecaReadyMsg, DodecaRecordConfig, DodecaRunBuildResult,
    DodecaSelectResult, DodecaServerCommand, DodecaServerStatus, DodecaSmallCellServicesFixture,
    DodecaStartDevServerResult, DodecaSubsetFontInput, DodecaTaskProgress, DodecaTaskStatus,
    DodecaTermResult, DodecaWebpEncodeInput, DodecaWebpResult,
};
use spec_proto::{
    DodecaDataFormat, DodecaFrontmatter, DodecaLoadDataResult, DodecaMarkdownHeading,
    DodecaParseResult, DodecaReqDefinition, DodecaSourceKind, DodecaSourceMap,
    DodecaSourceMapEntry,
};
use spec_proto::{
    DodecaDeadLinkTarget, DodecaDevtoolsEvent, DodecaEditEntry, DodecaEditList, DodecaEditLoad,
    DodecaEditPreview, DodecaEditRead, DodecaEditSave, DodecaEditSaveReq, DodecaEditUpload,
    DodecaEditUploadReq, DodecaErrorInfo, DodecaEvalResult, DodecaOpenSourceResult,
    DodecaScopeEntry, DodecaScopeValue, DodecaSidLine, DodecaSourceLine, DodecaSourceSnippet,
};
use spec_proto::{
    TraceyApiCodeUnit, TraceyApiFileData, TraceyApiFileEntry, TraceyApiReverseData, TraceyApiRule,
    TraceyApiSpecData, TraceyApiSpecForward, TraceyApiStaleRef, TraceyConfigPatternRequest,
    TraceyFileRequest, TraceyOutlineCoverage, TraceyOutlineEntry, TraceySearchResult,
    TraceySpecSection, TraceyUpdateError, TraceyUpdateFileRangeRequest,
};
use spec_tests::harness::{
    SubjectSpec, accept_subject_spec, run_async, run_subject_client_scenario,
};
use vox::VoxError;

// r[verify call.initiate]
// r[verify call.complete]
// r[verify call.lifecycle.single-response]
// r[verify call.lifecycle.ordering]
// r[verify transport.message.binary]
pub fn run_rpc_echo_roundtrip(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let resp = client
            .echo("hello".to_string())
            .await
            .map_err(|e| format!("echo: {e:?}"))?;
        if resp != "hello" {
            return Err(format!("expected \"hello\", got {:?}", resp));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.user]
pub fn run_rpc_user_error_roundtrip(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let result = client.divide(10, 0).await;
        match result {
            Err(VoxError::User(error)) if *error == MathError::DivisionByZero => {}
            Ok(resp) => {
                return Err(format!(
                    "expected Err(User(DivisionByZero)), got Ok({})",
                    resp
                ));
            }
            Err(other) => {
                return Err(format!(
                    "expected Err(User(DivisionByZero)), got Err({other:?})"
                ));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.pipelining.allowed]
// r[verify call.pipelining.independence]
// r[verify core.call]
// r[verify core.call.request-id]
pub fn run_rpc_pipelining_multiple_requests(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let (r1, r2, r3) = tokio::join!(
            client.echo("first".to_string()),
            client.echo("second".to_string()),
            client.echo("third".to_string()),
        );
        if r1.map_err(|e| format!("{e:?}"))? != "first" {
            return Err("pipelining: first response wrong".to_string());
        }
        if r2.map_err(|e| format!("{e:?}"))? != "second" {
            return Err("pipelining: second response wrong".to_string());
        }
        if r3.map_err(|e| format!("{e:?}"))? != "third" {
            return Err("pipelining: third response wrong".to_string());
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.initiate]
// r[verify call.complete]
pub fn run_rpc_reverse_roundtrip(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let resp = client
            .reverse("hello".to_string())
            .await
            .map_err(|e| format!("reverse: {e:?}"))?;
        if resp != "olleh" {
            return Err(format!("expected \"olleh\", got {:?}", resp));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.user]
pub fn run_rpc_lookup_user_error(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let result = client.lookup(999).await;
        match result {
            Err(VoxError::User(err)) => {
                // Any lookup error is acceptable — key thing is it's a user error
                let _ = err;
            }
            Ok(resp) => {
                return Err(format!("expected Err(User(...)), got Ok({resp:?})"));
            }
            Err(other) => {
                return Err(format!("expected Err(User(...)), got Err({other:?})"));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.initiate]
// r[verify encoding.struct]
pub fn run_rpc_complex_struct_echo(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let point = Point { x: 3, y: 7 };
        let resp = client
            .echo_point(point.clone())
            .await
            .map_err(|e| format!("echo_point: {e:?}"))?;
        if resp != point {
            return Err(format!("expected {point:?}, got {resp:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.option]
pub fn run_rpc_optional_field(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        // Test with Some email
        let p1 = client
            .create_person(
                "Alice".to_string(),
                30,
                Some("alice@example.com".to_string()),
            )
            .await
            .map_err(|e| format!("create_person with email: {e:?}"))?;
        if p1.name != "Alice" || p1.age != 30 || p1.email.as_deref() != Some("alice@example.com") {
            return Err(format!("create_person with email: got {p1:?}"));
        }
        // Test with None email
        let p2 = client
            .create_person("Bob".to_string(), 25, None)
            .await
            .map_err(|e| format!("create_person without email: {e:?}"))?;
        if p2.name != "Bob" || p2.age != 25 || p2.email.is_some() {
            return Err(format!("create_person without email: got {p2:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct.nested]
pub fn run_rpc_nested_struct(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let rect = Rectangle {
            top_left: Point { x: 0, y: 10 },
            bottom_right: Point { x: 5, y: 0 },
            label: Some("test".to_string()),
        };
        let area = client
            .rectangle_area(rect)
            .await
            .map_err(|e| format!("rectangle_area: {e:?}"))?;
        // area = |x2-x1| * |y2-y1| = 5 * 10 = 50
        if (area - 50.0_f64).abs() > 1e-9 {
            return Err(format!("expected area 50.0, got {area}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.option.return]
pub fn run_rpc_option_return(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        // Known color
        let color = client
            .parse_color("red".to_string())
            .await
            .map_err(|e| format!("parse_color red: {e:?}"))?;
        if color != Some(Color::Red) {
            return Err(format!("expected Some(Red), got {color:?}"));
        }
        // Unknown color → None
        let none = client
            .parse_color("purple".to_string())
            .await
            .map_err(|e| format!("parse_color unknown: {e:?}"))?;
        if none.is_some() {
            return Err(format!("expected None for unknown color, got {none:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.struct-variants]
pub fn run_rpc_enum_struct_variants(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let area_circle = client
            .shape_area(Shape::Circle { radius: 1.0 })
            .await
            .map_err(|e| format!("shape_area circle: {e:?}"))?;
        // π * r² ≈ 3.14159
        if (area_circle - std::f64::consts::PI).abs() > 1e-6 {
            return Err(format!("circle area: expected ~pi, got {area_circle}"));
        }
        let area_rect = client
            .shape_area(Shape::Rectangle {
                width: 3.0,
                height: 4.0,
            })
            .await
            .map_err(|e| format!("shape_area rect: {e:?}"))?;
        if (area_rect - 12.0_f64).abs() > 1e-9 {
            return Err(format!("rect area: expected 12.0, got {area_rect}"));
        }
        let area_point = client
            .shape_area(Shape::Point)
            .await
            .map_err(|e| format!("shape_area point: {e:?}"))?;
        if (area_point - 0.0_f64).abs() > 1e-9 {
            return Err(format!("point area: expected 0.0, got {area_point}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct.nested]
// r[verify encoding.vec]
// r[verify encoding.enum]
pub fn run_rpc_vec_of_structs(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let canvas = client
            .create_canvas(
                "test".to_string(),
                vec![
                    Shape::Point,
                    Shape::Circle { radius: 2.0 },
                    Shape::Rectangle {
                        width: 1.0,
                        height: 3.0,
                    },
                ],
                Color::Blue,
            )
            .await
            .map_err(|e| format!("create_canvas: {e:?}"))?;
        if canvas.name != "test" {
            return Err(format!(
                "canvas name: expected 'test', got {:?}",
                canvas.name
            ));
        }
        if canvas.background != Color::Blue {
            return Err(format!(
                "canvas background: expected Blue, got {:?}",
                canvas.background
            ));
        }
        if canvas.shapes.len() != 3 {
            return Err(format!(
                "canvas shapes: expected 3, got {}",
                canvas.shapes.len()
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.newtype-variants]
pub fn run_rpc_enum_newtype_variants(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        // Text variant — subject prefixes with "processed: "
        let text_out = client
            .process_message(Message::Text("hello".to_string()))
            .await
            .map_err(|e| format!("process_message text: {e:?}"))?;
        match text_out {
            Message::Text(_) => {}
            other => {
                return Err(format!(
                    "process_message text: expected Text, got {other:?}"
                ));
            }
        }
        // Number variant — subject doubles the number
        let num_out = client
            .process_message(Message::Number(21))
            .await
            .map_err(|e| format!("process_message number: {e:?}"))?;
        match num_out {
            Message::Number(42) => {}
            other => {
                return Err(format!(
                    "process_message number: expected Number(42), got {other:?}"
                ));
            }
        }
        // Data variant — subject reverses the bytes
        let data_out = client
            .process_message(Message::Data(vec![1, 2, 3, 4]))
            .await
            .map_err(|e| format!("process_message data: {e:?}"))?;
        match data_out {
            Message::Data(ref bytes) if *bytes == vec![4, 3, 2, 1] => {}
            other => {
                return Err(format!(
                    "process_message data: expected Data([4,3,2,1]), got {other:?}"
                ));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.vec]
pub fn run_rpc_vec_return(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let points = client
            .get_points(3)
            .await
            .map_err(|e| format!("get_points: {e:?}"))?;
        if points.len() != 3 {
            return Err(format!("expected 3 points, got {}", points.len()));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.tuple]
pub fn run_rpc_tuple_type(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let (s, n) = client
            .swap_pair((42, "hello".to_string()))
            .await
            .map_err(|e| format!("swap_pair: {e:?}"))?;
        if s != "hello" || n != 42 {
            return Err(format!(
                "swap_pair: expected (\"hello\", 42), got ({s:?}, {n})"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// ============================================================================
// Subject→Harness direction: TypeScript calls Rust's service
// ============================================================================

// r[verify call.initiate]
// r[verify call.complete]
pub fn run_subject_calls_echo(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo");
}

// r[verify encoding.enum.struct-variants]
pub fn run_subject_calls_shape_area(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "shape_area");
}

// r[verify encoding.struct.nested]
// r[verify encoding.vec]
pub fn run_subject_calls_create_canvas(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "create_canvas");
}

// r[verify encoding.enum.newtype-variants]
pub fn run_subject_calls_process_message(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "process_message");
}

// ============================================================================
// Additional harness→subject: error variants
// ============================================================================

// r[verify call.error.user]
pub fn run_rpc_divide_overflow(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        // i64::MIN / -1 overflows
        let result = client.divide(i64::MIN, -1).await;
        match result {
            Err(VoxError::User(error)) if *error == MathError::Overflow => {}
            Ok(v) => return Err(format!("divide_overflow: expected Overflow, got Ok({v})")),
            Err(other) => return Err(format!("divide_overflow: expected Overflow, got {other:?}")),
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.user]
pub fn run_rpc_lookup_found(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        // id=1 → Alice with email
        let alice = client
            .lookup(1)
            .await
            .map_err(|e| format!("lookup 1: {e:?}"))?;
        if alice.name != "Alice" || alice.email.as_deref() != Some("alice@example.com") {
            return Err(format!("lookup 1: unexpected {alice:?}"));
        }
        // id=2 → Bob without email
        let bob = client
            .lookup(2)
            .await
            .map_err(|e| format!("lookup 2: {e:?}"))?;
        if bob.name != "Bob" || bob.email.is_some() {
            return Err(format!("lookup 2: unexpected {bob:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.user]
pub fn run_rpc_lookup_access_denied(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let result = client.lookup(100).await;
        match result {
            Err(VoxError::User(error)) if *error == LookupError::AccessDenied => {}
            Ok(v) => return Err(format!("expected AccessDenied, got Ok({v:?})")),
            Err(other) => return Err(format!("expected AccessDenied, got {other:?}")),
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// ============================================================================
// Additional harness→subject: new primitive/type methods
// ============================================================================

// r[verify encoding.bytes]
pub fn run_rpc_echo_bytes(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let data = vec![0u8, 1, 127, 128, 255];
        let result = client
            .echo_bytes(data.clone())
            .await
            .map_err(|e| format!("echo_bytes: {e:?}"))?;
        if result != data {
            return Err(format!("echo_bytes: expected {data:?}, got {result:?}"));
        }
        // Empty bytes
        let empty = client
            .echo_bytes(vec![])
            .await
            .map_err(|e| format!("echo_bytes empty: {e:?}"))?;
        if !empty.is_empty() {
            return Err(format!("echo_bytes empty: expected [], got {empty:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.bool]
pub fn run_rpc_echo_bool(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        for b in [true, false] {
            let result = client
                .echo_bool(b)
                .await
                .map_err(|e| format!("echo_bool({b}): {e:?}"))?;
            if result != b {
                return Err(format!("echo_bool({b}): expected {b}, got {result}"));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.u64]
pub fn run_rpc_echo_u64(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        for n in [0u64, 1, u64::MAX, 1_000_000_000_000] {
            let result = client
                .echo_u64(n)
                .await
                .map_err(|e| format!("echo_u64({n}): {e:?}"))?;
            if result != n {
                return Err(format!("echo_u64({n}): expected {n}, got {result}"));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.option]
pub fn run_rpc_echo_option_string(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let some = client
            .echo_option_string(Some("hello".to_string()))
            .await
            .map_err(|e| format!("echo_option_string Some: {e:?}"))?;
        if some.as_deref() != Some("hello") {
            return Err(format!("echo_option_string Some: got {some:?}"));
        }
        let none = client
            .echo_option_string(None)
            .await
            .map_err(|e| format!("echo_option_string None: {e:?}"))?;
        if none.is_some() {
            return Err(format!("echo_option_string None: got {none:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct.multi-arg]
pub fn run_rpc_describe_point(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let tp = client
            .describe_point("test".to_string(), 5, -3, true)
            .await
            .map_err(|e| format!("describe_point: {e:?}"))?;
        if tp.label != "test" || tp.x != 5 || tp.y != -3 || !tp.active {
            return Err(format!("describe_point: unexpected {tp:?}"));
        }
        let tp2 = client
            .describe_point("far".to_string(), -100, 200, false)
            .await
            .map_err(|e| format!("describe_point 2: {e:?}"))?;
        if tp2.label != "far" || tp2.x != -100 || tp2.y != 200 || tp2.active {
            return Err(format!("describe_point 2: unexpected {tp2:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.unit-variants]
pub fn run_rpc_all_colors(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let colors = client
            .all_colors()
            .await
            .map_err(|e| format!("all_colors: {e:?}"))?;
        if colors != vec![Color::Red, Color::Green, Color::Blue] {
            return Err(format!(
                "all_colors: expected [Red,Green,Blue], got {colors:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.struct-variants]
pub fn run_rpc_echo_shape(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        for shape in [
            Shape::Point,
            Shape::Circle { radius: 1.5 },
            Shape::Rectangle {
                width: 3.0,
                height: 4.0,
            },
        ] {
            let result = client
                .echo_shape(shape.clone())
                .await
                .map_err(|e| format!("echo_shape: {e:?}"))?;
            if result != shape {
                return Err(format!("echo_shape: expected {shape:?}, got {result:?}"));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

/// A canonical recursive tree, shared by the harness-side echo_tree check and the
/// subject scenario drivers so the round-trip assertion is identical everywhere.
fn sample_tree() -> Tree {
    Tree {
        value: 1,
        children: vec![
            Tree {
                value: 2,
                children: vec![],
            },
            Tree {
                value: 3,
                children: vec![Tree {
                    value: 4,
                    children: vec![],
                }],
            },
        ],
    }
}

fn sample_ecosystem_bridge_payload() -> EcosystemBridgePayload {
    EcosystemBridgePayload {
        html: "<main><img src=\"/hero.png\"></main>".to_string(),
        path_map: BTreeMap::from([("/old.css".to_string(), "/assets/new.css".to_string())]),
        known_routes: BTreeSet::from(["/".to_string(), "/guide/".to_string()]),
        image_variants: BTreeMap::from([(
            "/hero.png".to_string(),
            BridgeResponsiveImageInfo {
                jxl_srcset: vec![("/hero-640.jxl".to_string(), 640)],
                webp_srcset: vec![("/hero-640.webp".to_string(), 640)],
            },
        )]),
        blobs: vec![vec![0, 1, 2, 3, 255], vec![]],
    }
}

fn sample_dynamic_template_object() -> Value {
    let mut object = VObject::new();
    object.insert(VString::new("sidebar"), Value::from(true));
    object.insert(VString::new("title"), Value::from("Phon migration"));
    object.insert(VString::new("count"), Value::from(42i64));
    object.into()
}

fn sample_dodeca_template_call() -> DodecaTemplateCall {
    DodecaTemplateCall {
        context_id: "ctx-docs".to_string(),
        name: "render-card".to_string(),
        args: vec![sample_dynamic_template_object(), Value::from("docs")],
        kwargs: vec![("path".to_string(), Value::from("/guide/"))],
    }
}

fn sample_dodeca_data_content() -> String {
    "{\"title\":\"Phon\",\"sidebar\":true,\"count\":42}".to_string()
}

fn sample_dodeca_data_format() -> DodecaDataFormat {
    DodecaDataFormat::Json
}

fn sample_dodeca_dynamic_data_value() -> Value {
    let mut object = VObject::new();
    object.insert(VString::new("title"), Value::from("Phon"));
    object.insert(VString::new("sidebar"), Value::from(true));
    object.insert(VString::new("count"), Value::from(42i64));
    object.into()
}

fn sample_dodeca_load_data_result() -> DodecaLoadDataResult {
    DodecaLoadDataResult::Success {
        value: sample_dodeca_dynamic_data_value(),
    }
}

fn sample_dodeca_markdown_source_path() -> String {
    "content/guide.md".to_string()
}

fn sample_dodeca_markdown_content() -> String {
    "+++\ntitle = \"Phon migration\"\n+++\n\n# Intro\n\nr[vox.dodeca.markdown]\n".to_string()
}

fn sample_dodeca_frontmatter_extra() -> Value {
    let mut object = VObject::new();
    object.insert(VString::new("sidebar"), Value::from(true));
    object.insert(VString::new("icon"), Value::from("book"));
    object.insert(VString::new("custom_value"), Value::from(42i64));
    object.into()
}

fn sample_dodeca_parse_result() -> DodecaParseResult {
    DodecaParseResult::Success {
        frontmatter: DodecaFrontmatter {
            title: "Phon migration".to_string(),
            weight: 10,
            description: Some("Generated fixture for Dodeca markdown".to_string()),
            template: Some("page.html".to_string()),
            extra: sample_dodeca_frontmatter_extra(),
        },
        html: "<h1 data-sid=\"h1\">Intro</h1><p data-sid=\"p1\">Generated fixture</p>".to_string(),
        headings: vec![DodecaMarkdownHeading {
            title: "Intro".to_string(),
            id: "intro".to_string(),
            level: 1,
        }],
        reqs: vec![DodecaReqDefinition {
            id: "vox.dodeca.markdown".to_string(),
            anchor_id: "r-vox-dodeca-markdown".to_string(),
        }],
        head_injections: vec![
            "<link rel=\"stylesheet\" href=\"/assets/arborium.css\">".to_string(),
        ],
        source_map: Box::new(DodecaSourceMap {
            source_path: Some(sample_dodeca_markdown_source_path()),
            entries: vec![
                DodecaSourceMapEntry {
                    id: "h1".to_string(),
                    kind: DodecaSourceKind::Heading,
                    line_start: 5,
                    line_end: 5,
                    byte_start: 38,
                    byte_end: 45,
                },
                DodecaSourceMapEntry {
                    id: "p1".to_string(),
                    kind: DodecaSourceKind::Paragraph,
                    line_start: 7,
                    line_end: 7,
                    byte_start: 47,
                    byte_end: 71,
                },
            ],
        }),
    }
}

fn byte_ramp(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| seed.wrapping_add(i as u8)).collect()
}

fn sample_dodeca_decoded_image(seed: u8, width: u32, height: u32) -> DodecaDecodedImage {
    DodecaDecodedImage {
        pixels: byte_ramp(width as usize * height as usize * 4, seed),
        width,
        height,
        channels: 4,
    }
}

fn sample_dodeca_image_processor_fixture() -> DodecaImageProcessorFixture {
    let decoded = sample_dodeca_decoded_image(0x20, 96, 64);
    let resized = sample_dodeca_decoded_image(0x80, 48, 32);
    DodecaImageProcessorFixture {
        png_data: byte_ramp(16_384, 0),
        decoded_result: DodecaImageResult::Success {
            image: decoded.clone(),
        },
        resize_input: DodecaResizeInput {
            pixels: decoded.pixels.clone(),
            width: decoded.width,
            height: decoded.height,
            channels: decoded.channels,
            target_width: resized.width,
        },
        resize_result: DodecaImageResult::Success { image: resized },
        thumbhash_input: DodecaThumbhashInput {
            pixels: decoded.pixels,
            width: decoded.width,
            height: decoded.height,
        },
        thumbhash_result: DodecaImageResult::ThumbhashSuccess {
            data_url: "data:image/thumbhash;base64,BwgJCgsMDQ4PEA==".to_string(),
        },
        error_result: DodecaImageResult::Error {
            message: "unsupported color profile in source image".to_string(),
        },
    }
}

fn sample_dodeca_search_indexer_fixture() -> DodecaSearchIndexerFixture {
    let pages = (0..32)
        .map(|i| DodecaSearchPage {
            url: format!("/guide/topic-{i}/"),
            source: format!("content/guide/topic-{i}.md"),
            html: format!("<article><h1>Topic {i}</h1><p>Search body {i}</p></article>"),
        })
        .collect();
    let files = (0..8)
        .map(|i| DodecaSearchFile {
            path: format!("public/search/chunk-{i}.json"),
            contents: byte_ramp(1_024, (i * 17) as u8),
        })
        .collect();
    DodecaSearchIndexerFixture {
        pages,
        result: DodecaSearchIndexResult::Success { files },
        error_result: DodecaSearchIndexResult::Error {
            message: "search index could not write public/search/index.json".to_string(),
        },
    }
}

fn sample_dodeca_asset_processing_fixture() -> DodecaAssetProcessingFixture {
    DodecaAssetProcessingFixture {
        css_source: "body { background: url('/old/bg.png'); color: red; }".to_string(),
        css_path_map: BTreeMap::from([
            ("/old/bg.png".to_string(), "/assets/bg.abcd.png".to_string()),
            (
                "/old/font.woff2".to_string(),
                "/assets/font.woff2".to_string(),
            ),
        ]),
        css_result: DodecaCssResult::Success {
            css: "body{background:url('/assets/bg.abcd.png');color:red}".to_string(),
        },
        sass_entrypoint: "styles/app.scss".to_string(),
        sass_files: BTreeMap::from([
            (
                "styles/app.scss".to_string(),
                "$brand: #c0ffee; @import 'partials/buttons'; body { color: $brand; }".to_string(),
            ),
            (
                "styles/partials/_buttons.scss".to_string(),
                ".button { padding: 4px; }".to_string(),
            ),
        ]),
        sass_load_paths: vec!["styles".to_string(), "vendor".to_string()],
        sass_result: DodecaSassResult::Success {
            css: "body{color:#c0ffee}.button{padding:4px}".to_string(),
        },
        svg_source:
            "<svg viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"10\" fill=\"red\"/></svg>"
                .to_string(),
        svgo_result: DodecaSvgoResult::Success {
            svg: "<svg viewBox=\"0 0 10 10\"><path fill=\"red\" d=\"M0 0h10v10H0z\"/></svg>"
                .to_string(),
        },
    }
}

fn sample_dodeca_task_progress(
    name: &str,
    total: u32,
    completed: u32,
    status: DodecaTaskStatus,
) -> DodecaTaskProgress {
    DodecaTaskProgress {
        name: name.to_string(),
        total,
        completed,
        status,
        message: (status == DodecaTaskStatus::Error).then(|| format!("{name} failed")),
    }
}

fn sample_dodeca_small_cell_services_fixture() -> DodecaSmallCellServicesFixture {
    DodecaSmallCellServicesFixture {
        ready_msg: DodecaReadyMsg {
            peer_id: 42,
            cell_name: "ddc-cell-fonts".to_string(),
            pid: Some(12_345),
            version: Some("1.0.0-dev".to_string()),
            features: vec!["woff2".to_string(), "subset".to_string()],
        },
        ready_ack: DodecaReadyAck {
            ok: true,
            host_time_unix_ms: Some(1_778_000_000_000),
        },
        minify_result: DodecaMinifyResult::Success {
            content: "<main><h1>Hi</h1></main>".to_string(),
        },
        js_input: DodecaJsRewriteInput {
            js: "import '/assets/theme.css'; console.log('/assets/app.js')".to_string(),
            path_map: BTreeMap::from([
                (
                    "/assets/app.js".to_string(),
                    "/assets/app.1234.js".to_string(),
                ),
                (
                    "/assets/theme.css".to_string(),
                    "/assets/theme.abcd.css".to_string(),
                ),
            ]),
        },
        js_result: Ok(
            "import '/assets/theme.abcd.css'; console.log('/assets/app.1234.js')".to_string(),
        ),
        html_diff_input: DodecaHtmlDiffInput {
            old_html: "<main><h1>Old</h1></main>".to_string(),
            new_html: "<main><h1>New</h1><p>body</p></main>".to_string(),
        },
        html_diff_result: Ok(DodecaHtmlDiffOutcome {
            patches_blob: vec![0x91, 0xa4, b'p', b'a', b't', b'h'],
        }),
        subset_font_input: DodecaSubsetFontInput {
            data: vec![0x77, 0x4f, 0x46, 0x32],
            chars: vec![
                'A',
                char::from_u32(0x00e9).unwrap(),
                char::from_u32(0x1f41d).unwrap(),
            ],
        },
        font_results: vec![
            DodecaFontResult::DecompressSuccess {
                data: vec![0x00, 0x01, 0x00, 0x00],
            },
            DodecaFontResult::SubsetSuccess {
                data: vec![0xde, 0xad, 0xbe, 0xef],
            },
            DodecaFontResult::CompressSuccess {
                data: vec![0x77, 0x4f, 0x46, 0x32, 0x01],
            },
        ],
        webp_encode_input: DodecaWebpEncodeInput {
            pixels: vec![0, 32, 64, 255, 255, 128, 0, 255],
            width: 2,
            height: 1,
            quality: 82,
        },
        webp_results: vec![
            DodecaWebpResult::DecodeSuccess {
                pixels: vec![0, 32, 64, 255],
                width: 1,
                height: 1,
                channels: 4,
            },
            DodecaWebpResult::EncodeSuccess {
                data: vec![b'R', b'I', b'F', b'F'],
            },
        ],
        jxl_encode_input: DodecaJxlEncodeInput {
            pixels: vec![0, 0, 0, 255, 255, 255, 255, 255],
            width: 2,
            height: 1,
            quality: 90,
        },
        jxl_results: vec![
            DodecaJxlResult::DecodeSuccess {
                pixels: vec![255, 0, 255, 255],
                width: 1,
                height: 1,
                channels: 4,
            },
            DodecaJxlResult::Error {
                message: "unsupported color profile".to_string(),
            },
        ],
        select_result: DodecaSelectResult::Selected { index: 2 },
        confirm_result: DodecaConfirmResult::Yes,
        record_config: DodecaRecordConfig {
            shell: Some("/bin/zsh".to_string()),
        },
        term_result: DodecaTermResult::Success {
            html: "<t-b>cargo nextest</t-b>".to_string(),
        },
        start_dev_server_result: DodecaStartDevServerResult::Success { port: 5173 },
        run_build_result: DodecaRunBuildResult::Error {
            message: "vite config missing".to_string(),
        },
        link_check_input: DodecaLinkCheckInput {
            urls: vec![
                "https://example.com/ok".to_string(),
                "https://example.com/missing".to_string(),
            ],
            delay_ms: 250,
            timeout_secs: 15,
        },
        link_check_result: DodecaLinkCheckResult::Success {
            output: DodecaLinkCheckOutput {
                results: BTreeMap::from([
                    ("https://example.com/ok".to_string(), DodecaLinkStatus::Ok),
                    (
                        "https://example.com/missing".to_string(),
                        DodecaLinkStatus::HttpError {
                            code: 404,
                            diagnostics: DodecaLinkDiagnostics {
                                request_headers: vec![(
                                    "accept".to_string(),
                                    "text/html".to_string(),
                                )],
                                response_headers: vec![(
                                    "content-type".to_string(),
                                    "text/html".to_string(),
                                )],
                                response_body: "<h1>not found</h1>".to_string(),
                            },
                        },
                    ),
                    (
                        "https://slow.example.com".to_string(),
                        DodecaLinkStatus::Skipped,
                    ),
                ]),
            },
        },
        build_progress: DodecaBuildProgress {
            parse: sample_dodeca_task_progress("parse", 12, 12, DodecaTaskStatus::Done),
            render: sample_dodeca_task_progress("render", 48, 40, DodecaTaskStatus::Running),
            sass: sample_dodeca_task_progress("sass", 3, 3, DodecaTaskStatus::Done),
            links: sample_dodeca_task_progress("links", 10, 7, DodecaTaskStatus::Running),
            search: sample_dodeca_task_progress("search", 1, 0, DodecaTaskStatus::Pending),
        },
        log_event: DodecaLogEvent {
            level: DodecaLogLevel::Warn,
            kind: DodecaEventKind::Http { status: 404 },
            message: "dead link".to_string(),
            fields: vec![
                ("route".to_string(), "/guide/".to_string()),
                ("href".to_string(), "/missing/".to_string()),
            ],
        },
        server_status: DodecaServerStatus {
            urls: vec![
                "http://127.0.0.1:5173".to_string(),
                "http://192.168.1.42:5173".to_string(),
            ],
            is_running: true,
            bind_mode: DodecaBindMode::Lan,
            picante_cache_size: 4_096,
            cas_cache_size: 8_192,
            code_exec_cache_size: 1_024,
        },
        server_command: DodecaServerCommand::SetLogFilter {
            filter: "dodeca=debug,cell=trace".to_string(),
        },
        command_result: DodecaCommandResult::Ok,
    }
}

fn sample_dodeca_source_lines() -> Vec<DodecaSourceLine> {
    vec![
        DodecaSourceLine {
            number: 12,
            content: "{% for item in data.items %}".to_string(),
        },
        DodecaSourceLine {
            number: 13,
            content: "{{ item.title }}".to_string(),
        },
    ]
}

fn sample_dodeca_source_snippet() -> DodecaSourceSnippet {
    DodecaSourceSnippet {
        lines: sample_dodeca_source_lines(),
        error_line: 13,
    }
}

fn sample_dodeca_error_info() -> DodecaErrorInfo {
    DodecaErrorInfo {
        route: "/guide/".to_string(),
        message: "unknown filter `slugify`".to_string(),
        template: Some("templates/page.html".to_string()),
        line: Some(13),
        column: Some(8),
        source_snippet: Some(sample_dodeca_source_snippet()),
        snapshot_id: "snap-devtools-42".to_string(),
        available_variables: vec!["page".to_string(), "root".to_string(), "data".to_string()],
    }
}

fn sample_dodeca_devtools_event() -> DodecaDevtoolsEvent {
    DodecaDevtoolsEvent::Error(sample_dodeca_error_info())
}

fn sample_dodeca_scope_entries() -> Vec<DodecaScopeEntry> {
    vec![
        DodecaScopeEntry {
            name: "title".to_string(),
            value: DodecaScopeValue::String("Phon migration".to_string()),
            expandable: false,
        },
        DodecaScopeEntry {
            name: "items".to_string(),
            value: DodecaScopeValue::Array {
                length: 3,
                preview: "[intro, install, api]".to_string(),
            },
            expandable: true,
        },
        DodecaScopeEntry {
            name: "metrics".to_string(),
            value: DodecaScopeValue::Object {
                fields: 2,
                preview: "{views, updated_at}".to_string(),
            },
            expandable: true,
        },
        DodecaScopeEntry {
            name: "score".to_string(),
            value: DodecaScopeValue::Number(42.5),
            expandable: false,
        },
    ]
}

fn sample_dodeca_eval_result() -> DodecaEvalResult {
    DodecaEvalResult::Ok(DodecaScopeValue::Object {
        fields: 2,
        preview: "{title, route}".to_string(),
    })
}

fn sample_dodeca_dead_link_target() -> DodecaDeadLinkTarget {
    DodecaDeadLinkTarget::Wiki {
        key: "missing-page".to_string(),
        title: "Missing Page".to_string(),
    }
}

fn sample_dodeca_open_source_result() -> DodecaOpenSourceResult {
    DodecaOpenSourceResult::Ok
}

fn sample_dodeca_sid_lines() -> Vec<DodecaSidLine> {
    vec![
        DodecaSidLine {
            sid: "p-1".to_string(),
            line: 5,
        },
        DodecaSidLine {
            sid: "code-1".to_string(),
            line: 17,
        },
    ]
}

fn sample_dodeca_edit_load() -> DodecaEditLoad {
    DodecaEditLoad::Ok {
        source_key: "content/guide.md".to_string(),
        route: "/guide/".to_string(),
        uri: "file:///workspace/content/guide.md".to_string(),
        content: "# Guide\n\nWelcome to Phon.".to_string(),
        base: "a1b2c3d4".to_string(),
    }
}

fn sample_dodeca_edit_preview() -> DodecaEditPreview {
    DodecaEditPreview::Ok {
        html: "<article><h1>Guide</h1><p>Welcome to Phon.</p></article>".to_string(),
        source_map: sample_dodeca_sid_lines(),
    }
}

fn sample_dodeca_edit_save_req() -> DodecaEditSaveReq {
    DodecaEditSaveReq {
        source_key: "content/guide.md".to_string(),
        buffer: "# Guide\n\nUpdated from browser.".to_string(),
        base: "a1b2c3d4".to_string(),
        message: "Update guide".to_string(),
    }
}

fn sample_dodeca_edit_save() -> DodecaEditSave {
    DodecaEditSave::Ok {
        commit: "deadbeef1234".to_string(),
        base: "b4c3d2a1".to_string(),
    }
}

fn sample_dodeca_edit_upload_req() -> DodecaEditUploadReq {
    DodecaEditUploadReq {
        source_key: "content/guide.md".to_string(),
        filename: "diagram.png".to_string(),
        bytes: byte_ramp(128, 31),
    }
}

fn sample_dodeca_edit_upload() -> DodecaEditUpload {
    DodecaEditUpload::Ok {
        markdown: "![diagram](./diagram.png)".to_string(),
        path: "diagram.png".to_string(),
    }
}

fn sample_dodeca_edit_read() -> DodecaEditRead {
    DodecaEditRead::Ok {
        content: "# Guide\n\nWelcome to Phon.".to_string(),
        base: "a1b2c3d4".to_string(),
    }
}

fn sample_dodeca_edit_list() -> DodecaEditList {
    DodecaEditList::Ok {
        entries: vec![
            DodecaEditEntry {
                source_key: "content/guide.md".to_string(),
                route: "/guide/".to_string(),
                uri: "file:///workspace/content/guide.md".to_string(),
                title: "Guide".to_string(),
            },
            DodecaEditEntry {
                source_key: "content/reference.md".to_string(),
                route: "/reference/".to_string(),
                uri: "file:///workspace/content/reference.md".to_string(),
                title: "Reference".to_string(),
            },
        ],
    }
}

fn sample_dodeca_resolved_dependency() -> DodecaResolvedDependency {
    DodecaResolvedDependency {
        name: "facet".to_string(),
        version: "0.46.0".to_string(),
        source: DodecaDependencySource::Git {
            url: "https://github.com/facet-rs/facet".to_string(),
            commit: "abc1234".to_string(),
        },
    }
}

fn sample_dodeca_code_metadata() -> DodecaCodeExecutionMetadata {
    DodecaCodeExecutionMetadata {
        rustc_version: "rustc 1.89.0".to_string(),
        cargo_version: "cargo 1.89.0".to_string(),
        target: "aarch64-apple-darwin".to_string(),
        timestamp: "2026-06-05T00:00:00Z".to_string(),
        cache_hit: true,
        platform: "macos".to_string(),
        arch: "aarch64".to_string(),
        dependencies: vec![sample_dodeca_resolved_dependency()],
    }
}

fn sample_dodeca_responsive_image_info() -> DodecaResponsiveImageInfo {
    DodecaResponsiveImageInfo {
        jxl_srcset: vec![
            ("/assets/hero-640.jxl".to_string(), 640),
            ("/assets/hero-1280.jxl".to_string(), 1280),
        ],
        webp_srcset: vec![("/assets/hero-640.webp".to_string(), 640)],
        original_width: 1920,
        original_height: 1080,
        thumbhash_data_url: "data:image/png;base64,dGh1bWI=".to_string(),
    }
}

fn sample_dodeca_html_process_input() -> DodecaHtmlProcessInput {
    DodecaHtmlProcessInput {
        html: "<main><a href=\"/missing\">missing</a><img src=\"/hero.png\"></main>".to_string(),
        path_map: Some(BTreeMap::from([(
            "/old/hero.png".to_string(),
            "/assets/hero.png".to_string(),
        )])),
        known_routes: Some(BTreeSet::from(["/".to_string(), "/guide/".to_string()])),
        code_metadata: Some(BTreeMap::from([(
            "sample-1".to_string(),
            sample_dodeca_code_metadata(),
        )])),
        injections: vec![
            DodecaInjection::HeadStyle {
                css: "body { color: oklch(0.2 0.03 240); }".to_string(),
            },
            DodecaInjection::HeadScript {
                js: "console.log('dodeca')".to_string(),
                module: true,
            },
            DodecaInjection::BodyScript {
                js: "window.__dodeca = true".to_string(),
                module: false,
            },
        ],
        minify: Some(DodecaMinifyOptions {
            minify_inline_css: true,
            minify_inline_js: true,
            minify_html: false,
        }),
        source_to_route: Some(BTreeMap::from([(
            "content/guide.md".to_string(),
            "/guide/".to_string(),
        )])),
        wiki_to_route: Some(BTreeMap::from([(
            "getting-started".to_string(),
            "/guide/".to_string(),
        )])),
        base_route: Some("/guide/intro/".to_string()),
        image_variants: Some(BTreeMap::from([(
            "/hero.png".to_string(),
            sample_dodeca_responsive_image_info(),
        )])),
        vite_css_map: Some(BTreeMap::from([(
            "/src/main.ts".to_string(),
            vec![
                "/assets/main.css".to_string(),
                "/assets/theme.css".to_string(),
            ],
        )])),
        mount: Some(DodecaMountLocalization {
            segment: "wiki".to_string(),
            routes: BTreeSet::from(["/exec/".to_string(), "/guide/".to_string()]),
        }),
    }
}

fn sample_dodeca_html_process_result() -> DodecaHtmlProcessResult {
    DodecaHtmlProcessResult::Success {
        html: "<main data-processed=\"true\"><a data-dead href=\"/missing\">missing</a></main>"
            .to_string(),
        had_dead_links: true,
        had_code_buttons: true,
        hrefs: vec!["/missing".to_string(), "/guide/".to_string()],
        element_ids: vec!["intro".to_string(), "sample-1".to_string()],
        unresolved_wiki_links: vec![DodecaWikiLinkRef {
            key: "unknown".to_string(),
            target: "Missing Page".to_string(),
        }],
    }
}

fn sample_dodeca_dependency_spec() -> DodecaDependencySpec {
    DodecaDependencySpec {
        name: "facet".to_string(),
        version: "0.46".to_string(),
        git: Some("https://github.com/facet-rs/facet".to_string()),
        rev: None,
        branch: Some("main".to_string()),
        path: None,
        features: Some(vec!["derive".to_string()]),
    }
}

fn sample_dodeca_rust_config() -> DodecaRustConfig {
    DodecaRustConfig {
        command: Some("cargo".to_string()),
        args: Some(vec!["run".to_string(), "--quiet".to_string()]),
        extension: Some("rs".to_string()),
        prepare_code: Some(true),
        auto_imports: Some(vec![
            "use std::collections::HashMap;".to_string(),
            "use facet::Facet;".to_string(),
        ]),
        show_output: Some(true),
    }
}

fn sample_dodeca_code_execution_config() -> DodecaCodeExecutionConfig {
    DodecaCodeExecutionConfig {
        enabled: true,
        fail_on_error: true,
        timeout_secs: 30,
        cache_dir: ".cache/code-execution".to_string(),
        project_root: Some("/workspace/docs".to_string()),
        dependencies: vec![sample_dodeca_dependency_spec()],
        rust: Some(sample_dodeca_rust_config()),
    }
}

fn sample_dodeca_code_sample() -> DodecaCodeSample {
    DodecaCodeSample {
        source_path: "content/guide.md".to_string(),
        line: 42,
        language: "rust".to_string(),
        code: "#[derive(Facet)]\nstruct Card { title: String }".to_string(),
        executable: true,
        expected_errors: vec![],
    }
}

fn sample_dodeca_build_metadata() -> DodecaBuildMetadata {
    DodecaBuildMetadata {
        rustc_version: "rustc 1.89.0".to_string(),
        cargo_version: "cargo 1.89.0".to_string(),
        target: "aarch64-apple-darwin".to_string(),
        timestamp: "2026-06-05T00:00:00Z".to_string(),
        cache_hit: false,
        platform: "macos".to_string(),
        arch: "aarch64".to_string(),
        dependencies: vec![sample_dodeca_resolved_dependency()],
    }
}

fn sample_dodeca_execute_samples_input() -> DodecaExecuteSamplesInput {
    DodecaExecuteSamplesInput {
        samples: vec![sample_dodeca_code_sample()],
        config: sample_dodeca_code_execution_config(),
    }
}

fn sample_dodeca_code_execution_result() -> DodecaCodeExecutionResult {
    let sample = sample_dodeca_code_sample();
    DodecaCodeExecutionResult::ExecuteSuccess {
        output: DodecaExecuteSamplesOutput {
            results: vec![(
                sample,
                DodecaExecutionResult {
                    status: DodecaExecutionStatus::Success,
                    exit_code: Some(0),
                    stdout: "Card { title: \"Phon\" }".to_string(),
                    stderr: String::new(),
                    duration_ms: 128,
                    error: None,
                    metadata: Some(sample_dodeca_build_metadata()),
                },
            )],
        },
    }
}

fn styx_span(start: u32, end: u32) -> Option<StyxSpan> {
    Some(StyxSpan { start, end })
}

fn styx_scalar(text: &str, kind: StyxScalarKind, start: u32, end: u32) -> StyxValue {
    StyxValue {
        tag: None,
        payload: Some(StyxPayload::Scalar(StyxScalar {
            text: text.to_string(),
            kind,
            span: styx_span(start, end),
        })),
        span: styx_span(start, end),
    }
}

fn sample_styx_value() -> StyxValue {
    StyxValue {
        tag: Some(StyxTag {
            name: "schema".to_string(),
            span: styx_span(0, 7),
        }),
        payload: Some(StyxPayload::Object(StyxObject {
            entries: vec![
                StyxEntry {
                    key: styx_scalar("title", StyxScalarKind::Bare, 9, 14),
                    value: styx_scalar("Phon migration", StyxScalarKind::Quoted, 15, 31),
                    doc_comment: Some("page title".to_string()),
                },
                StyxEntry {
                    key: styx_scalar("features", StyxScalarKind::Bare, 33, 41),
                    value: StyxValue {
                        tag: Some(StyxTag {
                            name: "seq".to_string(),
                            span: styx_span(42, 46),
                        }),
                        payload: Some(StyxPayload::Sequence(StyxSequence {
                            items: vec![
                                styx_scalar("jit", StyxScalarKind::Bare, 47, 50),
                                StyxValue {
                                    tag: Some(StyxTag {
                                        name: "object".to_string(),
                                        span: styx_span(51, 58),
                                    }),
                                    payload: Some(StyxPayload::Object(StyxObject {
                                        entries: vec![StyxEntry {
                                            key: styx_scalar("lang", StyxScalarKind::Bare, 59, 63),
                                            value: styx_scalar("rust", StyxScalarKind::Raw, 64, 70),
                                            doc_comment: None,
                                        }],
                                        span: styx_span(58, 71),
                                    })),
                                    span: styx_span(51, 71),
                                },
                            ],
                            span: styx_span(46, 72),
                        })),
                        span: styx_span(42, 72),
                    },
                    doc_comment: None,
                },
            ],
            span: styx_span(8, 73),
        })),
        span: styx_span(0, 73),
    }
}

fn sample_styx_lsp_uri() -> String {
    "file:///workspace/queries.styx".to_string()
}

fn sample_styx_lsp_source() -> String {
    "@query { from products select (id name) }".to_string()
}

fn sample_styx_lsp_cursor() -> StyxLspCursor {
    StyxLspCursor {
        line: 0,
        character: 16,
        offset: 16,
    }
}

fn sample_styx_lsp_range() -> StyxLspRange {
    StyxLspRange {
        start: StyxLspPosition {
            line: 0,
            character: 0,
        },
        end: StyxLspPosition {
            line: 0,
            character: 38,
        },
    }
}

fn sample_styx_lsp_initialize_params() -> StyxLspInitializeParams {
    StyxLspInitializeParams {
        styx_version: "4.0".to_string(),
        document_uri: sample_styx_lsp_uri(),
        schema_id: "crate:dibs-queries@1".to_string(),
    }
}

fn sample_styx_lsp_initialize_result() -> StyxLspInitializeResult {
    StyxLspInitializeResult {
        name: "dibs-styx-extension".to_string(),
        version: "0.1.0".to_string(),
        capabilities: vec![
            StyxLspCapability::Completions,
            StyxLspCapability::Hover,
            StyxLspCapability::Diagnostics,
            StyxLspCapability::CodeActions,
            StyxLspCapability::Definition,
        ],
    }
}

fn sample_styx_lsp_completion_params() -> StyxLspCompletionParams {
    StyxLspCompletionParams {
        document_uri: sample_styx_lsp_uri(),
        cursor: sample_styx_lsp_cursor(),
        path: vec![
            "AllProducts".to_string(),
            "@query".to_string(),
            "select".to_string(),
        ],
        prefix: "na".to_string(),
        context: Some(sample_styx_value()),
        tagged_context: Some(sample_styx_value()),
    }
}

fn sample_styx_lsp_completions() -> Vec<StyxLspCompletionItem> {
    vec![
        StyxLspCompletionItem {
            label: "name".to_string(),
            detail: Some("TEXT".to_string()),
            documentation: Some("Product display name".to_string()),
            kind: Some(StyxLspCompletionKind::Field),
            sort_text: Some("0001".to_string()),
            insert_text: None,
        },
        StyxLspCompletionItem {
            label: "metadata".to_string(),
            detail: Some("JSONB".to_string()),
            documentation: None,
            kind: Some(StyxLspCompletionKind::Field),
            sort_text: Some("0002".to_string()),
            insert_text: Some("metadata".to_string()),
        },
    ]
}

fn sample_styx_lsp_hover_params() -> StyxLspHoverParams {
    StyxLspHoverParams {
        document_uri: sample_styx_lsp_uri(),
        cursor: sample_styx_lsp_cursor(),
        path: vec![
            "AllProducts".to_string(),
            "@query".to_string(),
            "from".to_string(),
        ],
        context: Some(sample_styx_value()),
        tagged_context: Some(sample_styx_value()),
    }
}

fn sample_styx_lsp_hover_result() -> StyxLspHoverResult {
    StyxLspHoverResult {
        contents: "**products** table\n\nBacked by `Product`.".to_string(),
        range: Some(StyxLspRange {
            start: StyxLspPosition {
                line: 0,
                character: 14,
            },
            end: StyxLspPosition {
                line: 0,
                character: 22,
            },
        }),
    }
}

fn sample_styx_lsp_inlay_hint_params() -> StyxLspInlayHintParams {
    StyxLspInlayHintParams {
        document_uri: sample_styx_lsp_uri(),
        range: sample_styx_lsp_range(),
        context: Some(sample_styx_value()),
    }
}

fn sample_styx_lsp_inlay_hints() -> Vec<StyxLspInlayHint> {
    vec![StyxLspInlayHint {
        position: StyxLspPosition {
            line: 0,
            character: 9,
        },
        label: "Product".to_string(),
        kind: Some(StyxLspInlayHintKind::Type),
        padding_left: true,
        padding_right: false,
    }]
}

fn sample_styx_lsp_diagnostic() -> StyxLspDiagnostic {
    StyxLspDiagnostic {
        span: StyxSpan { start: 23, end: 29 },
        severity: StyxLspDiagnosticSeverity::Warning,
        message: "column `legacy` is deprecated".to_string(),
        source: Some("dibs".to_string()),
        code: Some("deprecated-column".to_string()),
        data: Some(sample_styx_value()),
    }
}

fn sample_styx_lsp_diagnostic_params() -> StyxLspDiagnosticParams {
    StyxLspDiagnosticParams {
        document_uri: sample_styx_lsp_uri(),
        tree: sample_styx_value(),
        content: sample_styx_lsp_source(),
    }
}

fn sample_styx_lsp_diagnostics() -> Vec<StyxLspDiagnostic> {
    vec![sample_styx_lsp_diagnostic()]
}

fn sample_styx_lsp_code_action_params() -> StyxLspCodeActionParams {
    StyxLspCodeActionParams {
        document_uri: sample_styx_lsp_uri(),
        span: StyxSpan { start: 23, end: 29 },
        diagnostics: sample_styx_lsp_diagnostics(),
    }
}

fn sample_styx_lsp_code_actions() -> Vec<StyxLspCodeAction> {
    vec![StyxLspCodeAction {
        title: "Replace legacy column".to_string(),
        kind: Some(StyxLspCodeActionKind::QuickFix),
        edit: Some(StyxLspWorkspaceEdit {
            changes: vec![StyxLspDocumentEdit {
                uri: sample_styx_lsp_uri(),
                edits: vec![StyxLspTextEdit {
                    span: StyxSpan { start: 23, end: 29 },
                    new_text: "name".to_string(),
                }],
            }],
        }),
        is_preferred: true,
    }]
}

fn sample_styx_lsp_definition_params() -> StyxLspDefinitionParams {
    StyxLspDefinitionParams {
        document_uri: sample_styx_lsp_uri(),
        cursor: sample_styx_lsp_cursor(),
        path: vec![
            "AllProducts".to_string(),
            "@query".to_string(),
            "from".to_string(),
        ],
        context: Some(sample_styx_value()),
        tagged_context: Some(sample_styx_value()),
    }
}

fn sample_styx_lsp_locations() -> Vec<StyxLspLocation> {
    vec![StyxLspLocation {
        uri: "file:///workspace/schema.styx".to_string(),
        span: StyxSpan {
            start: 120,
            end: 128,
        },
    }]
}

fn sample_styx_lsp_get_subtree_params() -> StyxLspGetSubtreeParams {
    StyxLspGetSubtreeParams {
        document_uri: sample_styx_lsp_uri(),
        path: vec!["AllProducts".to_string(), "@query".to_string()],
    }
}

fn sample_styx_lsp_get_document_params() -> StyxLspGetDocumentParams {
    StyxLspGetDocumentParams {
        document_uri: sample_styx_lsp_uri(),
    }
}

fn sample_styx_lsp_get_source_params() -> StyxLspGetSourceParams {
    StyxLspGetSourceParams {
        document_uri: sample_styx_lsp_uri(),
    }
}

fn sample_styx_lsp_get_schema_params() -> StyxLspGetSchemaParams {
    StyxLspGetSchemaParams {
        document_uri: sample_styx_lsp_uri(),
    }
}

fn sample_styx_lsp_schema_info() -> StyxLspSchemaInfo {
    StyxLspSchemaInfo {
        source: "@schema { @ @object{ name @string } }".to_string(),
        uri: "styx-embedded://crate:dibs-queries@1".to_string(),
    }
}

fn sample_styx_lsp_offset_to_position_params() -> StyxLspOffsetToPositionParams {
    StyxLspOffsetToPositionParams {
        document_uri: sample_styx_lsp_uri(),
        offset: 16,
    }
}

fn sample_styx_lsp_position_to_offset_params() -> StyxLspPositionToOffsetParams {
    StyxLspPositionToOffsetParams {
        document_uri: sample_styx_lsp_uri(),
        position: StyxLspPosition {
            line: 0,
            character: 16,
        },
    }
}

fn stax_off_cpu(seed: u64) -> StaxOffCpuBreakdown {
    StaxOffCpuBreakdown {
        idle_ns: seed + 1,
        lock_ns: seed + 2,
        semaphore_ns: seed + 3,
        ipc_ns: seed + 4,
        io_read_ns: seed + 5,
        io_write_ns: seed + 6,
        readiness_ns: seed + 7,
        sleep_ns: seed + 8,
        connect_ns: seed + 9,
        other_ns: seed + 10,
    }
}

fn sample_stax_view_params() -> StaxViewParams {
    StaxViewParams {
        tid: Some(42),
        filter: StaxLiveFilter {
            time_range: Some(StaxTimeRange {
                start_ns: 1_000,
                end_ns: 8_500,
            }),
            exclude_symbols: vec![
                StaxSymbolRef {
                    function_name: Some("malloc_zone_malloc".to_string()),
                    binary: Some("libsystem_malloc.dylib".to_string()),
                },
                StaxSymbolRef {
                    function_name: None,
                    binary: Some("libswift_Concurrency.dylib".to_string()),
                },
            ],
        },
    }
}

fn sample_stax_flamegraph_update(params: &StaxViewParams) -> StaxFlamegraphUpdate {
    let tid = params.tid.unwrap_or(0);
    let filter_count = params.filter.exclude_symbols.len() as u64;
    let range_ns = params
        .filter
        .time_range
        .map(|range| range.end_ns.saturating_sub(range.start_ns))
        .unwrap_or(0);
    let total_on_cpu_ns = 120_000 + tid as u64 + range_ns.min(1_000);

    StaxFlamegraphUpdate {
        total_on_cpu_ns,
        total_off_cpu: stax_off_cpu(100 + filter_count),
        strings: vec![
            "root".to_string(),
            "bee::decode".to_string(),
            "libbee.dylib".to_string(),
            "rust".to_string(),
            "phon::jit".to_string(),
            "libphon.dylib".to_string(),
        ],
        root: StaxFlameNode {
            address: 0,
            function_name: Some(0),
            binary: None,
            is_main: true,
            language: 3,
            on_cpu_ns: total_on_cpu_ns,
            off_cpu: stax_off_cpu(200 + filter_count),
            pet_samples: 64,
            off_cpu_intervals: 3,
            cycles: 900_000,
            instructions: 600_000,
            l1d_misses: 42,
            branch_mispreds: 7,
            children: vec![
                StaxFlameNode {
                    address: 0x1000 + tid as u64,
                    function_name: Some(1),
                    binary: Some(2),
                    is_main: true,
                    language: 3,
                    on_cpu_ns: 80_000 + filter_count,
                    off_cpu: stax_off_cpu(300 + filter_count),
                    pet_samples: 48,
                    off_cpu_intervals: 2,
                    cycles: 500_000,
                    instructions: 350_000,
                    l1d_misses: 30,
                    branch_mispreds: 5,
                    children: vec![StaxFlameNode {
                        address: 0x2000 + tid as u64,
                        function_name: Some(4),
                        binary: Some(5),
                        is_main: false,
                        language: 3,
                        on_cpu_ns: 45_000,
                        off_cpu: stax_off_cpu(400 + filter_count),
                        pet_samples: 32,
                        off_cpu_intervals: 1,
                        cycles: 250_000,
                        instructions: 180_000,
                        l1d_misses: 18,
                        branch_mispreds: 3,
                        children: vec![],
                    }],
                },
                StaxFlameNode {
                    address: 0x3000 + tid as u64,
                    function_name: None,
                    binary: Some(2),
                    is_main: false,
                    language: 3,
                    on_cpu_ns: 20_000,
                    off_cpu: stax_off_cpu(500 + filter_count),
                    pet_samples: 12,
                    off_cpu_intervals: 0,
                    cycles: 120_000,
                    instructions: 70_000,
                    l1d_misses: 4,
                    branch_mispreds: 1,
                    children: vec![],
                },
            ],
        },
    }
}

fn sample_stax_secondary_view_params() -> StaxViewParams {
    StaxViewParams {
        tid: None,
        filter: StaxLiveFilter {
            time_range: Some(StaxTimeRange {
                start_ns: 9_000,
                end_ns: 9_640,
            }),
            exclude_symbols: vec![StaxSymbolRef {
                function_name: Some("mach_msg2_trap".to_string()),
                binary: None,
            }],
        },
    }
}

fn sample_stax_flamegraph_updates() -> Vec<StaxFlamegraphUpdate> {
    vec![
        sample_stax_flamegraph_update(&sample_stax_view_params()),
        sample_stax_flamegraph_update(&sample_stax_secondary_view_params()),
    ]
}

fn sample_stax_linux_broker_control_fixture() -> StaxLinuxBrokerControlFixture {
    StaxLinuxBrokerControlFixture {
        config: StaxLinuxPerfSessionConfig {
            target_pid: 42_424,
            frequency_hz: 997,
            kernel_stacks: true,
            request_waking: true,
            request_pmu: true,
            request_dwarf_unwind: false,
        },
        status: StaxLinuxDaemonStatus {
            version: "1.0.0-dev".to_string(),
            host_arch: "x86_64".to_string(),
            privileged: true,
            perf_event_paranoid: 1,
        },
        errors: vec![
            StaxLinuxPerfSessionError::NotPrivileged {
                detail: "perf_event_paranoid=3 without CAP_PERFMON".to_string(),
            },
            StaxLinuxPerfSessionError::PerfEventOpen {
                cpu: 3,
                errno: 24,
                detail: "EMFILE while opening PMU sibling".to_string(),
            },
            StaxLinuxPerfSessionError::NoSuchTarget(99_999),
            StaxLinuxPerfSessionError::NotAuthorized {
                caller_uid: 501,
                target_uid: 0,
            },
        ],
        waking_field_offsets: Some(StaxLinuxWakingFieldOffsets {
            wakee_pid_offset: 16,
            wakee_pid_size: 4,
        }),
    }
}

fn sample_stax_macos_config() -> StaxMacSessionConfig {
    StaxMacSessionConfig {
        target_pid: 42_424,
        frequency_hz: 997,
        buf_records: 1_048_576,
        samplers: 0x1 | 0x2 | 0x10,
        pmu_event_configs: vec![0xfeed_beef, 0x1_0000_0001],
        class_mask: 0b1011,
        filter_range_value1: 0x3100_0000,
        filter_range_value2: 0x31ff_ffff,
        typefilter_cscs: vec![0x3101, 0x3102, 0x3108],
    }
}

fn sample_stax_macos_batches() -> Vec<StaxMacKdBufBatch> {
    vec![
        StaxMacKdBufBatch {
            records: vec![
                StaxMacKdBuf {
                    timestamp: 900_000,
                    arg1: 0x1000,
                    arg2: 0x2000,
                    arg3: 0x3000,
                    arg4: 0x4000,
                    arg5: 0xfeed_face,
                    debugid: 0x3101_0004,
                    cpuid: 3,
                    unused: 0,
                },
                StaxMacKdBuf {
                    timestamp: 900_128,
                    arg1: 0x1008,
                    arg2: 0x2008,
                    arg3: 0x3008,
                    arg4: 0x4008,
                    arg5: 0xfeed_face,
                    debugid: 0x3101_0008,
                    cpuid: 4,
                    unused: 0,
                },
            ],
            read_started_mach_ticks: 899_900,
            drained_mach_ticks: 900_140,
            queued_for_send_mach_ticks: 900_150,
            send_started_mach_ticks: 900_180,
            drained_at_unix_ns: 1_801_000_000_123_456_789,
        },
        StaxMacKdBufBatch {
            records: vec![StaxMacKdBuf {
                timestamp: 900_256,
                arg1: 0x1010,
                arg2: 0x2010,
                arg3: 0x3010,
                arg4: 0x4010,
                arg5: 0xfeed_face,
                debugid: 0x3101_000c,
                cpuid: 5,
                unused: 0,
            }],
            read_started_mach_ticks: 900_200,
            drained_mach_ticks: 900_270,
            queued_for_send_mach_ticks: 900_290,
            send_started_mach_ticks: 900_310,
            drained_at_unix_ns: 1_801_000_000_123_556_789,
        },
    ]
}

fn sample_stax_macos_record_summary() -> StaxMacRecordSummary {
    StaxMacRecordSummary {
        records_drained: sample_stax_macos_batches()
            .iter()
            .map(|batch| batch.records.len() as u64)
            .sum(),
        session_ns: 240_000,
    }
}

fn sample_hotmeal_live_reload_events() -> Vec<HotmealLiveReloadEvent> {
    vec![
        HotmealLiveReloadEvent::Reload,
        HotmealLiveReloadEvent::Patches {
            route: "/guide/".to_string(),
            patches_blob: vec![0, 1, 2, 3, 255],
        },
        HotmealLiveReloadEvent::HeadChanged {
            route: "/guide/".to_string(),
        },
    ]
}

fn sample_hotmeal_route() -> String {
    "/guide/".to_string()
}

fn sample_hotmeal_dom_node() -> HotmealDomNode {
    HotmealDomNode::Element {
        tag: "main".to_string(),
        attrs: vec![
            HotmealDomAttr {
                name: "id".to_string(),
                value: "app".to_string(),
            },
            HotmealDomAttr {
                name: "data-route".to_string(),
                value: "/guide/".to_string(),
            },
        ],
        children: vec![
            HotmealDomNode::Text("Hello ".to_string()),
            HotmealDomNode::Element {
                tag: "button".to_string(),
                attrs: vec![HotmealDomAttr {
                    name: "class".to_string(),
                    value: "primary".to_string(),
                }],
                children: vec![HotmealDomNode::Text("Reload".to_string())],
            },
            HotmealDomNode::Comment("hotmeal-marker".to_string()),
        ],
    }
}

fn sample_hotmeal_apply_patches_result() -> HotmealApplyPatchesResult {
    let initial = sample_hotmeal_dom_node();
    HotmealApplyPatchesResult {
        result_html: "<main id=\"app\"><button class=\"primary\">Reload</button></main>"
            .to_string(),
        normalized_old_html: "<main id=\"app\">Hello</main>".to_string(),
        initial_dom_tree: initial.clone(),
        patch_trace: vec![
            HotmealPatchStep {
                index: 0,
                patch_debug: "ReplaceText(path=[0], text=\"Hello \")".to_string(),
                html_after: "<main id=\"app\">Hello </main>".to_string(),
                dom_tree: initial,
                error: None,
            },
            HotmealPatchStep {
                index: 1,
                patch_debug: "InsertChild(path=[1], tag=\"button\")".to_string(),
                html_after: "<main id=\"app\">Hello <button>Reload</button></main>".to_string(),
                dom_tree: HotmealDomNode::Element {
                    tag: "main".to_string(),
                    attrs: vec![HotmealDomAttr {
                        name: "id".to_string(),
                        value: "app".to_string(),
                    }],
                    children: vec![
                        HotmealDomNode::Text("Hello ".to_string()),
                        HotmealDomNode::Element {
                            tag: "button".to_string(),
                            attrs: vec![],
                            children: vec![HotmealDomNode::Text("Reload".to_string())],
                        },
                    ],
                },
                error: Some("sample recoverable mismatch".to_string()),
            },
        ],
    }
}

fn sample_helix_stream_metrics() -> HelixStreamMetrics {
    HelixStreamMetrics {
        pulse_ids: vec![
            HelixSchedulerPulseId(101),
            HelixSchedulerPulseId(102),
            HelixSchedulerPulseId(103),
        ],
        pulse_duration_us: vec![8_100, 8_250, 8_400],
        decoded_tokens: vec![4, 5, 3],
        committed_tokens: vec![2, 4, 3],
        retained_speculative_tokens: vec![1, 2, 1],
        evicted_audio_tokens: vec![0, 16, 0],
        evicted_committed_tokens: vec![0, 0, 1],
        rewind_k: vec![0, 2, 1],
        ar_token_count: vec![4, 6, 3],
        rolling_wer: vec![0.25, 0.20, 0.18],
        s2d_p50_ms: vec![41.5, 39.0, 37.25],
    }
}

fn helix_audio_range(start: u32, end: u32) -> HelixAudioTokenRange {
    HelixAudioTokenRange {
        start: HelixAudioTokenId(start),
        end: HelixAudioTokenId(end),
    }
}

fn sample_helix_verify_evidence() -> HelixVerifyEvidenceDigest {
    HelixVerifyEvidenceDigest {
        pulse_id: HelixSchedulerPulseId(102),
        rewind_k: 2,
        accepted_prefix_len: Some(1),
        divergence_row: Some(1),
        drafts: vec![
            HelixVerifyDraftRow {
                draft_index: 0,
                draft_token_id: 812,
                verified_text_token_id: HelixTextTokenId(44),
                text: "hel".to_string(),
                status: HelixVerifyDraftStatus::Accepted,
                expected_observed_audio: helix_audio_range(10, 18),
                max_dominant_audio_mass: 0.73,
                record_count: 8,
                max_logit: 12.5,
                draft_logit: 12.4,
            },
            HelixVerifyDraftRow {
                draft_index: 1,
                draft_token_id: 927,
                verified_text_token_id: HelixTextTokenId(45),
                text: "ix".to_string(),
                status: HelixVerifyDraftStatus::Divergent,
                expected_observed_audio: helix_audio_range(18, 26),
                max_dominant_audio_mass: 0.61,
                record_count: 8,
                max_logit: 11.2,
                draft_logit: 9.9,
            },
            HelixVerifyDraftRow {
                draft_index: 2,
                draft_token_id: 415,
                verified_text_token_id: HelixTextTokenId(46),
                text: "".to_string(),
                status: HelixVerifyDraftStatus::DiscardedAfterDivergence,
                expected_observed_audio: helix_audio_range(26, 32),
                max_dominant_audio_mass: 0.0,
                record_count: 0,
                max_logit: 0.0,
                draft_logit: 0.0,
            },
        ],
        seed: Some(HelixVerifySeedRow {
            query_row: 3,
            next_token_seed: 1401,
            expected_observed_audio: helix_audio_range(32, 40),
            max_dominant_audio_mass: 0.58,
            record_count: 8,
            max_logit: 10.75,
        }),
    }
}

fn sample_helix_pulse_bundle_fields() -> HelixPulseBundleFields {
    HelixPulseBundleFields {
        prompt_layout: true,
        audio_provenance: true,
        attention_heatmap: true,
        encoder_frontier: true,
        encoder_provenance: true,
        audio_clip: true,
        mel_clip: true,
        pulse_rollup: true,
        timeline: true,
        gpu_chrome_events: true,
        verify_evidence: true,
        scheduler_snapshot: true,
    }
}

fn helix_audio_span(start: u32, end: u32, version: u32) -> HelixAudioRepresentationSpan {
    HelixAudioRepresentationSpan {
        audio: helix_audio_range(start, end),
        audio_representation_version: HelixAudioRepresentationVersion(version),
    }
}

fn sample_helix_audio_provenance() -> Vec<HelixAudioTokenProvenance> {
    vec![
        HelixAudioTokenProvenance {
            audio_token_id: HelixAudioTokenId(16),
            audio_representation_version: HelixAudioRepresentationVersion(7),
            mel_frames: vec![HelixMelFrameRange {
                start: 128,
                end: 136,
            }],
            native_window: HelixNativeEncoderWindowId(2),
            conv_stem_chunk: HelixConvStemChunkId(4),
            post_merge_audio_token_id: HelixAudioTokenId(16),
            merge: HelixAudioTokenMergeProvenance::NoMerge {
                pre_merge_audio_token_id: HelixAudioTokenId(16),
            },
            admission: HelixAudioTokenAdmissionProvenance::AdmitAll {
                admission_segment: HelixAdmissionSegmentId(12),
            },
            cosine_to_previous: Some(0.9825),
        },
        HelixAudioTokenProvenance {
            audio_token_id: HelixAudioTokenId(17),
            audio_representation_version: HelixAudioRepresentationVersion(7),
            mel_frames: vec![
                HelixMelFrameRange {
                    start: 136,
                    end: 144,
                },
                HelixMelFrameRange {
                    start: 144,
                    end: 152,
                },
            ],
            native_window: HelixNativeEncoderWindowId(2),
            conv_stem_chunk: HelixConvStemChunkId(4),
            post_merge_audio_token_id: HelixAudioTokenId(17),
            merge: HelixAudioTokenMergeProvenance::Merged {
                pre_merge: helix_audio_range(17, 19),
            },
            admission: HelixAudioTokenAdmissionProvenance::AdmitAll {
                admission_segment: HelixAdmissionSegmentId(13),
            },
            cosine_to_previous: None,
        },
    ]
}

fn sample_helix_prompt_layout() -> HelixPromptLayout {
    HelixPromptLayout {
        pulse_id: HelixSchedulerPulseId(102),
        first_audio_token_id: HelixAudioTokenId(10),
        resident_audio_frames: 32,
        changed_audio_spans: vec![helix_audio_span(16, 20, 7), helix_audio_span(24, 28, 8)],
        text_token_start: HelixTextTokenId(40),
        text_token_end: HelixTextTokenId(44),
        text_tokens: vec![
            HelixTextTokenSnapshot {
                text_token_id: HelixTextTokenId(40),
                text: Some("hel".to_string()),
                text_before: Some("he".to_string()),
                in_verify_batch: true,
                decoded_this_pulse: false,
            },
            HelixTextTokenSnapshot {
                text_token_id: HelixTextTokenId(41),
                text: Some("ix".to_string()),
                text_before: None,
                in_verify_batch: false,
                decoded_this_pulse: true,
            },
        ],
    }
}

fn sample_helix_attention_heatmap() -> HelixPulseAttentionHeatmap {
    HelixPulseAttentionHeatmap {
        pulse_id: HelixSchedulerPulseId(102),
        first_audio_token_id: HelixAudioTokenId(10),
        audio_token_count: 6,
        text_token_start: HelixTextTokenId(40),
        text_token_count: 2,
        record_count: 16,
        max_value: 0.42,
        mean_audio_mass: vec![
            0.02, 0.04, 0.08, 0.16, 0.28, 0.42, 0.03, 0.05, 0.09, 0.15, 0.24, 0.31,
        ],
        text_token_glyphs: vec!["hel".to_string(), "ix".to_string()],
    }
}

fn sample_helix_encoder_frontier() -> HelixEncoderFrontierSeries {
    HelixEncoderFrontierSeries {
        pulse_id: HelixSchedulerPulseId(102),
        layers: vec![
            HelixEncoderFrontierLayer {
                encoder_layer_index: 0,
                points: vec![
                    HelixEncoderFrontierPoint {
                        audio_token_id: HelixAudioTokenId(16),
                        mean_frontier_debt: 0.12,
                        head_count: 4,
                    },
                    HelixEncoderFrontierPoint {
                        audio_token_id: HelixAudioTokenId(17),
                        mean_frontier_debt: 0.18,
                        head_count: 4,
                    },
                ],
            },
            HelixEncoderFrontierLayer {
                encoder_layer_index: 1,
                points: vec![HelixEncoderFrontierPoint {
                    audio_token_id: HelixAudioTokenId(16),
                    mean_frontier_debt: 0.09,
                    head_count: 4,
                }],
            },
        ],
        min_audio_token_id: HelixAudioTokenId(16),
        max_audio_token_id: HelixAudioTokenId(17),
        min_frontier_debt: 0.09,
        max_frontier_debt: 0.18,
    }
}

fn sample_helix_encoder_provenance_report() -> HelixEncoderProvenanceReport {
    HelixEncoderProvenanceReport {
        pulse_id: HelixSchedulerPulseId(102),
        records_checked: 32,
        violations: vec![HelixEncoderProvenanceViolation {
            audio_token_id: HelixAudioTokenId(18),
            encoder_layer_index: 2,
            head_index: 3,
            observed_audio_token_id: Some(HelixAudioTokenId(21)),
            kind: HelixEncoderProvenanceViolationKind::VersionMismatch,
            message: "observed audio provenance version lagged refresh".to_string(),
        }],
    }
}

fn sample_helix_pulse_rollup() -> HelixPulseRollup {
    HelixPulseRollup {
        pulse_id: HelixSchedulerPulseId(102),
        pulse_start_us: Some(1_000_000),
        pulse_duration_us: Some(8_250),
        encoder_duration_us: Some(2_100),
        refresh_duration_us: Some(1_400),
        verify_duration_us: Some(900),
        decode_duration_us: Some(2_300),
        commit_duration_us: Some(250),
        pulse_mel_frames: 16,
        committed_tokens: 4,
        retained_speculative_tokens: 2,
        resident_committed_tokens: 38,
        evicted_audio_tokens: 16,
        evicted_committed_tokens: 0,
        decoded_tokens: 5,
        hit_eos: false,
        verify: Some(HelixVerifyOutcome {
            rewind_k: 2,
            accepted_prefix_len: Some(1),
            divergence_row: Some(1),
            discarded_speculative_tokens: Some(1),
        }),
        has_attention_batch: true,
        ar_token_count: 6,
    }
}

fn sample_helix_timeline() -> Vec<HelixStreamingTraceEvent> {
    vec![
        HelixStreamingTraceEvent::Pulse {
            start_us: 1_000_000,
            duration_us: 8_250,
            pulse_id: 102,
            previous_consumed_mel_frames: 1_632,
            consumed_mel_frames: 1_648,
            pulse_mel_frames: 16,
            committed_text_len_start: 36,
            speculative_len_start: 3,
            committed_tokens: 4,
            retained_speculative_tokens: 2,
            resident_committed_tokens: 38,
            evicted_audio_tokens: 16,
            evicted_committed_tokens: 0,
        },
        HelixStreamingTraceEvent::AudioEncoderUpdate {
            start_us: 1_000_200,
            duration_us: 2_100,
            pulse_id: 102,
            num_audio_frames: 64,
            first_audio_token_id: 10,
            resident_audio_frames: 32,
            changed_span_count: 2,
            changed_audio_tokens: 8,
            latest_audio_representation_version: 7,
        },
        HelixStreamingTraceEvent::AudioEviction {
            timestamp_us: 1_000_300,
            pulse_id: 102,
            evicted_audio_tokens: 16,
            first_audio_token_id: 10,
            resident_audio_frames: 32,
            audio_ring_capacity: 96,
        },
        HelixStreamingTraceEvent::RefreshPrompt {
            start_us: 1_002_500,
            duration_us: 1_400,
            pulse_id: 102,
            first_audio_token_id: 10,
            resident_audio_frames: 32,
            committed_text_len: 36,
            resident_committed_len: 32,
            resident_text_len: 35,
            logical_start: 80,
            logical_end: 117,
            text_token_start: 40,
            text_token_end: 44,
            spans: vec![HelixTracePositionSpan {
                logical_start: 80,
                rows: 16,
                physical_start: 12,
            }],
        },
        HelixStreamingTraceEvent::LayoutSnapshot {
            timestamp_us: 1_003_950,
            pulse_id: 102,
            audio_len: 32,
            audio_head: 4,
            first_audio_token_id: 10,
            text_len: 35,
            first_text_token_id: 40,
            prompt_len: 67,
            resident_committed_len: 32,
            resident_text_len: 35,
        },
        HelixStreamingTraceEvent::Verify {
            start_us: 1_004_000,
            duration_us: 900,
            pulse_id: 102,
            rewind_k: 2,
            post_rewind_text_len: 37,
            text_token_start: 44,
            text_token_end: 47,
            logical_start: 114,
            logical_end: 117,
            spans: vec![HelixTracePositionSpan {
                logical_start: 114,
                rows: 3,
                physical_start: 46,
            }],
            accepted_prefix_len: Some(1),
            divergence_row: Some(1),
            next_token_seed: Some(1401),
            discarded_speculative_tokens: Some(1),
            invalidated_speculative_slots: Some(2),
        },
        HelixStreamingTraceEvent::ArDecode {
            start_us: 1_005_000,
            duration_us: 2_300,
            pulse_id: 102,
            decode_steps: 5,
            decoded_tokens: 5,
            speculative_len_entering: 1,
            live_speculative_tokens: 6,
            hit_eos: false,
            seed_token_id: 1401,
            seed_token_text: "hel".to_string(),
            early_exit_reason: HelixArDecodeEarlyExitReason::BudgetExhausted,
            next_after_tail: 1502,
        },
        HelixStreamingTraceEvent::ArToken {
            start_us: 1_005_100,
            duration_us: 300,
            pulse_id: 102,
            step_index: 0,
            input_token_id: 1401,
            input_text: "hel".to_string(),
            text_token_id: 47,
            query_position: 118,
            physical_start: 49,
            summary_records: 64,
            next_token_id: 1502,
            next_text: "ix".to_string(),
        },
        HelixStreamingTraceEvent::Commit {
            start_us: 1_007_500,
            duration_us: 250,
            pulse_id: 102,
            speculative_len_pre: 6,
            revisable_tail_target: 2,
            committed_tokens: 4,
            retained_speculative_tokens: 2,
            committed_text_len: 40,
            next_after_committed: 1502,
        },
        HelixStreamingTraceEvent::VerifySkipped {
            timestamp_us: 1_007_800,
            pulse_id: 102,
            reason: HelixVerifySkippedReason::PreCommitFullRewind,
            rewind_k: 0,
            resident_committed_len: 0,
            speculative_len: 2,
        },
        HelixStreamingTraceEvent::TextEviction {
            timestamp_us: 1_007_900,
            pulse_id: 102,
            evicted_committed_tokens: 0,
            resident_committed_capacity: 128,
            committed_text_len: 40,
        },
    ]
}

fn sample_helix_pulse_evidence() -> HelixPulseEvidenceSnapshot {
    HelixPulseEvidenceSnapshot {
        pulse_id: HelixSchedulerPulseId(102),
        encoder: Some(HelixEncoderFactsSnapshot {
            refreshed_audio: helix_audio_range(16, 18),
            audio_representation_version: HelixAudioRepresentationVersion(7),
            provenance: sample_helix_audio_provenance(),
        }),
        counts: HelixDecoderEvidenceFactCounts {
            decode: 1,
            verify_prediction: 1,
            verify_seed: 1,
            prompt_prefill: 1,
        },
        decode: vec![HelixDecodeFact {
            text_token_id: HelixTextTokenId(47),
            query_position: HelixLogicalPosition(118),
            input_token_id: 1401,
            observed_audio: helix_audio_range(10, 18),
        }],
        verify_prediction: vec![HelixVerifyPredictionFact {
            verified_text_token_id: HelixTextTokenId(45),
            verified_draft_index: 1,
            draft_token_id: 927,
            query_row: 2,
            query_position: HelixLogicalPosition(116),
            observed_audio: helix_audio_range(18, 26),
        }],
        verify_seed: vec![HelixVerifySeedFact {
            query_row: 3,
            query_position: HelixLogicalPosition(117),
            next_token_seed: 1401,
            observed_audio: helix_audio_range(32, 40),
        }],
        prompt_prefill: vec![HelixPromptPrefillFact {
            query_position: HelixLogicalPosition(80),
            observed_audio: helix_audio_range(10, 18),
        }],
    }
}

fn sample_helix_pulse_bundle() -> HelixPulseBundle {
    HelixPulseBundle {
        pulse_id: HelixSchedulerPulseId(102),
        schema_version: 1,
        prompt_layout: Some(sample_helix_prompt_layout()),
        audio_provenance: Some(sample_helix_audio_provenance()),
        attention_heatmap: Some(sample_helix_attention_heatmap()),
        encoder_frontier: Some(sample_helix_encoder_frontier()),
        encoder_provenance: Some(sample_helix_encoder_provenance_report()),
        audio_clip: Some(HelixAudioClip {
            sample_rate: 16_000,
            first_sample: 262_144,
            samples: vec![-0.25, -0.10, 0.0, 0.10, 0.25, 0.50, 0.25, 0.0],
        }),
        mel_clip: Some(HelixMelClip {
            num_mel_bins: 4,
            first_mel_frame: 128,
            num_mel_frames: 3,
            values: vec![
                0.10, 0.20, 0.30, 0.40, 0.15, 0.25, 0.35, 0.45, 0.05, 0.12, 0.18, 0.22,
            ],
            min_value: 0.05,
            max_value: 0.45,
            corpus_min_value: -1.25,
            corpus_max_value: 2.75,
        }),
        pulse_rollup: Some(sample_helix_pulse_rollup()),
        timeline: Some(sample_helix_timeline()),
        gpu_chrome_events: Some(vec![
            HelixChromeTraceEvent {
                name: "metal.dispatch".to_string(),
                cat: "gpu".to_string(),
                ph: "X".to_string(),
                ts: 1_006_000.0,
                dur: Some(420.0),
                pid: 2,
                tid: 7,
                s: None,
                args: BTreeMap::new(),
            },
            HelixChromeTraceEvent {
                name: "pulse_marker".to_string(),
                cat: "scheduler".to_string(),
                ph: "i".to_string(),
                ts: 1_007_950.0,
                dur: None,
                pid: 1,
                tid: 0,
                s: Some("p".to_string()),
                args: BTreeMap::new(),
            },
        ]),
        verify_evidence: Some(sample_helix_verify_evidence()),
        scheduler_snapshot: Some(sample_helix_pulse_evidence()),
    }
}

fn sample_helix_pulses() -> Vec<HelixPulseAvailable> {
    vec![
        HelixPulseAvailable {
            pulse_id: HelixSchedulerPulseId(101),
        },
        HelixPulseAvailable {
            pulse_id: HelixSchedulerPulseId(102),
        },
        HelixPulseAvailable {
            pulse_id: HelixSchedulerPulseId(103),
        },
    ]
}

fn tracey_rule_id(base: &str, version: u32) -> TraceyRuleId {
    TraceyRuleId {
        base: base.to_string(),
        version,
    }
}

fn sample_helix_audio_clip() -> HelixAudioClip {
    HelixAudioClip {
        sample_rate: 16_000,
        first_sample: 262_144,
        samples: vec![-0.25, -0.10, 0.0, 0.10, 0.25, 0.50, 0.25, 0.0],
    }
}

fn sample_helix_mel_clip() -> HelixMelClip {
    HelixMelClip {
        num_mel_bins: 4,
        first_mel_frame: 128,
        num_mel_frames: 3,
        values: vec![
            0.10, 0.20, 0.30, 0.40, 0.15, 0.25, 0.35, 0.45, 0.05, 0.12, 0.18, 0.22,
        ],
        min_value: 0.05,
        max_value: 0.45,
        corpus_min_value: -1.25,
        corpus_max_value: 2.75,
    }
}

fn sample_helix_chrome_events() -> Vec<HelixChromeTraceEvent> {
    vec![HelixChromeTraceEvent {
        name: "metal.dispatch".to_string(),
        cat: "gpu".to_string(),
        ph: "X".to_string(),
        ts: 1_006_000.0,
        dur: Some(420.0),
        pid: 2,
        tid: 7,
        s: None,
        args: BTreeMap::new(),
    }]
}

fn sample_helix_support() -> HelixAttentionSupportSummary {
    HelixAttentionSupportSummary {
        total_audio_mass: 0.42,
        observed_audio: helix_audio_range(10, 18),
        dominant_audio: helix_audio_range(16, 18),
        dominant_audio_mass: 0.21,
        center_audio_token: Some(17.25),
        width_audio_tokens: Some(3.5),
    }
}

fn sample_helix_text_support() -> Vec<HelixTextAttentionSupportRecord> {
    vec![HelixTextAttentionSupportRecord {
        text_token_id: HelixTextTokenId(47),
        query_position: HelixLogicalPosition(118),
        decoder_layer_index: 2,
        head_index: 3,
        support: sample_helix_support(),
        audio_weights: vec![0.03125, 0.0625, 0.125, 0.25, 0.5],
    }]
}

fn sample_helix_attention_batch() -> HelixAttentionSummaryBatch {
    HelixAttentionSummaryBatch {
        schema_version: 2,
        pulse_id: HelixSchedulerPulseId(102),
        audio_context_id: 77,
        text_context_id: 99,
        audio_representation_spans: vec![helix_audio_span(10, 18, 7)],
        changed_audio_representation_spans: vec![helix_audio_span(16, 18, 8)],
        text_support: sample_helix_text_support(),
        header_text_support: vec![HelixQueryRowAttentionRecord {
            query_position: HelixLogicalPosition(80),
            decoder_layer_index: 1,
            head_index: 0,
            support: sample_helix_support(),
            audio_weights: vec![0.125, 0.25, 0.375, 0.25],
        }],
        audio_encoder_support: vec![HelixAudioEncoderSupportRecord {
            audio_token_id: HelixAudioTokenId(16),
            audio_representation_version: HelixAudioRepresentationVersion(7),
            encoder_layer_index: 0,
            head_index: 1,
            support: sample_helix_support(),
            frontier_debt: 0.125,
        }],
        decoder_evidence: vec![
            HelixDecoderEvidenceRecord {
                text_token_id: Some(HelixTextTokenId(47)),
                query_position: HelixLogicalPosition(118),
                expected_observed_audio: helix_audio_range(10, 18),
                records: sample_helix_text_support(),
                kind: HelixDecoderEvidenceKind::Decode {
                    input_token_id: 1401,
                },
            },
            HelixDecoderEvidenceRecord {
                text_token_id: Some(HelixTextTokenId(45)),
                query_position: HelixLogicalPosition(116),
                expected_observed_audio: helix_audio_range(18, 26),
                records: sample_helix_text_support(),
                kind: HelixDecoderEvidenceKind::VerifyPrediction {
                    verified_draft_index: 1,
                    draft_token_id: 927,
                    query_row: 2,
                    max_logit: 11.25,
                    draft_logit: 9.875,
                },
            },
            HelixDecoderEvidenceRecord {
                text_token_id: None,
                query_position: HelixLogicalPosition(117),
                expected_observed_audio: helix_audio_range(32, 40),
                records: sample_helix_text_support(),
                kind: HelixDecoderEvidenceKind::VerifySeed {
                    query_row: 3,
                    next_token_seed: 1401,
                    max_logit: 10.75,
                },
            },
            HelixDecoderEvidenceRecord {
                text_token_id: None,
                query_position: HelixLogicalPosition(80),
                expected_observed_audio: helix_audio_range(10, 18),
                records: sample_helix_text_support(),
                kind: HelixDecoderEvidenceKind::PromptPrefill,
            },
        ],
    }
}

fn sample_helix_trace_service_surface() -> HelixTraceServiceSurface {
    HelixTraceServiceSurface {
        meta: HelixStreamMeta {
            schema_version: 2,
            pulse_ids: vec![HelixSchedulerPulseId(101), HelixSchedulerPulseId(102)],
            timeline_event_count: 420,
            attention_batch_count: 17,
        },
        pulse_rollup: Some(sample_helix_pulse_rollup()),
        timeline: sample_helix_timeline(),
        attention_batch: Some(sample_helix_attention_batch()),
        prompt_layout: Some(sample_helix_prompt_layout()),
        audio_attended_by: vec![HelixTextAttendanceRow {
            text_token_id: HelixTextTokenId(47),
            decoder_layer_index: 2,
            head_index: 3,
            dominant_audio_mass: 0.21,
            total_audio_mass: 0.42,
            observed_audio: helix_audio_range(10, 18),
            dominant_audio: helix_audio_range(16, 18),
            audio_weights: vec![0.03125, 0.0625, 0.125, 0.25, 0.5],
            queried_audio_weight: 0.25,
        }],
        text_attends_to: vec![HelixAudioAttendanceRow {
            decoder_layer_index: 2,
            head_index: 3,
            dominant_audio_mass: 0.21,
            total_audio_mass: 0.42,
            center_audio_token: Some(17.25),
            width_audio_tokens: Some(3.5),
            observed_audio: helix_audio_range(10, 18),
            dominant_audio: helix_audio_range(16, 18),
            audio_weights: vec![0.03125, 0.0625, 0.125, 0.25, 0.5],
        }],
        refresh_attends_to: vec![HelixRefreshAttendanceRow {
            query_position: HelixLogicalPosition(80),
            decoder_layer_index: 1,
            head_index: 0,
            dominant_audio_mass: 0.375,
            total_audio_mass: 1.0,
            center_audio_token: Some(15.5),
            width_audio_tokens: Some(4.0),
            observed_audio: helix_audio_range(10, 18),
            dominant_audio: helix_audio_range(14, 18),
            audio_weights: vec![0.125, 0.25, 0.375, 0.25],
        }],
        audio_token_provenance: sample_helix_audio_provenance().into_iter().next(),
        audio_provenance_for_pulse: sample_helix_audio_provenance(),
        audio_tokens_for_mel_frame: vec![HelixAudioTokenId(16), HelixAudioTokenId(17)],
        audio_clip_for_audio_token: Some(sample_helix_audio_clip()),
        audio_clip_for_prompt: Some(sample_helix_audio_clip()),
        audio_clip_for_audio_range: Some(sample_helix_audio_clip()),
        mel_clip_for_prompt: Some(sample_helix_mel_clip()),
        audio_self_attention: vec![HelixAudioSelfAttentionRow {
            encoder_layer_index: 0,
            head_index: 1,
            audio_representation_version: HelixAudioRepresentationVersion(7),
            dominant_audio_mass: 0.25,
            total_audio_mass: 0.5,
            center_audio_token: Some(16.5),
            width_audio_tokens: Some(2.0),
            observed_audio: helix_audio_range(10, 18),
            dominant_audio: helix_audio_range(16, 18),
            frontier_debt: 0.125,
        }],
        transcript: vec![
            HelixTranscriptToken {
                text_token_id: HelixTextTokenId(40),
                decoded_in_pulse: HelixSchedulerPulseId(101),
                text: "hel".to_string(),
                committed: true,
            },
            HelixTranscriptToken {
                text_token_id: HelixTextTokenId(41),
                decoded_in_pulse: HelixSchedulerPulseId(102),
                text: "ix".to_string(),
                committed: false,
            },
        ],
        pulse_attention_heatmap: Some(sample_helix_attention_heatmap()),
        encoder_frontier: Some(sample_helix_encoder_frontier()),
        stream_metrics: sample_helix_stream_metrics(),
        verify_evidence: Some(sample_helix_verify_evidence()),
        decoder_evidence_report: HelixDecoderEvidenceReport {
            total_batches: 7,
            batches_without_decoder_evidence: 1,
            pulses_without_decoder_evidence: vec![HelixSchedulerPulseId(101)],
            variant_evidence_counts: HelixDecoderEvidenceVariantCounts {
                decode: 12,
                verify_prediction: 6,
                verify_seed: 3,
                prompt_prefill: 4,
            },
            variant_record_counts: HelixDecoderEvidenceVariantCounts {
                decode: 96,
                verify_prediction: 48,
                verify_seed: 24,
                prompt_prefill: 32,
            },
            observed_decoder_layer_indices: vec![0, 1, 2],
            observed_decoder_head_indices: vec![0, 1, 2, 3],
        },
        pulse_evidence_snapshot: Some(sample_helix_pulse_evidence()),
        gpu_chrome_events_for_pulse: sample_helix_chrome_events(),
        run_info: Some(HelixRunInfo {
            backend: "metal".to_string(),
            model_dir: "/models/helix-mini".to_string(),
            input: "helix fixture".to_string(),
            piece: Some("demo".to_string()),
            pulse_ms: 8,
            audio_ring_capacity: 4096,
            text_ring_capacity: 512,
            commit_revisable_tail_text_tokens: 4,
            revise_logit_margin: 0.75,
            sample_rate: 16_000,
            mel_hop_samples: 160,
            num_mel_bins: 80,
            num_mel_frames: 384,
            audio_tokens_per_chunk: 2,
            native_window_tokens: 16,
            realtime_pacing: true,
            profile_phases: true,
            attention_trace_schema_version: 3,
            trace_server_schema_version: 5,
        }),
        piece_eval_reference: Some(HelixPieceEvalReference {
            piece: "demo".to_string(),
            language: "en".to_string(),
            words: vec!["helix".to_string(), "fixture".to_string()],
        }),
        piece_eval_for_pulse: Some(HelixPieceEvalSnapshot {
            audio_now_ms: 1234.5,
            reference_words_available: 16,
            hypothesis_words: 15,
            substitutions: 1,
            deletions: 0,
            insertions: 1,
            rolling_wer: 0.125,
            s2d_matched_words: 14,
            s2d_new_words: 2,
            s2d_p50_ms: Some(41.5),
            s2d_p90_ms: Some(75.0),
            s2d_p100_ms: Some(101.25),
            s2d_avg_ms: Some(50.0),
            audio_frontier: 160,
            displayed_frontier: 156,
            committed_frontier: 152,
            lag_ms: 250.0,
        }),
        encoder_provenance_report: Some(sample_helix_encoder_provenance_report()),
        pulse_bundle_fields: sample_helix_pulse_bundle_fields(),
        pulse_bundle: sample_helix_pulse_bundle(),
        pulse_available: HelixPulseAvailable {
            pulse_id: HelixSchedulerPulseId(102),
        },
    }
}

fn sample_tracey_status_response() -> TraceyStatusResponse {
    TraceyStatusResponse {
        impls: vec![
            TraceyImplStatus {
                spec: "vox".to_string(),
                impl_name: "rust".to_string(),
                total_rules: 59,
                covered_rules: 59,
                stale_rules: 0,
                verified_rules: 59,
            },
            TraceyImplStatus {
                spec: "vox".to_string(),
                impl_name: "typescript".to_string(),
                total_rules: 173,
                covered_rules: 173,
                stale_rules: 0,
                verified_rules: 100,
            },
        ],
    }
}

fn sample_tracey_query_request() -> TraceyUncoveredRequest {
    TraceyUncoveredRequest {
        spec: Some("vox".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: Some("rpc.channel".to_string()),
    }
}

fn sample_tracey_untested_request() -> TraceyUntestedRequest {
    TraceyUntestedRequest {
        spec: Some("vox".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: Some("rpc.channel".to_string()),
    }
}

fn sample_tracey_stale_request() -> TraceyStaleRequest {
    TraceyStaleRequest {
        spec: Some("vox".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: Some("rpc.channel".to_string()),
    }
}

fn sample_tracey_unmapped_request() -> TraceyUnmappedRequest {
    TraceyUnmappedRequest {
        spec: Some("vox".to_string()),
        impl_name: Some("rust".to_string()),
        path: Some("rust/vox-codegen/src".to_string()),
    }
}

fn sample_tracey_section_rules() -> Vec<TraceySectionRules> {
    vec![TraceySectionRules {
        section: "Channel Binding".to_string(),
        rules: vec![
            TraceyRuleRef {
                id: tracey_rule_id("rpc.channel.direct-args", 1),
                text: Some("Channels are direct service arguments.".to_string()),
            },
            TraceyRuleRef {
                id: tracey_rule_id("rpc.channel.no-collections", 1),
                text: None,
            },
        ],
    }]
}

fn sample_tracey_uncovered_response() -> TraceyUncoveredResponse {
    TraceyUncoveredResponse {
        spec: "vox".to_string(),
        impl_name: "rust".to_string(),
        total_rules: 175,
        uncovered_count: 2,
        by_section: sample_tracey_section_rules(),
    }
}

fn sample_tracey_untested_response() -> TraceyUntestedResponse {
    TraceyUntestedResponse {
        spec: "vox".to_string(),
        impl_name: "rust".to_string(),
        total_rules: 175,
        untested_count: 3,
        by_section: sample_tracey_section_rules(),
    }
}

fn sample_tracey_stale_response() -> TraceyStaleResponse {
    TraceyStaleResponse {
        spec: "vox".to_string(),
        impl_name: "rust".to_string(),
        total_rules: 175,
        stale_count: 1,
        refs: vec![TraceyStaleEntry {
            current_id: tracey_rule_id("rpc.channel.direct-args", 2),
            file: "rust/vox-codegen/src/targets/swift/mod.rs".to_string(),
            line: 67,
            reference_id: tracey_rule_id("rpc.channel.direct-args", 1),
        }],
    }
}

fn sample_tracey_unmapped_response() -> TraceyUnmappedResponse {
    TraceyUnmappedResponse {
        spec: "vox".to_string(),
        impl_name: "rust".to_string(),
        total_units: 9,
        unmapped_count: 2,
        entries: vec![
            TraceyUnmappedEntry {
                path: "rust/vox-codegen/src/targets".to_string(),
                is_dir: true,
                total_units: 5,
                unmapped_units: 1,
                units: vec![],
            },
            TraceyUnmappedEntry {
                path: "rust/vox-codegen/src/targets/swift/mod.rs".to_string(),
                is_dir: false,
                total_units: 4,
                unmapped_units: 1,
                units: vec![TraceyUnmappedUnit {
                    kind: "function".to_string(),
                    name: Some("emit_tracey_bridge".to_string()),
                    start_line: 41,
                    end_line: 78,
                }],
            },
        ],
    }
}

fn sample_tracey_api_config() -> TraceyApiConfig {
    TraceyApiConfig {
        project_root: "/workspace/vox".to_string(),
        specs: vec![TraceyApiSpecInfo {
            name: "vox".to_string(),
            prefix: "r".to_string(),
            source: Some("docs/content/spec/*.md".to_string()),
            source_url: Some("https://vixen.rs/vox/spec".to_string()),
            implementations: vec![
                "rust".to_string(),
                "swift".to_string(),
                "typescript".to_string(),
            ],
        }],
    }
}

fn sample_tracey_reload_response() -> TraceyReloadResponse {
    TraceyReloadResponse {
        version: 13,
        rebuild_time_ms: 42,
    }
}

fn sample_tracey_health_response() -> TraceyHealthResponse {
    TraceyHealthResponse {
        version: 13,
        watcher_active: true,
        watcher_error: None,
        config_error: Some("ignored include pattern failed to parse".to_string()),
        watcher_last_event_ms: Some(1_717_000_000_123),
        watcher_event_count: 7,
        watched_directories: vec!["docs/content/spec".to_string(), "rust".to_string()],
        uptime_secs: 3600,
    }
}

fn sample_tracey_rule_info() -> TraceyRuleInfo {
    TraceyRuleInfo {
        id: tracey_rule_id("rpc.channel.direct-args", 1),
        raw: "Channels are direct service arguments.".to_string(),
        html: "<p>Channels are direct service arguments.</p>".to_string(),
        source_file: Some("docs/content/spec/vox.md".to_string()),
        source_line: Some(42),
        coverage: vec![TraceyRuleCoverage {
            spec: "vox".to_string(),
            impl_name: "rust".to_string(),
            impl_refs: vec![TraceyCodeRef {
                file: "rust/vox-codegen/src/targets/swift/mod.rs".to_string(),
                line: 67,
            }],
            verify_refs: vec![TraceyCodeRef {
                file: "spec/spec-tests/tests/cases/testbed.rs".to_string(),
                line: 1450,
            }],
        }],
        version_diff: Some("Added direct argument wording.".to_string()),
    }
}

fn sample_tracey_forward_response() -> TraceyApiSpecForward {
    TraceyApiSpecForward {
        name: "vox".to_string(),
        rules: vec![TraceyApiRule {
            id: tracey_rule_id("rpc.channel.direct-args", 2),
            raw: "Channels are direct service arguments.".to_string(),
            html: "<p>Channels are direct service arguments.</p>".to_string(),
            status: Some("stable".to_string()),
            level: Some("must".to_string()),
            source_file: Some("docs/content/spec/rpc.md".to_string()),
            source_line: Some(42),
            source_column: Some(3),
            section: Some("channel-binding".to_string()),
            section_title: Some("Channel Binding".to_string()),
            impl_refs: vec![TraceyCodeRef {
                file: "rust/vox-codegen/src/targets/typescript/mod.rs".to_string(),
                line: 128,
            }],
            verify_refs: vec![TraceyCodeRef {
                file: "spec/spec-tests/tests/cases/testbed.rs".to_string(),
                line: 3662,
            }],
            depends_refs: vec![TraceyCodeRef {
                file: "docs/content/guides/typescript.md".to_string(),
                line: 18,
            }],
            is_stale: true,
            stale_refs: vec![TraceyApiStaleRef {
                file: "swift/subject/Sources/subject-swift/Subject.swift".to_string(),
                line: 549,
                reference_id: tracey_rule_id("rpc.channel.direct-args", 1),
            }],
        }],
    }
}

fn sample_tracey_reverse_response() -> TraceyApiReverseData {
    TraceyApiReverseData {
        total_units: 7,
        covered_units: 5,
        files: vec![
            TraceyApiFileEntry {
                path: "rust/vox-codegen/src/targets/typescript/mod.rs".to_string(),
                total_units: 4,
                covered_units: 3,
            },
            TraceyApiFileEntry {
                path: "swift/subject/Sources/subject-swift/Subject.swift".to_string(),
                total_units: 3,
                covered_units: 2,
            },
        ],
    }
}

fn sample_tracey_file_request() -> TraceyFileRequest {
    TraceyFileRequest {
        spec: "vox".to_string(),
        impl_name: "rust".to_string(),
        path: "rust/vox-codegen/src/targets/typescript/mod.rs".to_string(),
    }
}

fn sample_tracey_file_response() -> TraceyApiFileData {
    TraceyApiFileData {
        path: "rust/vox-codegen/src/targets/typescript/mod.rs".to_string(),
        content: "fn emit_tracey_dashboard_bridge() {}\n".to_string(),
        html: "<pre><span>fn emit_tracey_dashboard_bridge() {}</span></pre>".to_string(),
        units: vec![TraceyApiCodeUnit {
            kind: "function".to_string(),
            name: Some("emit_tracey_dashboard_bridge".to_string()),
            start_line: 1,
            end_line: 1,
            rule_refs: vec![
                "rpc.channel.direct-args".to_string(),
                "encoding.struct".to_string(),
            ],
        }],
    }
}

fn sample_tracey_spec_content_response() -> TraceyApiSpecData {
    let direct = TraceyOutlineCoverage {
        impl_count: 1,
        verify_count: 1,
        total: 2,
    };
    let aggregate = TraceyOutlineCoverage {
        impl_count: 3,
        verify_count: 2,
        total: 4,
    };
    TraceyApiSpecData {
        name: "vox".to_string(),
        sections: vec![TraceySpecSection {
            source_file: "docs/content/spec/rpc.md".to_string(),
            html: "<h2 id=\"channel-binding\">Channel Binding</h2>".to_string(),
            weight: 20,
        }],
        outline: vec![TraceyOutlineEntry {
            title: "Channel Binding".to_string(),
            slug: "channel-binding".to_string(),
            level: 2,
            coverage: direct,
            aggregated: aggregate,
        }],
        head_injections: vec![
            "<script type=\"module\">mermaid.initialize({});</script>".to_string(),
        ],
    }
}

fn sample_tracey_search_results() -> Vec<TraceySearchResult> {
    vec![
        TraceySearchResult {
            kind: "rule".to_string(),
            id: "rpc.channel.direct-args".to_string(),
            line: 0,
            content: Some("Channels are direct service arguments.".to_string()),
            highlighted: Some("<mark>channel</mark> direct args".to_string()),
            score: 12.5,
        },
        TraceySearchResult {
            kind: "source".to_string(),
            id: "rust/vox-codegen/src/targets/typescript/mod.rs".to_string(),
            line: 128,
            content: Some("// r[impl rpc.channel.direct-args]".to_string()),
            highlighted: None,
            score: 7.25,
        },
    ]
}

fn sample_tracey_update_file_range_request() -> TraceyUpdateFileRangeRequest {
    TraceyUpdateFileRangeRequest {
        path: "docs/content/spec/rpc.md".to_string(),
        start: 120,
        end: 144,
        content: "Channels are direct service arguments.".to_string(),
        file_hash: "sha256:tracey-dashboard-ok".to_string(),
    }
}

fn sample_tracey_update_file_range_conflict_request() -> TraceyUpdateFileRangeRequest {
    TraceyUpdateFileRangeRequest {
        file_hash: "stale".to_string(),
        ..sample_tracey_update_file_range_request()
    }
}

fn sample_tracey_update_error() -> TraceyUpdateError {
    TraceyUpdateError {
        message: "file changed on disk".to_string(),
    }
}

fn sample_tracey_config_pattern_request() -> TraceyConfigPatternRequest {
    TraceyConfigPatternRequest {
        spec: Some("vox".to_string()),
        impl_name: Some("typescript".to_string()),
        pattern: "typescript/**/*.generated.ts".to_string(),
    }
}

fn sample_tracey_bad_config_pattern_request() -> TraceyConfigPatternRequest {
    TraceyConfigPatternRequest {
        pattern: "bad[glob".to_string(),
        ..sample_tracey_config_pattern_request()
    }
}

fn sample_tracey_validate_request() -> TraceyValidateRequest {
    TraceyValidateRequest {
        spec: Some("vox".to_string()),
        impl_name: Some("rust".to_string()),
    }
}

fn sample_tracey_validation_result() -> TraceyValidationResult {
    TraceyValidationResult {
        spec: "vox".to_string(),
        impl_name: "rust".to_string(),
        errors: vec![
            TraceyValidationError {
                code: TraceyValidationErrorCode::StaleRequirement,
                message: "reference points to an older rule version".to_string(),
                file: Some("rust/subject-rust/src/lib.rs".to_string()),
                line: Some(12),
                column: Some(9),
                related_rules: vec![tracey_rule_id("rpc.channel.direct-args", 2)],
                reference_rule_id: Some(tracey_rule_id("rpc.channel.direct-args", 1)),
                reference_text: Some("r[impl rpc.channel.direct-args]".to_string()),
            },
            TraceyValidationError {
                code: TraceyValidationErrorCode::UnknownRequirement,
                message: "unknown requirement".to_string(),
                file: None,
                line: None,
                column: None,
                related_rules: vec![],
                reference_rule_id: None,
                reference_text: Some("r[verify typo.rule]".to_string()),
            },
        ],
        warning_count: 1,
        error_count: 1,
    }
}

fn sample_tracey_lsp_content() -> String {
    "// r[impl rpc.channel.direct-args]\nfn main() {}\n".to_string()
}

fn sample_tracey_lsp_position_request() -> TraceyLspPositionRequest {
    TraceyLspPositionRequest {
        path: "src/lib.rs".to_string(),
        content: sample_tracey_lsp_content(),
        line: 0,
        character: 8,
    }
}

fn sample_tracey_lsp_references_request() -> TraceyLspReferencesRequest {
    TraceyLspReferencesRequest {
        path: "src/lib.rs".to_string(),
        content: sample_tracey_lsp_content(),
        line: 0,
        character: 8,
        include_declaration: true,
    }
}

fn sample_tracey_lsp_document_request() -> TraceyLspDocumentRequest {
    TraceyLspDocumentRequest {
        path: "src/lib.rs".to_string(),
        content: sample_tracey_lsp_content(),
    }
}

fn sample_tracey_lsp_inlay_hints_request() -> TraceyLspInlayHintsRequest {
    TraceyLspInlayHintsRequest {
        path: "src/lib.rs".to_string(),
        content: sample_tracey_lsp_content(),
        start_line: 0,
        end_line: 2,
    }
}

fn sample_tracey_lsp_rename_request() -> TraceyLspRenameRequest {
    TraceyLspRenameRequest {
        path: "src/lib.rs".to_string(),
        content: sample_tracey_lsp_content(),
        line: 0,
        character: 8,
        new_name: "rpc.channel.direct-args-renamed".to_string(),
    }
}

fn sample_tracey_lsp_locations() -> Vec<TraceyLspLocation> {
    vec![
        TraceyLspLocation {
            path: "docs/content/spec/rpc.md".to_string(),
            line: 211,
            character: 3,
        },
        TraceyLspLocation {
            path: "spec/spec-tests/tests/cases/testbed.rs".to_string(),
            line: 1450,
            character: 6,
        },
    ]
}

fn sample_tracey_hover_info() -> TraceyHoverInfo {
    TraceyHoverInfo {
        rule_id: tracey_rule_id("rpc.channel.direct-args", 1),
        raw: "Channels are direct service arguments.".to_string(),
        spec_name: "vox".to_string(),
        spec_url: Some("https://vixen.rs/vox/spec/rpc".to_string()),
        source_file: Some("docs/content/spec/rpc.md".to_string()),
        impl_count: 1,
        verify_count: 1,
        impl_refs: vec![TraceyCodeRef {
            file: "rust/vox-codegen/src/targets/swift/mod.rs".to_string(),
            line: 67,
        }],
        verify_refs: vec![TraceyCodeRef {
            file: "spec/spec-tests/tests/cases/testbed.rs".to_string(),
            line: 1450,
        }],
        range_start_line: 0,
        range_start_char: 3,
        range_end_line: 0,
        range_end_char: 36,
        version_diff: Some("Added direct argument wording.".to_string()),
    }
}

fn sample_tracey_lsp_completions() -> Vec<TraceyLspCompletionItem> {
    vec![
        TraceyLspCompletionItem {
            label: "impl".to_string(),
            kind: "verb".to_string(),
            detail: Some("implementation reference".to_string()),
            documentation: None,
            insert_text: Some("impl ".to_string()),
        },
        TraceyLspCompletionItem {
            label: "rpc.channel.direct-args".to_string(),
            kind: "rule".to_string(),
            detail: Some("vox".to_string()),
            documentation: Some("Channels are direct service arguments.".to_string()),
            insert_text: None,
        },
    ]
}

fn sample_tracey_lsp_workspace_diagnostics() -> Vec<TraceyLspFileDiagnostics> {
    vec![TraceyLspFileDiagnostics {
        path: "src/lib.rs".to_string(),
        diagnostics: vec![TraceyLspDiagnostic {
            severity: "warning".to_string(),
            code: "stale_requirement".to_string(),
            message: "reference points to an older rule version".to_string(),
            start_line: 7,
            start_char: 4,
            end_line: 7,
            end_char: 41,
        }],
    }]
}

fn sample_tracey_lsp_symbols() -> Vec<TraceyLspSymbol> {
    vec![
        TraceyLspSymbol {
            name: "rpc.channel.direct-args".to_string(),
            kind: "impl".to_string(),
            path: Some("src/lib.rs".to_string()),
            start_line: 0,
            start_char: 3,
            end_line: 0,
            end_char: 36,
        },
        TraceyLspSymbol {
            name: "rpc.channel.no-collections".to_string(),
            kind: "verify".to_string(),
            path: Some("spec/spec-tests/tests/cases/testbed.rs".to_string()),
            start_line: 1450,
            start_char: 6,
            end_line: 1450,
            end_char: 41,
        },
    ]
}

fn sample_tracey_lsp_semantic_tokens() -> Vec<TraceyLspSemanticToken> {
    vec![
        TraceyLspSemanticToken {
            line: 0,
            start_char: 3,
            length: 4,
            token_type: 0,
            modifiers: 0,
        },
        TraceyLspSemanticToken {
            line: 0,
            start_char: 8,
            length: 23,
            token_type: 1,
            modifiers: 2,
        },
    ]
}

fn sample_tracey_lsp_code_lens() -> Vec<TraceyLspCodeLens> {
    vec![TraceyLspCodeLens {
        line: 0,
        start_char: 3,
        end_char: 36,
        title: "1 impl, 1 verify".to_string(),
        command: "tracey.showRule".to_string(),
        arguments: vec!["rpc.channel.direct-args".to_string()],
    }]
}

fn sample_tracey_lsp_inlay_hints() -> Vec<TraceyLspInlayHint> {
    vec![TraceyLspInlayHint {
        line: 0,
        character: 36,
        label: "covered".to_string(),
    }]
}

fn sample_tracey_prepare_rename_result() -> TraceyPrepareRenameResult {
    TraceyPrepareRenameResult {
        start_line: 0,
        start_char: 8,
        end_line: 0,
        end_char: 31,
        placeholder: "rpc.channel.direct-args".to_string(),
    }
}

fn sample_tracey_lsp_text_edits() -> Vec<TraceyLspTextEdit> {
    vec![
        TraceyLspTextEdit {
            path: "src/lib.rs".to_string(),
            start_line: 0,
            start_char: 8,
            end_line: 0,
            end_char: 31,
            new_text: "rpc.channel.direct-args-renamed".to_string(),
        },
        TraceyLspTextEdit {
            path: "docs/content/spec/rpc.md".to_string(),
            start_line: 211,
            start_char: 3,
            end_line: 211,
            end_char: 26,
            new_text: "rpc.channel.direct-args-renamed".to_string(),
        },
    ]
}

fn sample_tracey_lsp_code_actions() -> Vec<TraceyLspCodeAction> {
    vec![TraceyLspCodeAction {
        title: "Open requirement".to_string(),
        kind: "quickfix".to_string(),
        command: "tracey.openRule".to_string(),
        arguments: vec!["rpc.channel.direct-args".to_string()],
        is_preferred: true,
    }]
}

fn sample_tracey_updates() -> Vec<TraceyDataUpdate> {
    vec![
        TraceyDataUpdate {
            version: 11,
            delta: None,
        },
        TraceyDataUpdate {
            version: 12,
            delta: Some(TraceyDeltaSummary {
                newly_covered: vec![TraceyCoverageChange {
                    rule_id: tracey_rule_id("rpc.channel.direct-args", 1),
                    file: "rust/vox-codegen/src/targets/swift/mod.rs".to_string(),
                    line: 67,
                }],
                newly_uncovered: vec![tracey_rule_id("rpc.channel.no-collections", 1)],
            }),
        },
    ]
}

fn sample_dibs_list_request() -> DibsListRequest {
    DibsListRequest {
        table: "products".to_string(),
        filters: vec![
            DibsFilter {
                field: "active".to_string(),
                op: DibsFilterOp::Eq,
                value: DibsValue::Bool(true),
                values: vec![],
            },
            DibsFilter {
                field: "id".to_string(),
                op: DibsFilterOp::In,
                value: DibsValue::Null,
                values: vec![DibsValue::I64(1), DibsValue::I64(2)],
            },
            DibsFilter {
                field: "metadata".to_string(),
                op: DibsFilterOp::JsonGetText,
                value: DibsValue::String("sku".to_string()),
                values: vec![],
            },
        ],
        sort: vec![DibsSort {
            field: "created_at".to_string(),
            dir: DibsSortDir::Desc,
        }],
        limit: Some(2),
        offset: Some(0),
        select: vec![
            "id".to_string(),
            "name".to_string(),
            "active".to_string(),
            "payload".to_string(),
        ],
    }
}

fn sample_dibs_list_response() -> DibsListResponse {
    DibsListResponse {
        rows: vec![
            DibsRow {
                fields: vec![
                    DibsRowField {
                        name: "id".to_string(),
                        value: DibsValue::I64(1),
                    },
                    DibsRowField {
                        name: "name".to_string(),
                        value: DibsValue::String("phon adapter".to_string()),
                    },
                    DibsRowField {
                        name: "active".to_string(),
                        value: DibsValue::Bool(true),
                    },
                    DibsRowField {
                        name: "score".to_string(),
                        value: DibsValue::F64(9.5),
                    },
                    DibsRowField {
                        name: "payload".to_string(),
                        value: DibsValue::Bytes(vec![0, 1, 2, 255]),
                    },
                ],
            },
            DibsRow {
                fields: vec![
                    DibsRowField {
                        name: "id".to_string(),
                        value: DibsValue::I64(2),
                    },
                    DibsRowField {
                        name: "name".to_string(),
                        value: DibsValue::String("vox bridge".to_string()),
                    },
                    DibsRowField {
                        name: "active".to_string(),
                        value: DibsValue::Bool(false),
                    },
                    DibsRowField {
                        name: "small".to_string(),
                        value: DibsValue::I16(7),
                    },
                    DibsRowField {
                        name: "count".to_string(),
                        value: DibsValue::I32(42),
                    },
                    DibsRowField {
                        name: "ratio".to_string(),
                        value: DibsValue::F32(0.5),
                    },
                    DibsRowField {
                        name: "deleted_at".to_string(),
                        value: DibsValue::Null,
                    },
                    DibsRowField {
                        name: "payload".to_string(),
                        value: DibsValue::Bytes(vec![]),
                    },
                ],
            },
        ],
        total: Some(2),
    }
}

fn sample_dibs_schema() -> DibsSchemaInfo {
    DibsSchemaInfo {
        tables: vec![DibsTableInfo {
            name: "products".to_string(),
            columns: vec![
                DibsColumnInfo {
                    name: "id".to_string(),
                    sql_type: "BIGINT".to_string(),
                    rust_type: Some("i64".to_string()),
                    nullable: false,
                    default: Some("generated by default as identity".to_string()),
                    primary_key: true,
                    unique: true,
                    auto_generated: true,
                    long: false,
                    label: false,
                    enum_variants: vec![],
                    doc: Some("Product primary key".to_string()),
                    lang: None,
                    icon: Some("hash".to_string()),
                    subtype: None,
                },
                DibsColumnInfo {
                    name: "name".to_string(),
                    sql_type: "TEXT".to_string(),
                    rust_type: Some("String".to_string()),
                    nullable: false,
                    default: None,
                    primary_key: false,
                    unique: false,
                    auto_generated: false,
                    long: false,
                    label: true,
                    enum_variants: vec![],
                    doc: Some("Display name".to_string()),
                    lang: None,
                    icon: Some("text".to_string()),
                    subtype: None,
                },
                DibsColumnInfo {
                    name: "status".to_string(),
                    sql_type: "TEXT".to_string(),
                    rust_type: Some("ProductStatus".to_string()),
                    nullable: false,
                    default: Some("'draft'".to_string()),
                    primary_key: false,
                    unique: false,
                    auto_generated: false,
                    long: false,
                    label: false,
                    enum_variants: vec!["draft".to_string(), "active".to_string()],
                    doc: None,
                    lang: None,
                    icon: Some("badge".to_string()),
                    subtype: None,
                },
                DibsColumnInfo {
                    name: "metadata".to_string(),
                    sql_type: "JSONB".to_string(),
                    rust_type: Some("Jsonb<facet_value::Value>".to_string()),
                    nullable: true,
                    default: None,
                    primary_key: false,
                    unique: false,
                    auto_generated: false,
                    long: true,
                    label: false,
                    enum_variants: vec![],
                    doc: Some("Structured product metadata".to_string()),
                    lang: Some("json".to_string()),
                    icon: Some("braces".to_string()),
                    subtype: None,
                },
                DibsColumnInfo {
                    name: "category_id".to_string(),
                    sql_type: "BIGINT".to_string(),
                    rust_type: Some("Option<i64>".to_string()),
                    nullable: true,
                    default: None,
                    primary_key: false,
                    unique: false,
                    auto_generated: false,
                    long: false,
                    label: false,
                    enum_variants: vec![],
                    doc: None,
                    lang: None,
                    icon: Some("link".to_string()),
                    subtype: None,
                },
            ],
            foreign_keys: vec![DibsForeignKeyInfo {
                columns: vec!["category_id".to_string()],
                references_table: "categories".to_string(),
                references_columns: vec!["id".to_string()],
            }],
            indices: vec![DibsIndexInfo {
                name: "products_active_created_at_idx".to_string(),
                columns: vec![
                    DibsIndexColumnInfo {
                        name: "active".to_string(),
                        order: "asc".to_string(),
                        nulls: "default".to_string(),
                    },
                    DibsIndexColumnInfo {
                        name: "created_at".to_string(),
                        order: "desc".to_string(),
                        nulls: "last".to_string(),
                    },
                ],
                unique: false,
                where_clause: Some("deleted_at IS NULL".to_string()),
            }],
            source_file: Some("examples/my-app-workspace/my-app-db/src/lib.rs".to_string()),
            source_line: Some(42),
            doc: Some("Products shown in the dynamic Dibs admin UI".to_string()),
            icon: Some("package".to_string()),
        }],
    }
}

fn sample_dibs_get_request() -> DibsGetRequest {
    DibsGetRequest {
        table: "products".to_string(),
        pk: DibsValue::I64(1),
    }
}

fn sample_dibs_create_request() -> DibsCreateRequest {
    DibsCreateRequest {
        table: "products".to_string(),
        data: DibsRow {
            fields: vec![
                DibsRowField {
                    name: "name".to_string(),
                    value: DibsValue::String("new adapter".to_string()),
                },
                DibsRowField {
                    name: "active".to_string(),
                    value: DibsValue::Bool(true),
                },
            ],
        },
    }
}

fn sample_dibs_create_response() -> DibsRow {
    DibsRow {
        fields: vec![
            DibsRowField {
                name: "id".to_string(),
                value: DibsValue::I64(3),
            },
            DibsRowField {
                name: "name".to_string(),
                value: DibsValue::String("new adapter".to_string()),
            },
            DibsRowField {
                name: "active".to_string(),
                value: DibsValue::Bool(true),
            },
        ],
    }
}

fn sample_dibs_update_request() -> DibsUpdateRequest {
    DibsUpdateRequest {
        table: "products".to_string(),
        pk: DibsValue::I64(1),
        data: DibsRow {
            fields: vec![
                DibsRowField {
                    name: "active".to_string(),
                    value: DibsValue::Bool(false),
                },
                DibsRowField {
                    name: "score".to_string(),
                    value: DibsValue::F64(10.0),
                },
            ],
        },
    }
}

fn sample_dibs_update_response() -> DibsRow {
    DibsRow {
        fields: vec![
            DibsRowField {
                name: "id".to_string(),
                value: DibsValue::I64(1),
            },
            DibsRowField {
                name: "name".to_string(),
                value: DibsValue::String("phon adapter".to_string()),
            },
            DibsRowField {
                name: "active".to_string(),
                value: DibsValue::Bool(false),
            },
            DibsRowField {
                name: "score".to_string(),
                value: DibsValue::F64(10.0),
            },
        ],
    }
}

fn sample_dibs_delete_request() -> DibsDeleteRequest {
    DibsDeleteRequest {
        table: "products".to_string(),
        pk: DibsValue::I64(2),
    }
}

fn sample_dibs_migration_status_request() -> DibsMigrationStatusRequest {
    DibsMigrationStatusRequest {
        database_url: "postgres://localhost/dibs_fixture".to_string(),
    }
}

fn sample_dibs_migration_status() -> Vec<DibsMigrationInfo> {
    vec![
        DibsMigrationInfo {
            version: "20240501000000".to_string(),
            name: "create_users".to_string(),
            applied: true,
            applied_at: Some("2024-05-01T00:00:00Z".to_string()),
            source_file: Some("migrations/20240501000000_create_users.rs".to_string()),
            source: Some("CREATE TABLE users (...)".to_string()),
        },
        DibsMigrationInfo {
            version: "20240601000000".to_string(),
            name: "create_products".to_string(),
            applied: false,
            applied_at: None,
            source_file: Some("migrations/20240601000000_create_products.rs".to_string()),
            source: Some("CREATE TABLE products (...)".to_string()),
        },
    ]
}

fn sample_dibs_migrate_request() -> DibsMigrateRequest {
    DibsMigrateRequest {
        database_url: "postgres://localhost/dibs_fixture".to_string(),
        migration: Some("20240601000000_create_products".to_string()),
    }
}

fn sample_dibs_logs() -> Vec<DibsMigrationLog> {
    let migration = "20240601000000_create_products".to_string();
    vec![
        DibsMigrationLog {
            level: DibsLogLevel::Info,
            message: "checking migrations".to_string(),
            migration: None,
        },
        DibsMigrationLog {
            level: DibsLogLevel::Debug,
            message: "running migration".to_string(),
            migration: Some(migration.clone()),
        },
        DibsMigrationLog {
            level: DibsLogLevel::Warn,
            message: "sample warning".to_string(),
            migration: Some(migration.clone()),
        },
        DibsMigrationLog {
            level: DibsLogLevel::Info,
            message: "migration complete".to_string(),
            migration: Some(migration),
        },
    ]
}

fn sample_dibs_migrate_result() -> DibsMigrateResult {
    DibsMigrateResult {
        total_defined: 3,
        already_applied: vec![DibsAppliedMigration {
            version: "20240501000000_create_users".to_string(),
            applied_at: "2024-05-01T00:00:00Z".to_string(),
        }],
        applied: vec![DibsRanMigration {
            version: "20240601000000_create_products".to_string(),
            duration_ms: 37,
        }],
        setup_ms: 5,
        total_time_ms: 42,
    }
}

// r[verify encoding.struct.recursive]
pub fn run_rpc_echo_tree(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let tree = sample_tree();
        let result = client
            .echo_tree(tree.clone())
            .await
            .map_err(|e| format!("echo_tree: {e:?}"))?;
        if result != tree {
            return Err(format!("echo_tree: expected {tree:?}, got {result:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.tuple]
// r[verify encoding.bytes]
pub fn run_rpc_echo_ecosystem_bridge(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let payload = sample_ecosystem_bridge_payload();
        let result = client
            .echo_ecosystem_bridge(payload.clone())
            .await
            .map_err(|e| format!("echo_ecosystem_bridge: {e:?}"))?;
        if result != payload {
            return Err(format!(
                "echo_ecosystem_bridge: expected {payload:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.tuple]
pub fn run_rpc_echo_dodeca_template_call(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let payload = sample_dodeca_template_call();
        let result = client
            .echo_dodeca_template_call(payload.clone())
            .await
            .map_err(|e| format!("echo_dodeca_template_call: {e:?}"))?;
        if result != payload {
            return Err(format!(
                "echo_dodeca_template_call: expected {payload:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.tuple]
pub fn run_rpc_dodeca_html_process(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let input = sample_dodeca_html_process_input();
        let expected = sample_dodeca_html_process_result();
        let result = client
            .dodeca_html_process(input)
            .await
            .map_err(|e| format!("dodeca_html_process: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_html_process: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.tuple]
pub fn run_rpc_dodeca_execute_code_samples(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let input = sample_dodeca_execute_samples_input();
        let expected = sample_dodeca_code_execution_result();
        let result = client
            .dodeca_execute_code_samples(input)
            .await
            .map_err(|e| format!("dodeca_execute_code_samples: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_execute_code_samples: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify type-system.dynamic]
// r[verify encoding.enum]
pub fn run_rpc_dodeca_load_data(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_load_data_result();
        let result = client
            .dodeca_load_data(sample_dodeca_data_content(), sample_dodeca_data_format())
            .await
            .map_err(|e| format!("dodeca_load_data: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_load_data: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify type-system.dynamic]
// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.enum]
pub fn run_rpc_dodeca_parse_and_render(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_parse_result();
        let result = client
            .dodeca_parse_and_render(
                sample_dodeca_markdown_source_path(),
                sample_dodeca_markdown_content(),
                true,
            )
            .await
            .map_err(|e| format!("dodeca_parse_and_render: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_parse_and_render: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.bytes]
// r[verify encoding.enum.payload]
pub fn run_rpc_echo_dodeca_image_processor_fixture(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let payload = sample_dodeca_image_processor_fixture();
        let result = client
            .echo_dodeca_image_processor_fixture(payload.clone())
            .await
            .map_err(|e| format!("echo_dodeca_image_processor_fixture: {e:?}"))?;
        if result != payload {
            return Err(format!(
                "echo_dodeca_image_processor_fixture: expected {payload:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.bytes]
// r[verify encoding.enum.payload]
pub fn run_rpc_echo_dodeca_search_indexer_fixture(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let payload = sample_dodeca_search_indexer_fixture();
        let result = client
            .echo_dodeca_search_indexer_fixture(payload.clone())
            .await
            .map_err(|e| format!("echo_dodeca_search_indexer_fixture: {e:?}"))?;
        if result != payload {
            return Err(format!(
                "echo_dodeca_search_indexer_fixture: expected {payload:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.enum.payload]
pub fn run_rpc_echo_dodeca_asset_processing_fixture(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let payload = sample_dodeca_asset_processing_fixture();
        let result = client
            .echo_dodeca_asset_processing_fixture(payload.clone())
            .await
            .map_err(|e| format!("echo_dodeca_asset_processing_fixture: {e:?}"))?;
        if result != payload {
            return Err(format!(
                "echo_dodeca_asset_processing_fixture: expected {payload:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
// r[verify encoding.enum.payload]

// r[verify encoding.struct]
// r[verify encoding.map]
// r[verify encoding.vec]
// r[verify encoding.bytes]
// r[verify encoding.enum.payload]
// r[verify type-system.rust-subset]
pub fn run_rpc_echo_dodeca_small_cell_services_fixture(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let payload = sample_dodeca_small_cell_services_fixture();
        let result = client
            .echo_dodeca_small_cell_services_fixture(payload.clone())
            .await
            .map_err(|e| format!("echo_dodeca_small_cell_services_fixture: {e:?}"))?;
        if result != payload {
            return Err(format!(
                "echo_dodeca_small_cell_services_fixture: expected {payload:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

pub fn run_rpc_echo_dodeca_devtools_event(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let payload = sample_dodeca_devtools_event();
        let result = client
            .echo_dodeca_devtools_event(payload.clone())
            .await
            .map_err(|e| format!("echo_dodeca_devtools_event: {e:?}"))?;
        if result != payload {
            return Err(format!(
                "echo_dodeca_devtools_event: expected {payload:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
// r[verify encoding.enum.payload]
pub fn run_rpc_dodeca_devtools_get_scope(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_scope_entries();
        let result = client
            .dodeca_devtools_get_scope(Some(vec!["page".to_string()]))
            .await
            .map_err(|e| format!("dodeca_devtools_get_scope: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_devtools_get_scope: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.enum.payload]
pub fn run_rpc_dodeca_devtools_eval(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_eval_result();
        let result = client
            .dodeca_devtools_eval("snap-devtools-42".to_string(), "page.title".to_string())
            .await
            .map_err(|e| format!("dodeca_devtools_eval: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_devtools_eval: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.payload]
pub fn run_rpc_dodeca_devtools_open_dead_link(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_open_source_result();
        let result = client
            .dodeca_devtools_open_dead_link("/guide/".to_string(), sample_dodeca_dead_link_target())
            .await
            .map_err(|e| format!("dodeca_devtools_open_dead_link: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_devtools_open_dead_link: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.payload]
pub fn run_rpc_dodeca_devtools_edit_load(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_edit_load();
        let result = client
            .dodeca_devtools_edit_load("editor-token".to_string(), "/guide/".to_string())
            .await
            .map_err(|e| format!("dodeca_devtools_edit_load: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_devtools_edit_load: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.enum.payload]
pub fn run_rpc_dodeca_devtools_edit_preview(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_edit_preview();
        let result = client
            .dodeca_devtools_edit_preview(
                "editor-token".to_string(),
                "content/guide.md".to_string(),
                "# Guide\n\nUpdated from browser.".to_string(),
            )
            .await
            .map_err(|e| format!("dodeca_devtools_edit_preview: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_devtools_edit_preview: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.enum.payload]
pub fn run_rpc_dodeca_devtools_edit_save(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_edit_save();
        let result = client
            .dodeca_devtools_edit_save("editor-token".to_string(), sample_dodeca_edit_save_req())
            .await
            .map_err(|e| format!("dodeca_devtools_edit_save: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_devtools_edit_save: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.enum.payload]
pub fn run_rpc_dodeca_devtools_edit_upload(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_edit_upload();
        let result = client
            .dodeca_devtools_edit_upload(
                "editor-token".to_string(),
                sample_dodeca_edit_upload_req(),
            )
            .await
            .map_err(|e| format!("dodeca_devtools_edit_upload: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_devtools_edit_upload: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.payload]
pub fn run_rpc_dodeca_devtools_edit_read(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_edit_read();
        let result = client
            .dodeca_devtools_edit_read(
                "editor-token".to_string(),
                "file:///workspace/content/guide.md".to_string(),
            )
            .await
            .map_err(|e| format!("dodeca_devtools_edit_read: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_devtools_edit_read: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.enum.payload]
pub fn run_rpc_dodeca_devtools_edit_list(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dodeca_edit_list();
        let result = client
            .dodeca_devtools_edit_list("editor-token".to_string())
            .await
            .map_err(|e| format!("dodeca_devtools_edit_list: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dodeca_devtools_edit_list: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct.recursive]
// r[verify encoding.enum.newtype-variants]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_echo_styx_value(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let value = sample_styx_value();
        let result = client
            .echo_styx_value(value.clone())
            .await
            .map_err(|e| format!("echo_styx_value: {e:?}"))?;
        if result != value {
            return Err(format!(
                "echo_styx_value: expected {value:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.enum.unit-variants]
pub fn run_rpc_styx_lsp_initialize(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_styx_lsp_initialize_result();
        let result = client
            .styx_lsp_initialize(sample_styx_lsp_initialize_params())
            .await
            .map_err(|e| format!("styx_lsp_initialize: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_lsp_initialize: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.enum.unit-variants]
pub fn run_rpc_styx_lsp_completions(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_styx_lsp_completions();
        let result = client
            .styx_lsp_completions(sample_styx_lsp_completion_params())
            .await
            .map_err(|e| format!("styx_lsp_completions: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_lsp_completions: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_styx_lsp_hover(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = Some(sample_styx_lsp_hover_result());
        let result = client
            .styx_lsp_hover(sample_styx_lsp_hover_params())
            .await
            .map_err(|e| format!("styx_lsp_hover: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_lsp_hover: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.enum.unit-variants]
pub fn run_rpc_styx_lsp_inlay_hints(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_styx_lsp_inlay_hints();
        let result = client
            .styx_lsp_inlay_hints(sample_styx_lsp_inlay_hint_params())
            .await
            .map_err(|e| format!("styx_lsp_inlay_hints: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_lsp_inlay_hints: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.enum.unit-variants]
pub fn run_rpc_styx_lsp_diagnostics(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_styx_lsp_diagnostics();
        let result = client
            .styx_lsp_diagnostics(sample_styx_lsp_diagnostic_params())
            .await
            .map_err(|e| format!("styx_lsp_diagnostics: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_lsp_diagnostics: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.enum.unit-variants]
pub fn run_rpc_styx_lsp_code_actions(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_styx_lsp_code_actions();
        let result = client
            .styx_lsp_code_actions(sample_styx_lsp_code_action_params())
            .await
            .map_err(|e| format!("styx_lsp_code_actions: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_lsp_code_actions: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_styx_lsp_definition(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_styx_lsp_locations();
        let result = client
            .styx_lsp_definition(sample_styx_lsp_definition_params())
            .await
            .map_err(|e| format!("styx_lsp_definition: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_lsp_definition: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

pub fn run_rpc_styx_lsp_shutdown(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        client
            .styx_lsp_shutdown()
            .await
            .map_err(|e| format!("styx_lsp_shutdown: {e:?}"))?;
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_styx_host_get_subtree(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = Some(sample_styx_value());
        let result = client
            .styx_host_get_subtree(sample_styx_lsp_get_subtree_params())
            .await
            .map_err(|e| format!("styx_host_get_subtree: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_host_get_subtree: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
pub fn run_rpc_styx_host_get_document(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = Some(sample_styx_value());
        let result = client
            .styx_host_get_document(sample_styx_lsp_get_document_params())
            .await
            .map_err(|e| format!("styx_host_get_document: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_host_get_document: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
pub fn run_rpc_styx_host_get_source(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = Some(sample_styx_lsp_source());
        let result = client
            .styx_host_get_source(sample_styx_lsp_get_source_params())
            .await
            .map_err(|e| format!("styx_host_get_source: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_host_get_source: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
pub fn run_rpc_styx_host_get_schema(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = Some(sample_styx_lsp_schema_info());
        let result = client
            .styx_host_get_schema(sample_styx_lsp_get_schema_params())
            .await
            .map_err(|e| format!("styx_host_get_schema: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_host_get_schema: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
pub fn run_rpc_styx_host_offset_to_position(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = Some(StyxLspPosition {
            line: 0,
            character: 16,
        });
        let result = client
            .styx_host_offset_to_position(sample_styx_lsp_offset_to_position_params())
            .await
            .map_err(|e| format!("styx_host_offset_to_position: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_host_offset_to_position: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
pub fn run_rpc_styx_host_position_to_offset(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = Some(16);
        let result = client
            .styx_host_position_to_offset(sample_styx_lsp_position_to_offset_params())
            .await
            .map_err(|e| format!("styx_host_position_to_offset: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "styx_host_position_to_offset: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_stax_flamegraph(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let params = sample_stax_view_params();
        let expected = sample_stax_flamegraph_update(&params);
        let result = client
            .stax_flamegraph(params)
            .await
            .map_err(|e| format!("stax_flamegraph: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "stax_flamegraph: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_echo_stax_flamegraph_update(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let params = sample_stax_view_params();
        let update = sample_stax_flamegraph_update(&params);
        let result = client
            .echo_stax_flamegraph_update(update.clone())
            .await
            .map_err(|e| format!("echo_stax_flamegraph_update: {e:?}"))?;
        if result != update {
            return Err(format!(
                "echo_stax_flamegraph_update: expected {update:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify rpc.channel.direct-args]
// r[verify schema.exchange.channels]
// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_stax_subscribe_flamegraph_updates(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let (update_tx, mut update_rx) = vox::channel::<StaxFlamegraphUpdate>();
        let recv = spec_tests::harness::spawn_loud(async move {
            let mut updates = Vec::new();
            while let Ok(Some(update)) = update_rx.recv().await {
                updates.push(update.get().clone());
            }
            updates
        });
        client
            .stax_subscribe_flamegraph_updates(update_tx)
            .await
            .map_err(|e| format!("stax_subscribe_flamegraph_updates: {e:?}"))?;
        let updates = recv
            .await
            .map_err(|e| format!("stax flamegraph updates recv: {e}"))?;
        let expected = sample_stax_flamegraph_updates();
        if updates != expected {
            return Err(format!(
                "stax_subscribe_flamegraph_updates: expected {expected:?}, got {updates:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

pub fn run_rpc_echo_stax_linux_broker_control(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let fixture = sample_stax_linux_broker_control_fixture();
        let result = client
            .echo_stax_linux_broker_control(fixture.clone())
            .await
            .map_err(|e| format!("echo_stax_linux_broker_control: {e:?}"))?;
        if result != fixture {
            return Err(format!(
                "echo_stax_linux_broker_control: expected {fixture:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify rpc.channel.direct-args]
// r[verify schema.exchange.channels]
// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify call.error.user]
pub fn run_rpc_stax_macos_record(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let (batch_tx, mut batch_rx) = vox::channel::<StaxMacKdBufBatch>();
        let expected_batches = sample_stax_macos_batches();
        let recv = spec_tests::harness::spawn_loud(async move {
            let mut batches = Vec::new();
            while let Ok(Some(batch)) = batch_rx.recv().await {
                batches.push(batch.get().clone());
            }
            batches
        });

        let result = client
            .stax_macos_record(sample_stax_macos_config(), batch_tx)
            .await
            .map_err(|e| format!("stax_macos_record: {e:?}"))?;
        let batches = recv.await.map_err(|e| format!("macos batches recv: {e}"))?;
        let expected_summary = sample_stax_macos_record_summary();
        if result != expected_summary {
            return Err(format!(
                "stax_macos_record: expected {expected_summary:?}, got {result:?}"
            ));
        }
        if batches != expected_batches {
            return Err(format!(
                "stax_macos_record batches: expected {expected_batches:?}, got {batches:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.struct-variants]
// r[verify encoding.bytes]
// r[verify encoding.vec]
pub fn run_rpc_echo_hotmeal_live_reload_event(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        for event in sample_hotmeal_live_reload_events() {
            let result = client
                .echo_hotmeal_live_reload_event(event.clone())
                .await
                .map_err(|e| format!("echo_hotmeal_live_reload_event: {e:?}"))?;
            if result != event {
                return Err(format!(
                    "echo_hotmeal_live_reload_event: expected {event:?}, got {result:?}"
                ));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.enum.struct-variants]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_echo_hotmeal_apply_patches_result(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let result_payload = sample_hotmeal_apply_patches_result();
        let result = client
            .echo_hotmeal_apply_patches_result(result_payload.clone())
            .await
            .map_err(|e| format!("echo_hotmeal_apply_patches_result: {e:?}"))?;
        if result != result_payload {
            return Err(format!(
                "echo_hotmeal_apply_patches_result: expected {result_payload:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.string]
pub fn run_rpc_hotmeal_live_reload_subscribe(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        client
            .hotmeal_live_reload_subscribe(sample_hotmeal_route())
            .await
            .map_err(|e| format!("hotmeal_live_reload_subscribe: {e:?}"))?;
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.struct-variants]
// r[verify encoding.bytes]
// r[verify encoding.vec]
pub fn run_rpc_hotmeal_live_reload_on_event(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        for event in sample_hotmeal_live_reload_events() {
            client
                .hotmeal_live_reload_on_event(event.clone())
                .await
                .map_err(|e| format!("hotmeal_live_reload_on_event: {e:?}"))?;
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.u64]
pub fn run_rpc_echo_helix_stream_metrics(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let metrics = sample_helix_stream_metrics();
        let result = client
            .echo_helix_stream_metrics(metrics.clone())
            .await
            .map_err(|e| format!("echo_helix_stream_metrics: {e:?}"))?;
        if result != metrics {
            return Err(format!(
                "echo_helix_stream_metrics: expected {metrics:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.enum.unit-variants]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_echo_helix_verify_evidence(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let digest = sample_helix_verify_evidence();
        let result = client
            .echo_helix_verify_evidence(digest.clone())
            .await
            .map_err(|e| format!("echo_helix_verify_evidence: {e:?}"))?;
        if result != digest {
            return Err(format!(
                "echo_helix_verify_evidence: expected {digest:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify rpc.channel.direct-args]
// r[verify schema.exchange.channels]
// r[verify encoding.struct]
pub fn run_rpc_helix_subscribe_pulses(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let (pulse_tx, mut pulse_rx) = vox::channel::<HelixPulseAvailable>();
        let recv = spec_tests::harness::spawn_loud(async move {
            let mut pulses = Vec::new();
            while let Ok(Some(pulse)) = pulse_rx.recv().await {
                pulses.push(*pulse.get());
            }
            pulses
        });
        client
            .helix_subscribe_pulses(pulse_tx)
            .await
            .map_err(|e| format!("helix_subscribe_pulses: {e:?}"))?;
        let pulses = recv.await.map_err(|e| format!("helix pulses recv: {e}"))?;
        let expected = sample_helix_pulses();
        if pulses != expected {
            return Err(format!(
                "helix_subscribe_pulses: expected {expected:?}, got {pulses:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.enum.payload]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.u64]
pub fn run_rpc_helix_pulse_bundle(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_helix_pulse_bundle();
        let result = client
            .helix_pulse_bundle(
                HelixSchedulerPulseId(102),
                sample_helix_pulse_bundle_fields(),
            )
            .await
            .map_err(|e| format!("helix_pulse_bundle: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "helix_pulse_bundle: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.enum.payload]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.u64]
// r[verify encoding.f32]
// r[verify type-system.dynamic]
pub fn run_rpc_helix_trace_service_surface(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_helix_trace_service_surface();
        let result = client
            .helix_trace_service_surface()
            .await
            .map_err(|e| format!("helix_trace_service_surface: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "helix_trace_service_surface: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.u64]
pub fn run_rpc_tracey_status(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_tracey_status_response();
        let result = client
            .tracey_status()
            .await
            .map_err(|e| format!("tracey_status: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "tracey_status: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.bool]
// r[verify encoding.u64]
pub fn run_rpc_tracey_core_control(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;

        let uncovered = client
            .tracey_uncovered(sample_tracey_query_request())
            .await
            .map_err(|e| format!("tracey_uncovered: {e:?}"))?;
        if uncovered != sample_tracey_uncovered_response() {
            return Err(format!("tracey_uncovered: got {uncovered:?}"));
        }

        let untested = client
            .tracey_untested(sample_tracey_untested_request())
            .await
            .map_err(|e| format!("tracey_untested: {e:?}"))?;
        if untested != sample_tracey_untested_response() {
            return Err(format!("tracey_untested: got {untested:?}"));
        }

        let stale = client
            .tracey_stale(sample_tracey_stale_request())
            .await
            .map_err(|e| format!("tracey_stale: {e:?}"))?;
        if stale != sample_tracey_stale_response() {
            return Err(format!("tracey_stale: got {stale:?}"));
        }

        let unmapped = client
            .tracey_unmapped(sample_tracey_unmapped_request())
            .await
            .map_err(|e| format!("tracey_unmapped: {e:?}"))?;
        if unmapped != sample_tracey_unmapped_response() {
            return Err(format!("tracey_unmapped: got {unmapped:?}"));
        }

        let config = client
            .tracey_config()
            .await
            .map_err(|e| format!("tracey_config: {e:?}"))?;
        if config != sample_tracey_api_config() {
            return Err(format!("tracey_config: got {config:?}"));
        }

        client
            .tracey_vfs_open("src/lib.rs".to_string(), sample_tracey_lsp_content())
            .await
            .map_err(|e| format!("tracey_vfs_open: {e:?}"))?;
        client
            .tracey_vfs_change(
                "src/lib.rs".to_string(),
                "// r[verify rpc.channel.direct-args]\n".to_string(),
            )
            .await
            .map_err(|e| format!("tracey_vfs_change: {e:?}"))?;
        client
            .tracey_vfs_close("src/lib.rs".to_string())
            .await
            .map_err(|e| format!("tracey_vfs_close: {e:?}"))?;

        let reload = client
            .tracey_reload()
            .await
            .map_err(|e| format!("tracey_reload: {e:?}"))?;
        if reload != sample_tracey_reload_response() {
            return Err(format!("tracey_reload: got {reload:?}"));
        }

        let version = client
            .tracey_version()
            .await
            .map_err(|e| format!("tracey_version: {e:?}"))?;
        if version != 13 {
            return Err(format!("tracey_version: got {version}"));
        }

        let health = client
            .tracey_health()
            .await
            .map_err(|e| format!("tracey_health: {e:?}"))?;
        if health != sample_tracey_health_response() {
            return Err(format!("tracey_health: got {health:?}"));
        }

        client
            .tracey_shutdown()
            .await
            .map_err(|e| format!("tracey_shutdown: {e:?}"))?;

        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_tracey_rule(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let result = client
            .tracey_rule(tracey_rule_id("rpc.channel.direct-args", 1))
            .await
            .map_err(|e| format!("tracey_rule known: {e:?}"))?;
        if result != Some(sample_tracey_rule_info()) {
            return Err(format!("tracey_rule known: got {result:?}"));
        }
        let missing = client
            .tracey_rule(tracey_rule_id("missing.rule", 1))
            .await
            .map_err(|e| format!("tracey_rule missing: {e:?}"))?;
        if missing.is_some() {
            return Err(format!(
                "tracey_rule missing: expected None, got {missing:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.f32]
// r[verify call.error.user]
pub fn run_rpc_tracey_dashboard(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;

        let forward = client
            .tracey_forward("vox".to_string(), "rust".to_string())
            .await
            .map_err(|e| format!("tracey_forward: {e:?}"))?;
        if forward != Some(sample_tracey_forward_response()) {
            return Err(format!("tracey_forward: got {forward:?}"));
        }
        let missing_forward = client
            .tracey_forward("missing".to_string(), "rust".to_string())
            .await
            .map_err(|e| format!("tracey_forward missing: {e:?}"))?;
        if missing_forward.is_some() {
            return Err(format!(
                "tracey_forward missing: expected None, got {missing_forward:?}"
            ));
        }

        let reverse = client
            .tracey_reverse("vox".to_string(), "rust".to_string())
            .await
            .map_err(|e| format!("tracey_reverse: {e:?}"))?;
        if reverse != Some(sample_tracey_reverse_response()) {
            return Err(format!("tracey_reverse: got {reverse:?}"));
        }

        let file = client
            .tracey_file(sample_tracey_file_request())
            .await
            .map_err(|e| format!("tracey_file: {e:?}"))?;
        if file != Some(sample_tracey_file_response()) {
            return Err(format!("tracey_file: got {file:?}"));
        }

        let spec_content = client
            .tracey_spec_content("vox".to_string(), "rust".to_string())
            .await
            .map_err(|e| format!("tracey_spec_content: {e:?}"))?;
        if spec_content != Some(sample_tracey_spec_content_response()) {
            return Err(format!("tracey_spec_content: got {spec_content:?}"));
        }

        let search = client
            .tracey_search("channel".to_string(), 10)
            .await
            .map_err(|e| format!("tracey_search: {e:?}"))?;
        if search != sample_tracey_search_results() {
            return Err(format!("tracey_search: got {search:?}"));
        }

        client
            .tracey_update_file_range(sample_tracey_update_file_range_request())
            .await
            .map_err(|e| format!("tracey_update_file_range ok: {e:?}"))?;
        match client
            .tracey_update_file_range(sample_tracey_update_file_range_conflict_request())
            .await
        {
            Err(VoxError::User(error)) if *error == sample_tracey_update_error() => {}
            Ok(()) => {
                return Err("tracey_update_file_range conflict: expected user error".to_string());
            }
            Err(other) => {
                return Err(format!(
                    "tracey_update_file_range conflict: expected user error, got {other:?}"
                ));
            }
        }

        client
            .tracey_config_add_exclude(sample_tracey_config_pattern_request())
            .await
            .map_err(|e| format!("tracey_config_add_exclude ok: {e:?}"))?;
        match client
            .tracey_config_add_exclude(sample_tracey_bad_config_pattern_request())
            .await
        {
            Err(VoxError::User(error)) if error.as_str() == "invalid pattern" => {}
            Ok(()) => {
                return Err(
                    "tracey_config_add_exclude bad pattern: expected user error".to_string()
                );
            }
            Err(other) => {
                return Err(format!(
                    "tracey_config_add_exclude bad pattern: expected user error, got {other:?}"
                ));
            }
        }
        client
            .tracey_config_add_include(sample_tracey_config_pattern_request())
            .await
            .map_err(|e| format!("tracey_config_add_include: {e:?}"))?;

        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.enum.unit-variants]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_rpc_tracey_validate(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_tracey_validation_result();
        let result = client
            .tracey_validate(sample_tracey_validate_request())
            .await
            .map_err(|e| format!("tracey_validate: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "tracey_validate: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.bool]
pub fn run_rpc_tracey_lsp_surface(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let test_file = client
            .tracey_is_test_file("spec/spec-tests/tests/cases/testbed.rs".to_string())
            .await
            .map_err(|e| format!("tracey_is_test_file true: {e:?}"))?;
        if !test_file {
            return Err("tracey_is_test_file: expected true for tests path".to_string());
        }
        let source_file = client
            .tracey_is_test_file("src/lib.rs".to_string())
            .await
            .map_err(|e| format!("tracey_is_test_file false: {e:?}"))?;
        if source_file {
            return Err("tracey_is_test_file: expected false for source path".to_string());
        }

        let hover = client
            .tracey_lsp_hover(sample_tracey_lsp_position_request())
            .await
            .map_err(|e| format!("tracey_lsp_hover: {e:?}"))?;
        if hover != Some(sample_tracey_hover_info()) {
            return Err(format!("tracey_lsp_hover: got {hover:?}"));
        }

        let definition = client
            .tracey_lsp_definition(sample_tracey_lsp_position_request())
            .await
            .map_err(|e| format!("tracey_lsp_definition: {e:?}"))?;
        if definition != sample_tracey_lsp_locations() {
            return Err(format!("tracey_lsp_definition: got {definition:?}"));
        }

        let implementation = client
            .tracey_lsp_implementation(sample_tracey_lsp_position_request())
            .await
            .map_err(|e| format!("tracey_lsp_implementation: {e:?}"))?;
        if implementation != sample_tracey_lsp_locations() {
            return Err(format!("tracey_lsp_implementation: got {implementation:?}"));
        }

        let references = client
            .tracey_lsp_references(sample_tracey_lsp_references_request())
            .await
            .map_err(|e| format!("tracey_lsp_references: {e:?}"))?;
        if references != sample_tracey_lsp_locations() {
            return Err(format!("tracey_lsp_references: got {references:?}"));
        }

        let completions = client
            .tracey_lsp_completions(sample_tracey_lsp_position_request())
            .await
            .map_err(|e| format!("tracey_lsp_completions: {e:?}"))?;
        if completions != sample_tracey_lsp_completions() {
            return Err(format!("tracey_lsp_completions: got {completions:?}"));
        }

        let document_symbols = client
            .tracey_lsp_document_symbols(sample_tracey_lsp_document_request())
            .await
            .map_err(|e| format!("tracey_lsp_document_symbols: {e:?}"))?;
        if document_symbols != sample_tracey_lsp_symbols() {
            return Err(format!(
                "tracey_lsp_document_symbols: got {document_symbols:?}"
            ));
        }

        let workspace_symbols = client
            .tracey_lsp_workspace_symbols("rpc.channel".to_string())
            .await
            .map_err(|e| format!("tracey_lsp_workspace_symbols: {e:?}"))?;
        if workspace_symbols != sample_tracey_lsp_symbols() {
            return Err(format!(
                "tracey_lsp_workspace_symbols: got {workspace_symbols:?}"
            ));
        }

        let semantic_tokens = client
            .tracey_lsp_semantic_tokens(sample_tracey_lsp_document_request())
            .await
            .map_err(|e| format!("tracey_lsp_semantic_tokens: {e:?}"))?;
        if semantic_tokens != sample_tracey_lsp_semantic_tokens() {
            return Err(format!(
                "tracey_lsp_semantic_tokens: got {semantic_tokens:?}"
            ));
        }

        let code_lens = client
            .tracey_lsp_code_lens(sample_tracey_lsp_document_request())
            .await
            .map_err(|e| format!("tracey_lsp_code_lens: {e:?}"))?;
        if code_lens != sample_tracey_lsp_code_lens() {
            return Err(format!("tracey_lsp_code_lens: got {code_lens:?}"));
        }

        let inlay_hints = client
            .tracey_lsp_inlay_hints(sample_tracey_lsp_inlay_hints_request())
            .await
            .map_err(|e| format!("tracey_lsp_inlay_hints: {e:?}"))?;
        if inlay_hints != sample_tracey_lsp_inlay_hints() {
            return Err(format!("tracey_lsp_inlay_hints: got {inlay_hints:?}"));
        }

        let prepare_rename = client
            .tracey_lsp_prepare_rename(sample_tracey_lsp_position_request())
            .await
            .map_err(|e| format!("tracey_lsp_prepare_rename: {e:?}"))?;
        if prepare_rename != Some(sample_tracey_prepare_rename_result()) {
            return Err(format!("tracey_lsp_prepare_rename: got {prepare_rename:?}"));
        }

        let text_edits = client
            .tracey_lsp_rename(sample_tracey_lsp_rename_request())
            .await
            .map_err(|e| format!("tracey_lsp_rename: {e:?}"))?;
        if text_edits != sample_tracey_lsp_text_edits() {
            return Err(format!("tracey_lsp_rename: got {text_edits:?}"));
        }

        let code_actions = client
            .tracey_lsp_code_actions(sample_tracey_lsp_position_request())
            .await
            .map_err(|e| format!("tracey_lsp_code_actions: {e:?}"))?;
        if code_actions != sample_tracey_lsp_code_actions() {
            return Err(format!("tracey_lsp_code_actions: got {code_actions:?}"));
        }

        let highlights = client
            .tracey_lsp_document_highlight(sample_tracey_lsp_position_request())
            .await
            .map_err(|e| format!("tracey_lsp_document_highlight: {e:?}"))?;
        if highlights != sample_tracey_lsp_locations() {
            return Err(format!("tracey_lsp_document_highlight: got {highlights:?}"));
        }

        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
pub fn run_rpc_tracey_lsp_workspace_diagnostics(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_tracey_lsp_workspace_diagnostics();
        let result = client
            .tracey_lsp_workspace_diagnostics()
            .await
            .map_err(|e| format!("tracey_lsp_workspace_diagnostics: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "tracey_lsp_workspace_diagnostics: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify rpc.channel.direct-args]
// r[verify schema.exchange.channels]
// r[verify encoding.struct]
pub fn run_rpc_tracey_subscribe_updates(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let (update_tx, mut update_rx) = vox::channel::<TraceyDataUpdate>();
        let recv = spec_tests::harness::spawn_loud(async move {
            let mut updates = Vec::new();
            while let Ok(Some(update)) = update_rx.recv().await {
                updates.push(update.get().clone());
            }
            updates
        });
        client
            .tracey_subscribe_updates(update_tx)
            .await
            .map_err(|e| format!("tracey_subscribe_updates: {e:?}"))?;
        let updates = recv
            .await
            .map_err(|e| format!("tracey updates recv: {e}"))?;
        let expected = sample_tracey_updates();
        if updates != expected {
            return Err(format!(
                "tracey_subscribe_updates: expected {expected:?}, got {updates:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
// r[verify encoding.enum.newtype-variants]
pub fn run_rpc_dibs_list(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let request = sample_dibs_list_request();
        let expected = sample_dibs_list_response();
        let result = client
            .dibs_list(request)
            .await
            .map_err(|e| format!("dibs_list: {e:?}"))?;
        if result != expected {
            return Err(format!("dibs_list: expected {expected:?}, got {result:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
// r[verify encoding.enum.newtype-variants]
pub fn run_rpc_dibs_schema(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dibs_schema();
        let result = client
            .dibs_schema()
            .await
            .map_err(|e| format!("dibs_schema: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dibs_schema: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.enum.newtype-variants]
pub fn run_rpc_dibs_get(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dibs_list_response().rows.into_iter().next();
        let result = client
            .dibs_get(sample_dibs_get_request())
            .await
            .map_err(|e| format!("dibs_get: {e:?}"))?;
        if result != expected {
            return Err(format!("dibs_get: expected {expected:?}, got {result:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.enum.newtype-variants]
pub fn run_rpc_dibs_create(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dibs_create_response();
        let result = client
            .dibs_create(sample_dibs_create_request())
            .await
            .map_err(|e| format!("dibs_create: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dibs_create: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.enum.newtype-variants]
pub fn run_rpc_dibs_update(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dibs_update_response();
        let result = client
            .dibs_update(sample_dibs_update_request())
            .await
            .map_err(|e| format!("dibs_update: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dibs_update: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.enum.newtype-variants]
pub fn run_rpc_dibs_delete(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let result = client
            .dibs_delete(sample_dibs_delete_request())
            .await
            .map_err(|e| format!("dibs_delete: {e:?}"))?;
        if result != 1 {
            return Err(format!("dibs_delete: expected 1, got {result}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
pub fn run_rpc_dibs_migration_status(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let expected = sample_dibs_migration_status();
        let result = client
            .dibs_migration_status(sample_dibs_migration_status_request())
            .await
            .map_err(|e| format!("dibs_migration_status: {e:?}"))?;
        if result != expected {
            return Err(format!(
                "dibs_migration_status: expected {expected:?}, got {result:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.type]
// r[verify channeling.lifecycle.immediate-data]
// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
// r[verify encoding.enum.unit-variants]
pub fn run_rpc_dibs_migrate(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let (log_tx, mut log_rx) = vox::channel::<DibsMigrationLog>();
        let expected_logs = sample_dibs_logs();
        let recv = spec_tests::harness::spawn_loud(async move {
            let mut logs = Vec::new();
            while let Ok(Some(log)) = log_rx.recv().await {
                logs.push(log.get().clone());
            }
            logs
        });

        let result = client
            .dibs_migrate(sample_dibs_migrate_request(), log_tx)
            .await
            .map_err(|e| format!("dibs_migrate: {e:?}"))?;
        let logs = recv.await.map_err(|e| format!("logs recv: {e}"))?;
        let expected_result = sample_dibs_migrate_result();
        if result != expected_result {
            return Err(format!(
                "dibs_migrate: expected {expected_result:?}, got {result:?}"
            ));
        }
        if logs != expected_logs {
            return Err(format!(
                "dibs_migrate logs: expected {expected_logs:?}, got {logs:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.unit-variants]
pub fn run_rpc_echo_status(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        for status in [spec_proto::Status::Active, spec_proto::Status::Inactive] {
            let result = client
                .echo_status_v1(status.clone())
                .await
                .map_err(|e| format!("echo_status_v1: {e:?}"))?;
            if result != status {
                return Err(format!(
                    "echo_status_v1: expected {status:?}, got {result:?}"
                ));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
pub fn run_rpc_echo_tag(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let tag = Tag {
            label: "important".to_string(),
            priority: 42,
            note: "do not delete".to_string(),
        };
        let result = client
            .echo_tag_v1(tag.clone())
            .await
            .map_err(|e| format!("echo_tag_v1: {e:?}"))?;
        if result != tag {
            return Err(format!("echo_tag_v1: expected {tag:?}, got {result:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.pipelining.allowed]
pub fn run_rpc_pipelining_10_concurrent(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let mut handles = Vec::new();
        for i in 0..10usize {
            let client = client.clone();
            let msg = format!("concurrent-{i}");
            handles.push(tokio::spawn(async move {
                client
                    .echo(msg.clone())
                    .await
                    .map_err(|e| format!("pipelining[{i}]: {e:?}"))
                    .and_then(|r| {
                        if r == msg {
                            Ok(())
                        } else {
                            Err(format!("pipelining[{i}]: expected {msg}, got {r}"))
                        }
                    })
            }));
        }
        for h in handles {
            h.await.map_err(|e| format!("pipelining join: {e}"))??;
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.flow-control]
pub fn run_rpc_channeling_large_stream(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let n: u32 = 100; // well above default initial_credit of 16
        let (tx, mut rx) = vox::channel::<i32>();
        let recv = spec_tests::harness::spawn_loud(async move {
            let mut received = Vec::new();
            while let Ok(Some(v)) = rx.recv().await {
                let v = v.get();
                received.push(*v);
            }
            received
        });
        client
            .generate_large(n, tx)
            .await
            .map_err(|e| format!("generate_large: {e:?}"))?;
        let received = recv.await.map_err(|e| format!("recv: {e}"))?;
        let expected: Vec<i32> = (0..n as i32).collect();
        if received != expected {
            return Err(format!(
                "generate_large: expected {n} sequential items, got {} items",
                received.len()
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.flow-control]
pub fn run_rpc_channeling_sum_large(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let n: i32 = 100;
        let (tx, rx) = vox::channel::<i32>();
        spec_tests::harness::spawn_loud(async move {
            for i in 0..n {
                tx.send(i).await.unwrap();
            }
            tx.close(Default::default()).await.unwrap();
        });
        let result = client
            .sum_large(rx)
            .await
            .map_err(|e| format!("sum_large: {e:?}"))?;
        let expected: i64 = (0..n as i64).sum();
        if result != expected {
            return Err(format!("sum_large: expected {expected}, got {result}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.type]
// r[verify channeling.lifecycle.immediate-data]
// r[verify encoding.bytes]
// r[verify encoding.vec]
pub fn run_rpc_dodeca_byte_tunnel(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let (inbound_tx, inbound_rx) = vox::channel::<Vec<u8>>();
        let (outbound_tx, mut outbound_rx) = vox::channel::<Vec<u8>>();
        let chunks = vec![vec![0, 1, 2, 3], vec![], vec![255, 254, 253]];
        let expected = chunks.clone();

        let recv = spec_tests::harness::spawn_loud(async move {
            let mut received = Vec::new();
            while let Ok(Some(chunk)) = outbound_rx.recv().await {
                received.push(chunk.get().clone());
            }
            received
        });

        let send = spec_tests::harness::spawn_loud(async move {
            for chunk in chunks {
                inbound_tx.send(chunk).await.unwrap();
            }
            inbound_tx.close(Default::default()).await.unwrap();
        });

        client
            .dodeca_byte_tunnel(inbound_rx, outbound_tx)
            .await
            .map_err(|e| format!("dodeca_byte_tunnel: {e:?}"))?;
        send.await.map_err(|e| format!("send: {e}"))?;
        let received = recv.await.map_err(|e| format!("recv: {e}"))?;
        if received != expected {
            return Err(format!(
                "dodeca_byte_tunnel: expected {expected:?}, got {received:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.type]
// r[verify channeling.lifecycle.immediate-data]
// r[verify encoding.string]
// r[verify encoding.vec]
pub fn run_rpc_dodeca_devtools_lsp(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;
        let (client_tx, client_rx) = vox::channel::<String>();
        let (server_tx, mut server_rx) = vox::channel::<String>();
        let chunks = vec![
            "Content-Length: 37\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1}".to_string(),
            "{\"method\":\"textDocument/didOpen\"}".to_string(),
        ];
        let expected = chunks
            .iter()
            .map(|chunk| format!("lsp:{chunk}"))
            .collect::<Vec<_>>();

        let recv = spec_tests::harness::spawn_loud(async move {
            let mut received = Vec::new();
            while let Ok(Some(chunk)) = server_rx.recv().await {
                received.push(chunk.get().clone());
            }
            received
        });

        let send = spec_tests::harness::spawn_loud(async move {
            for chunk in chunks {
                client_tx.send(chunk).await.unwrap();
            }
            client_tx.close(Default::default()).await.unwrap();
        });

        client
            .dodeca_devtools_lsp("editor-token".to_string(), client_rx, server_tx)
            .await
            .map_err(|e| format!("dodeca_devtools_lsp: {e:?}"))?;
        send.await.map_err(|e| format!("send: {e}"))?;
        let received = recv.await.map_err(|e| format!("recv: {e}"))?;
        if received != expected {
            return Err(format!(
                "dodeca_devtools_lsp: expected {expected:?}, got {received:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// ============================================================================
// Additional subject→harness: full type + error coverage
// ============================================================================

// r[verify call.initiate]
pub fn run_subject_calls_reverse(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "reverse");
}

// r[verify call.error.user]
pub fn run_subject_calls_divide_success(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "divide_success");
}

// r[verify call.error.user]
pub fn run_subject_calls_divide_zero(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "divide_zero");
}

// r[verify call.error.user]
pub fn run_subject_calls_divide_overflow(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "divide_overflow");
}

// r[verify call.error.user]
pub fn run_subject_calls_lookup_found(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "lookup_found");
}

// r[verify call.error.user]
pub fn run_subject_calls_lookup_found_no_email(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "lookup_found_no_email");
}

// r[verify call.error.user]
pub fn run_subject_calls_lookup_not_found(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "lookup_not_found");
}

// r[verify call.error.user]
pub fn run_subject_calls_lookup_access_denied(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "lookup_access_denied");
}

// r[verify encoding.struct]
pub fn run_subject_calls_echo_point(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_point");
}

// r[verify encoding.struct]
pub fn run_subject_calls_create_person(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "create_person");
}

// r[verify encoding.struct.nested]
pub fn run_subject_calls_rectangle_area(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "rectangle_area");
}

// r[verify encoding.option.return]
pub fn run_subject_calls_parse_color(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "parse_color");
}

// r[verify encoding.vec]
pub fn run_subject_calls_get_points(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "get_points");
}

// r[verify encoding.tuple]
pub fn run_subject_calls_swap_pair(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "swap_pair");
}

// r[verify encoding.bytes]
pub fn run_subject_calls_echo_bytes(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_bytes");
}

// r[verify encoding.bool]
pub fn run_subject_calls_echo_bool(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_bool");
}

// r[verify encoding.u64]
pub fn run_subject_calls_echo_u64(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_u64");
}

// r[verify encoding.option]
pub fn run_subject_calls_echo_option_string(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_option_string");
}

// r[verify encoding.struct.multi-arg]
pub fn run_subject_calls_describe_point(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "describe_point");
}

// r[verify encoding.enum.unit-variants]
pub fn run_subject_calls_all_colors(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "all_colors");
}

// r[verify encoding.enum.struct-variants]
pub fn run_subject_calls_echo_shape(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_shape");
}

// r[verify encoding.struct.recursive]
pub fn run_subject_calls_echo_tree(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_tree");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.tuple]
// r[verify encoding.bytes]
pub fn run_subject_calls_echo_ecosystem_bridge(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_ecosystem_bridge");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.tuple]
pub fn run_subject_calls_echo_dodeca_template_call(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_dodeca_template_call");
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.tuple]
pub fn run_subject_calls_dodeca_html_process(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_html_process");
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.tuple]
pub fn run_subject_calls_dodeca_execute_code_samples(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_execute_code_samples");
}

// r[verify type-system.dynamic]
// r[verify encoding.enum]
pub fn run_subject_calls_dodeca_load_data(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_load_data");
}

// r[verify type-system.dynamic]
// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.enum]
pub fn run_subject_calls_dodeca_parse_and_render(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_parse_and_render");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.bytes]
// r[verify encoding.enum.payload]
pub fn run_subject_calls_echo_dodeca_image_processor_fixture(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_dodeca_image_processor_fixture");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.bytes]
// r[verify encoding.enum.payload]
pub fn run_subject_calls_echo_dodeca_search_indexer_fixture(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_dodeca_search_indexer_fixture");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.enum.payload]
pub fn run_subject_calls_echo_dodeca_asset_processing_fixture(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_dodeca_asset_processing_fixture");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
// r[verify encoding.enum.payload]

// r[verify encoding.struct]
// r[verify encoding.map]
// r[verify encoding.vec]
// r[verify encoding.bytes]
// r[verify encoding.enum.payload]
// r[verify type-system.rust-subset]
pub fn run_subject_calls_echo_dodeca_small_cell_services_fixture(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_dodeca_small_cell_services_fixture");
}

pub fn run_subject_calls_echo_dodeca_devtools_event(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_dodeca_devtools_event");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
// r[verify encoding.enum.payload]
pub fn run_subject_calls_dodeca_devtools_get_scope(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_devtools_get_scope");
}

// r[verify encoding.struct]
// r[verify encoding.enum.payload]
pub fn run_subject_calls_dodeca_devtools_eval(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_devtools_eval");
}

// r[verify encoding.enum.payload]
pub fn run_subject_calls_dodeca_devtools_open_dead_link(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_devtools_open_dead_link");
}

// r[verify encoding.enum.payload]
pub fn run_subject_calls_dodeca_devtools_edit_load(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_devtools_edit_load");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.enum.payload]
pub fn run_subject_calls_dodeca_devtools_edit_preview(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_devtools_edit_preview");
}

// r[verify encoding.struct]
// r[verify encoding.enum.payload]
pub fn run_subject_calls_dodeca_devtools_edit_save(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_devtools_edit_save");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.enum.payload]
pub fn run_subject_calls_dodeca_devtools_edit_upload(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_devtools_edit_upload");
}

// r[verify encoding.enum.payload]
pub fn run_subject_calls_dodeca_devtools_edit_read(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_devtools_edit_read");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.enum.payload]
pub fn run_subject_calls_dodeca_devtools_edit_list(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_devtools_edit_list");
}

// r[verify encoding.struct.recursive]
// r[verify encoding.enum.newtype-variants]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_echo_styx_value(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_styx_value");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.enum.unit-variants]
pub fn run_subject_calls_styx_lsp_initialize(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_lsp_initialize");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.enum.unit-variants]
pub fn run_subject_calls_styx_lsp_completions(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_lsp_completions");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_styx_lsp_hover(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_lsp_hover");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.enum.unit-variants]
pub fn run_subject_calls_styx_lsp_inlay_hints(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_lsp_inlay_hints");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.enum.unit-variants]
pub fn run_subject_calls_styx_lsp_diagnostics(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_lsp_diagnostics");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.enum.unit-variants]
pub fn run_subject_calls_styx_lsp_code_actions(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_lsp_code_actions");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_styx_lsp_definition(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_lsp_definition");
}

pub fn run_subject_calls_styx_lsp_shutdown(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_lsp_shutdown");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_styx_host_get_subtree(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_host_get_subtree");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
pub fn run_subject_calls_styx_host_get_document(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_host_get_document");
}

// r[verify encoding.struct]
// r[verify encoding.option]
pub fn run_subject_calls_styx_host_get_source(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_host_get_source");
}

// r[verify encoding.struct]
// r[verify encoding.option]
pub fn run_subject_calls_styx_host_get_schema(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_host_get_schema");
}

// r[verify encoding.struct]
// r[verify encoding.option]
pub fn run_subject_calls_styx_host_offset_to_position(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_host_offset_to_position");
}

// r[verify encoding.struct]
// r[verify encoding.option]
pub fn run_subject_calls_styx_host_position_to_offset(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "styx_host_position_to_offset");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_stax_flamegraph(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "stax_flamegraph");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_echo_stax_flamegraph_update(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_stax_flamegraph_update");
}

// r[verify rpc.channel.direct-args]
// r[verify schema.exchange.channels]
// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_stax_subscribe_flamegraph_updates(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "stax_subscribe_flamegraph_updates");
}

pub fn run_subject_calls_echo_stax_linux_broker_control(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_stax_linux_broker_control");
}

// r[verify rpc.channel.direct-args]
// r[verify schema.exchange.channels]
// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify call.error.user]
pub fn run_subject_calls_stax_macos_record(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "stax_macos_record");
}

// r[verify encoding.enum.struct-variants]
// r[verify encoding.bytes]
// r[verify encoding.vec]
pub fn run_subject_calls_echo_hotmeal_live_reload_event(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_hotmeal_live_reload_event");
}

// r[verify encoding.struct]
// r[verify encoding.struct.recursive]
// r[verify encoding.enum.struct-variants]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_echo_hotmeal_apply_patches_result(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_hotmeal_apply_patches_result");
}

// r[verify encoding.string]
pub fn run_subject_calls_hotmeal_live_reload_subscribe(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "hotmeal_live_reload_subscribe");
}

// r[verify encoding.enum.struct-variants]
// r[verify encoding.bytes]
// r[verify encoding.vec]
pub fn run_subject_calls_hotmeal_live_reload_on_event(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "hotmeal_live_reload_on_event");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.u64]
pub fn run_subject_calls_echo_helix_stream_metrics(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_helix_stream_metrics");
}

// r[verify encoding.struct]
// r[verify encoding.enum.unit-variants]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_echo_helix_verify_evidence(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_helix_verify_evidence");
}

// r[verify rpc.channel.direct-args]
// r[verify schema.exchange.channels]
// r[verify encoding.struct]
pub fn run_subject_calls_helix_subscribe_pulses(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "helix_subscribe_pulses");
}

// r[verify encoding.struct]
// r[verify encoding.enum.payload]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.u64]
pub fn run_subject_calls_helix_pulse_bundle(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "helix_pulse_bundle");
}

// r[verify encoding.struct]
// r[verify encoding.enum.payload]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.u64]
// r[verify encoding.f32]
// r[verify type-system.dynamic]
pub fn run_subject_calls_helix_trace_service_surface(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "helix_trace_service_surface");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.u64]
pub fn run_subject_calls_tracey_status(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "tracey_status");
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.bool]
// r[verify encoding.u64]
pub fn run_subject_calls_tracey_core_control(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "tracey_core_control");
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_tracey_rule(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "tracey_rule");
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.f32]
// r[verify call.error.user]
pub fn run_subject_calls_tracey_dashboard(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "tracey_dashboard");
}

// r[verify encoding.struct]
// r[verify encoding.enum.unit-variants]
// r[verify encoding.option]
// r[verify encoding.vec]
pub fn run_subject_calls_tracey_validate(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "tracey_validate");
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.vec]
// r[verify encoding.bool]
pub fn run_subject_calls_tracey_lsp_surface(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "tracey_lsp_surface");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
pub fn run_subject_calls_tracey_lsp_workspace_diagnostics(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "tracey_lsp_workspace_diagnostics");
}

// r[verify rpc.channel.direct-args]
// r[verify schema.exchange.channels]
// r[verify encoding.struct]
pub fn run_subject_calls_tracey_subscribe_updates(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "tracey_subscribe_updates");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
// r[verify encoding.enum.newtype-variants]
pub fn run_subject_calls_dibs_list(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dibs_list");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
// r[verify encoding.enum.newtype-variants]
pub fn run_subject_calls_dibs_schema(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dibs_schema");
}

// r[verify encoding.struct]
// r[verify encoding.option]
// r[verify encoding.enum.newtype-variants]
pub fn run_subject_calls_dibs_get(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dibs_get");
}

// r[verify encoding.struct]
// r[verify encoding.enum.newtype-variants]
pub fn run_subject_calls_dibs_create(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dibs_create");
}

// r[verify encoding.struct]
// r[verify encoding.enum.newtype-variants]
pub fn run_subject_calls_dibs_update(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dibs_update");
}

// r[verify encoding.struct]
// r[verify encoding.enum.newtype-variants]
pub fn run_subject_calls_dibs_delete(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dibs_delete");
}

// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
pub fn run_subject_calls_dibs_migration_status(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dibs_migration_status");
}

// r[verify channeling.type]
// r[verify channeling.lifecycle.immediate-data]
// r[verify encoding.struct]
// r[verify encoding.vec]
// r[verify encoding.option]
// r[verify encoding.enum.unit-variants]
pub fn run_subject_calls_dibs_migrate(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dibs_migrate");
}

// r[verify call.pipelining.allowed]
pub fn run_subject_calls_pipelining(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "pipelining");
}

// r[verify channeling.flow-control]
pub fn run_subject_calls_sum_large(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "sum_large");
}

// r[verify channeling.flow-control]
pub fn run_subject_calls_generate_large(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "generate_large");
}

// r[verify channeling.caller-pov]
pub fn run_subject_calls_sum_client_to_server(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "sum_client_to_server");
}

// r[verify channeling.type]
// r[verify channeling.lifecycle.immediate-data]
pub fn run_subject_calls_transform_bidi(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "transform_bidi");
}

// r[verify channeling.type]
// r[verify channeling.lifecycle.immediate-data]
// r[verify encoding.bytes]
// r[verify encoding.vec]
pub fn run_subject_calls_dodeca_byte_tunnel(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_byte_tunnel");
}

// r[verify channeling.type]
// r[verify channeling.lifecycle.immediate-data]
// r[verify encoding.string]
// r[verify encoding.vec]
pub fn run_subject_calls_dodeca_devtools_lsp(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "dodeca_devtools_lsp");
}

// Cross-language test functions are generated inline by the xtask matrix
// generator directly from the scenario name list — no wrapper functions needed.
// See xtask/src/main.rs `cross_lang_scenarios` for the single source of truth.
