use std::io::{BufReader, Cursor};

use facet::Facet;
use facet_lsp::framing::{NotificationMessage, RequestMessage, RpcId, frame, read_frame};
use facet_lsp::types::{
    DidOpenTextDocumentParams, Hover, InitializeParams, InitializeResult, Location,
    PublishDiagnosticsParams, ReferenceContext, ReferenceParams, RenameParams, SemanticTokens,
    SemanticTokensParams, TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams,
    WorkspaceEdit,
};
use facet_testhelpers::test;
use vix_lsp::{RustLanguageQueries, Server};

fn sample_uri(name: &str) -> String {
    format!("file:///workspace/{name}")
}

fn merge_demand() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../playgrounds/snark/src/bundled/vix/samples/merge-demand.vix"
    ))
    .expect("read sample")
}

fn types_vix() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../playgrounds/snark/src/bundled/vix/samples/types.vix"
    ))
    .expect("read sample")
}

fn word_position(src: &str, needle: &str, nth: usize) -> facet_lsp::types::Position {
    let mut seen = 0;
    for (line_idx, line) in src.lines().enumerate() {
        for (byte_idx, _) in line.match_indices(needle) {
            let before = line.as_bytes().get(byte_idx.wrapping_sub(1)).copied();
            let after = line.as_bytes().get(byte_idx + needle.len()).copied();
            let is_word = |b: Option<u8>| b.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
            if is_word(before) || is_word(after) {
                continue;
            }
            if seen == nth {
                let character = line[..byte_idx].chars().map(char::len_utf16).sum::<usize>();
                return facet_lsp::types::Position {
                    line: u32::try_from(line_idx).unwrap(),
                    character: u32::try_from(character).unwrap(),
                };
            }
            seen += 1;
        }
    }
    panic!("missing {needle} occurrence {nth}");
}

#[derive(Debug, Facet)]
struct Response {
    jsonrpc: String,
    id: RpcId,
    result: Option<facet_lsp::RawJson<'static>>,
}

fn run_exchange(messages: Vec<Vec<u8>>) -> Vec<String> {
    let mut input = Vec::new();
    for message in messages {
        input.extend(message);
    }
    let mut output = Vec::new();
    let mut server = Server::new(RustLanguageQueries::new());
    server
        .run(&mut BufReader::new(Cursor::new(input)), &mut output)
        .expect("server run");
    let mut reader = BufReader::new(Cursor::new(output));
    let mut out = Vec::new();
    while let Some(body) = read_frame(&mut reader).expect("read output") {
        out.push(String::from_utf8(body).expect("utf8"));
    }
    out
}

fn response_result<T: Facet<'static>>(body: &str) -> T {
    let response: Response = facet_json::from_str(body).expect("response");
    let result = response.result.expect("result");
    facet_json::from_str(result.as_str()).expect("typed result")
}

#[test]
fn initialize_handshake_uses_real_framed_json_rpc() {
    let out = run_exchange(vec![
        frame(&RequestMessage::new(
            1,
            "initialize",
            InitializeParams {
                process_id: Some(42),
                root_uri: Some("file:///workspace".to_owned()),
            },
        ))
        .expect("frame"),
        frame(&NotificationMessage::new(
            "initialized",
            facet_lsp::types::InitializedParams::default(),
        ))
        .expect("frame"),
    ]);
    let result: InitializeResult = response_result(&out[0]);
    assert_eq!(result.server_info.name, "vix-lsp");
    assert!(result.capabilities.definition_provider);
    assert_eq!(result.capabilities.text_document_sync, 1);
}

#[test]
fn definition_references_hover_and_rename_flow_over_sample() {
    let src = merge_demand();
    let uri = sample_uri("merge-demand.vix");
    let object_ref = word_position(&src, "object", 1);
    let object_def = word_position(&src, "object", 0);
    let frames = vec![
        open_frame(&uri, &src),
        frame(&RequestMessage::new(
            2,
            "textDocument/definition",
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: object_ref,
            },
        ))
        .expect("frame"),
        frame(&RequestMessage::new(
            3,
            "textDocument/references",
            ReferenceParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: object_ref,
                context: ReferenceContext {
                    include_declaration: true,
                },
            },
        ))
        .expect("frame"),
        frame(&RequestMessage::new(
            4,
            "textDocument/hover",
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: object_def,
            },
        ))
        .expect("frame"),
        frame(&RequestMessage::new(
            5,
            "textDocument/rename",
            RenameParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: object_def,
                new_name: "compile_object".to_owned(),
            },
        ))
        .expect("frame"),
    ];
    let out = run_exchange(frames);
    let definition: Option<Location> = response_result(&out[1]);
    assert_eq!(definition.expect("definition").range.start, object_def);
    let refs: Vec<Location> = response_result(&out[2]);
    assert_eq!(refs.len(), 4);
    let hover: Option<Hover> = response_result(&out[3]);
    assert!(hover.expect("hover").contents.value.contains("Fn object"));
    let edit: WorkspaceEdit = response_result(&out[4]);
    assert_eq!(edit.changes[&uri].len(), 4);
    assert_eq!(edit.changes[&uri][0].new_text, "compile_object");
}

#[test]
fn semantic_tokens_run_is_pinned_for_types_sample() {
    let src = types_vix();
    let uri = sample_uri("types.vix");
    let out = run_exchange(vec![
        open_frame(&uri, &src),
        frame(&RequestMessage::new(
            6,
            "textDocument/semanticTokens/full",
            SemanticTokensParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
            },
        ))
        .expect("frame"),
    ]);
    let tokens: SemanticTokens = response_result(&out[1]);
    assert_eq!(
        &tokens.data[..15],
        &[0, 0, 52, 10, 0, 1, 0, 2, 10, 0, 1, 0, 73, 10, 0]
    );
    assert!(tokens.data.len() > 100);
}

#[test]
fn broken_snippet_publishes_diagnostic() {
    let uri = sample_uri("broken.vix");
    let out = run_exchange(vec![open_frame(&uri, "fn broken(")]);
    #[derive(Debug, Facet)]
    struct Notification {
        jsonrpc: String,
        method: String,
        params: facet_lsp::RawJson<'static>,
    }
    let notification: Notification = facet_json::from_str(&out[0]).expect("notification");
    assert_eq!(notification.method, "textDocument/publishDiagnostics");
    let params: PublishDiagnosticsParams =
        facet_json::from_str(notification.params.as_str()).expect("params");
    assert_eq!(params.uri, uri);
    assert!(!params.diagnostics.is_empty());
    assert!(params.diagnostics[0].message.contains("parse failed"));
}

fn open_frame(uri: &str, text: &str) -> Vec<u8> {
    frame(&NotificationMessage::new(
        "textDocument/didOpen",
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.to_owned(),
                language_id: "vix".to_owned(),
                version: 1,
                text: text.to_owned(),
            },
        },
    ))
    .expect("frame")
}
