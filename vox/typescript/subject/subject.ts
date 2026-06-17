// Node subject for the vox compliance suite.
//
// This demonstrates the minimal code needed to implement a vox service
// using the @vox/tcp transport library.

import type {
  TestbedHandler,
  Point,
  Person,
  Rectangle,
  Color,
  Shape,
  Canvas,
  Message,
  MathError,
  LookupError,
  Profile,
  Record,
  Status,
  Tag,
  Measurement,
  Config,
  TaggedPoint,
  GnarlyPayload,
  Tree,
  EcosystemBridgePayload,
  DodecaAssetProcessingFixture,
  DodecaBuildMetadata,
  DodecaCodeExecutionConfig,
  DodecaCodeExecutionMetadata,
  DodecaCodeExecutionResult,
  DodecaCodeSample,
  DodecaDataFormat,
  DodecaDeadLinkTarget,
  DodecaDecodedImage,
  DodecaDependencySpec,
  DodecaDevtoolsEvent,
  DodecaEditEntry,
  DodecaEditList,
  DodecaEditLoad,
  DodecaEditPreview,
  DodecaEditRead,
  DodecaEditSave,
  DodecaEditSaveReq,
  DodecaEditUpload,
  DodecaEditUploadReq,
  DodecaErrorInfo,
  DodecaEvalResult,
  DodecaExecuteSamplesInput,
  DodecaExecutionResult,
  DodecaImageProcessorFixture,
  DodecaLoadDataResult,
  DodecaOpenSourceResult,
  DodecaParseResult,
  DodecaHtmlProcessInput,
  DodecaHtmlProcessResult,
  DodecaResponsiveImageInfo,
  DodecaRustConfig,
  DodecaScopeEntry,
  DodecaScopeValue,
  DodecaSidLine,
  DodecaSearchIndexerFixture,
  DodecaSmallCellServicesFixture,
  DodecaSourceLine,
  DodecaSourceSnippet,
  DodecaTemplateCall,
  DibsCreateRequest,
  DibsDeleteRequest,
  DibsError,
  DibsGetRequest,
  DibsListRequest,
  DibsListResponse,
  DibsLogLevel,
  DibsMigrateRequest,
  DibsMigrateResult,
  DibsMigrationInfo,
  DibsMigrationLog,
  DibsMigrationStatusRequest,
  DibsRow,
  DibsRowField,
  DibsSchemaInfo,
  DibsUpdateRequest,
  DibsValue,
  StyxEntry,
  StyxObject,
  StyxPayload,
  StyxScalar,
  StyxScalarKind,
  StyxSequence,
  StyxSpan,
  StyxTag,
  StyxValue,
  StyxLspCapability,
  StyxLspCodeAction,
  StyxLspCodeActionParams,
  StyxLspCompletionItem,
  StyxLspCompletionParams,
  StyxLspCursor,
  StyxLspDefinitionParams,
  StyxLspDiagnostic,
  StyxLspDiagnosticParams,
  StyxLspGetDocumentParams,
  StyxLspGetSchemaParams,
  StyxLspGetSourceParams,
  StyxLspGetSubtreeParams,
  StyxLspHoverParams,
  StyxLspHoverResult,
  StyxLspInlayHint,
  StyxLspInlayHintParams,
  StyxLspInitializeParams,
  StyxLspInitializeResult,
  StyxLspLocation,
  StyxLspOffsetToPositionParams,
  StyxLspPosition,
  StyxLspPositionToOffsetParams,
  StyxLspRange,
  StyxLspSchemaInfo,
  StaxFlameNode,
  StaxFlamegraphUpdate,
  StaxLinuxBrokerControlFixture,
  StaxLinuxDaemonStatus,
  StaxLinuxPerfSessionConfig,
  StaxLinuxPerfSessionError,
  StaxLinuxWakingFieldOffsets,
  StaxMacKdBuf,
  StaxMacKdBufBatch,
  StaxMacRecordError,
  StaxMacRecordSummary,
  StaxMacSessionConfig,
  StaxOffCpuBreakdown,
  StaxViewParams,
  HotmealApplyPatchesResult,
  HotmealDomAttr,
  HotmealDomNode,
  HotmealLiveReloadEvent,
  HotmealPatchStep,
  HelixAudioTokenRange,
  HelixAudioTokenProvenance,
  HelixAttentionSummaryBatch,
  HelixAttentionSupportSummary,
  HelixAudioAttendanceRow,
  HelixAudioEncoderSupportRecord,
  HelixDecoderEvidenceReport,
  HelixDecoderEvidenceVariantCounts,
  HelixAudioSelfAttentionRow,
  HelixPulseBundle,
  HelixPulseBundleFields,
  HelixPulseAvailable,
  HelixQueryRowAttentionRecord,
  HelixRefreshAttendanceRow,
  HelixStreamMetrics,
  HelixStreamMeta,
  HelixStreamingTraceEvent,
  HelixTextAttendanceRow,
  HelixTextAttentionSupportRecord,
  HelixTraceServiceSurface,
  HelixTranscriptToken,
  HelixRunInfo,
  HelixPieceEvalReference,
  HelixPieceEvalSnapshot,
  HelixVerifyDraftRow,
  HelixVerifyDraftStatus,
  HelixVerifyEvidenceDigest,
  HelixVerifySeedRow,
  TraceyApiCodeUnit,
  TraceyApiConfig,
  TraceyApiFileData,
  TraceyApiFileEntry,
  TraceyApiReverseData,
  TraceyApiRule,
  TraceyApiSpecData,
  TraceyApiSpecForward,
  TraceyApiSpecInfo,
  TraceyApiStaleRef,
  TraceyCodeRef,
  TraceyConfigPatternRequest,
  TraceyCoverageChange,
  TraceyDataUpdate,
  TraceyDeltaSummary,
  TraceyFileRequest,
  TraceyHealthResponse,
  TraceyHoverInfo,
  TraceyImplStatus,
  TraceyLspCodeAction,
  TraceyLspCodeLens,
  TraceyLspCompletionItem,
  TraceyLspDiagnostic,
  TraceyLspDocumentRequest,
  TraceyLspFileDiagnostics,
  TraceyLspInlayHint,
  TraceyLspInlayHintsRequest,
  TraceyLspLocation,
  TraceyLspPositionRequest,
  TraceyLspReferencesRequest,
  TraceyLspRenameRequest,
  TraceyLspSemanticToken,
  TraceyLspSymbol,
  TraceyLspTextEdit,
  TraceyOutlineCoverage,
  TraceyOutlineEntry,
  TraceyPrepareRenameResult,
  TraceyReloadResponse,
  TraceySearchResult,
  TraceySpecSection,
  TraceyRuleCoverage,
  TraceyRuleId,
  TraceyRuleRef,
  TraceyRuleInfo,
  TraceySectionRules,
  TraceyStaleEntry,
  TraceyStaleRequest,
  TraceyStaleResponse,
  TraceyStatusResponse,
  TraceyUncoveredRequest,
  TraceyUncoveredResponse,
  TraceyUnmappedEntry,
  TraceyUnmappedRequest,
  TraceyUnmappedResponse,
  TraceyUnmappedUnit,
  TraceyUntestedRequest,
  TraceyUntestedResponse,
  TraceyUpdateError,
  TraceyUpdateFileRangeRequest,
  TraceyValidateRequest,
  TraceyValidationError,
  TraceyValidationErrorCode,
  TraceyValidationResult,
} from "@bearcove/vox-generated/testbed.generated.ts";
import { TestbedClient, TestbedDispatcher } from "@bearcove/vox-generated/testbed.generated.ts";
import type { Value } from "@bearcove/phon-schema";
import { tcpConnector, acceptTcp } from "@bearcove/vox-tcp";
import { createServer as createTcpServer, type AddressInfo } from "net";
import { wsConnector } from "@bearcove/vox-ws";
import {
  accept,
  connect,
  Driver,
  ConnectionError,
  channel,
  setVoxLogger,
  voxServiceMetadata,
  type Tx,
  type Rx,
} from "@bearcove/vox-core";
import { withSubjectTimeout } from "./timeout.ts";

// Enable vox internals logging for test visibility
setVoxLogger({
  debug: (...args) => console.error(...args),
  error: (...args) => console.error(...args),
});

function sameBytes(lhs: Uint8Array, rhs: Uint8Array): boolean {
  return lhs.length === rhs.length && lhs.every((value, idx) => rhs[idx] === value);
}

async function waitForBound(...handles: Array<{ isBound: boolean }>): Promise<void> {
  while (!handles.every((handle) => handle.isBound)) {
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
  }
}

function sameStringSet(lhs: Set<string>, rhs: Set<string>): boolean {
  return lhs.size === rhs.size && [...lhs].every((value) => rhs.has(value));
}

function sameValue(lhs: Value, rhs: Value): boolean {
  if (lhs instanceof Uint8Array || rhs instanceof Uint8Array) {
    return lhs instanceof Uint8Array && rhs instanceof Uint8Array && sameBytes(lhs, rhs);
  }
  if (lhs instanceof Map || rhs instanceof Map) {
    return lhs instanceof Map
      && rhs instanceof Map
      && lhs.size === rhs.size
      && [...lhs].every(([key, value]) => {
        const other = rhs.get(key);
        return other !== undefined && sameValue(value, other);
      });
  }
  if (Array.isArray(lhs) || Array.isArray(rhs)) {
    return Array.isArray(lhs)
      && Array.isArray(rhs)
      && lhs.length === rhs.length
      && lhs.every((value, idx) => sameValue(value, rhs[idx] as Value));
  }
  return Object.is(lhs, rhs);
}

function sameSrcset(lhs: [string, number][], rhs: [string, number][]): boolean {
  return lhs.length === rhs.length
    && lhs.every(([path, width], idx) => rhs[idx]?.[0] === path && rhs[idx]?.[1] === width);
}

function sameEcosystemBridgePayload(lhs: EcosystemBridgePayload, rhs: EcosystemBridgePayload): boolean {
  return lhs.html === rhs.html
    && lhs.path_map.size === rhs.path_map.size
    && [...lhs.path_map].every(([key, value]) => rhs.path_map.get(key) === value)
    && sameStringSet(lhs.known_routes, rhs.known_routes)
    && lhs.image_variants.size === rhs.image_variants.size
    && [...lhs.image_variants].every(([key, value]) => {
      const other = rhs.image_variants.get(key);
      return other !== undefined
        && sameSrcset(value.jxl_srcset, other.jxl_srcset)
        && sameSrcset(value.webp_srcset, other.webp_srcset);
    })
    && lhs.blobs.length === rhs.blobs.length
    && lhs.blobs.every((blob, idx) => sameBytes(blob, rhs.blobs[idx] ?? new Uint8Array()));
}

function sampleEcosystemBridgePayload(): EcosystemBridgePayload {
  return {
    html: "<main><img src=\"/hero.png\"></main>",
    path_map: new Map([["/old.css", "/assets/new.css"]]),
    known_routes: new Set(["/", "/guide/"]),
    image_variants: new Map([
      ["/hero.png", { jxl_srcset: [["/hero-640.jxl", 640]], webp_srcset: [["/hero-640.webp", 640]] }],
    ]),
    blobs: [new Uint8Array([0, 1, 2, 3, 255]), new Uint8Array()],
  };
}

function sampleDodecaTemplateCall(): DodecaTemplateCall {
  const context = new Map<string, Value>([
    ["sidebar", true],
    ["title", "Phon migration"],
    ["count", 42n],
  ]);
  return {
    context_id: "ctx-docs",
    name: "render-card",
    args: [context, "docs"],
    kwargs: [["path", "/guide/"]],
  };
}

function sampleDodecaDataContent(): string {
  return "{\"title\":\"Phon\",\"sidebar\":true,\"count\":42}";
}

function sampleDodecaDataFormat(): DodecaDataFormat {
  return { tag: "Json" };
}

function sampleDodecaLoadDataResult(): DodecaLoadDataResult {
  return {
    tag: "Success",
    value: new Map<string, Value>([
      ["title", "Phon"],
      ["sidebar", true],
      ["count", 42n],
    ]),
  };
}

function sampleDodecaMarkdownSourcePath(): string {
  return "content/guide.md";
}

function sampleDodecaMarkdownContent(): string {
  return "+++\ntitle = \"Phon migration\"\n+++\n\n# Intro\n\nr[vox.dodeca.markdown]\n";
}

function sampleDodecaParseResult(): DodecaParseResult {
  return {
    tag: "Success",
    frontmatter: {
      title: "Phon migration",
      weight: 10,
      description: "Generated fixture for Dodeca markdown",
      template: "page.html",
      extra: new Map<string, Value>([
        ["sidebar", true],
        ["icon", "book"],
        ["custom_value", 42n],
      ]),
    },
    html: "<h1 data-sid=\"h1\">Intro</h1><p data-sid=\"p1\">Generated fixture</p>",
    headings: [{ title: "Intro", id: "intro", level: 1 }],
    reqs: [{ id: "vox.dodeca.markdown", anchor_id: "r-vox-dodeca-markdown" }],
    head_injections: ["<link rel=\"stylesheet\" href=\"/assets/arborium.css\">"],
    source_map: {
      source_path: sampleDodecaMarkdownSourcePath(),
      entries: [
        {
          id: "h1",
          kind: { tag: "Heading" },
          line_start: 5,
          line_end: 5,
          byte_start: 38n,
          byte_end: 45n,
        },
        {
          id: "p1",
          kind: { tag: "Paragraph" },
          line_start: 7,
          line_end: 7,
          byte_start: 47n,
          byte_end: 71n,
        },
      ],
    },
  };
}

function byteRamp(length: number, seed: number): Uint8Array {
  return Uint8Array.from({ length }, (_, i) => (seed + i) & 0xff);
}

function sampleDodecaDecodedImage(seed: number, width: number, height: number): DodecaDecodedImage {
  return {
    pixels: byteRamp(width * height * 4, seed),
    width,
    height,
    channels: 4,
  };
}

function sampleDodecaImageProcessorFixture(): DodecaImageProcessorFixture {
  const decoded = sampleDodecaDecodedImage(0x20, 96, 64);
  const resized = sampleDodecaDecodedImage(0x80, 48, 32);
  return {
    png_data: byteRamp(16_384, 0),
    decoded_result: { tag: "Success", image: decoded },
    resize_input: {
      pixels: decoded.pixels,
      width: decoded.width,
      height: decoded.height,
      channels: decoded.channels,
      target_width: resized.width,
    },
    resize_result: { tag: "Success", image: resized },
    thumbhash_input: {
      pixels: decoded.pixels,
      width: decoded.width,
      height: decoded.height,
    },
    thumbhash_result: { tag: "ThumbhashSuccess", data_url: "data:image/thumbhash;base64,BwgJCgsMDQ4PEA==" },
    error_result: { tag: "Error", message: "unsupported color profile in source image" },
  };
}

function sampleDodecaSearchIndexerFixture(): DodecaSearchIndexerFixture {
  return {
    pages: Array.from({ length: 32 }, (_, i) => ({
      url: `/guide/topic-${i}/`,
      source: `content/guide/topic-${i}.md`,
      html: `<article><h1>Topic ${i}</h1><p>Search body ${i}</p></article>`,
    })),
    result: {
      tag: "Success",
      files: Array.from({ length: 8 }, (_, i) => ({
        path: `public/search/chunk-${i}.json`,
        contents: byteRamp(1_024, i * 17),
      })),
    },
    error_result: { tag: "Error", message: "search index could not write public/search/index.json" },
  };
}

function sampleDodecaAssetProcessingFixture(): DodecaAssetProcessingFixture {
  return {
    css_source: "body { background: url('/old/bg.png'); color: red; }",
    css_path_map: new Map([
      ["/old/bg.png", "/assets/bg.abcd.png"],
      ["/old/font.woff2", "/assets/font.woff2"],
    ]),
    css_result: { tag: "Success", css: "body{background:url('/assets/bg.abcd.png');color:red}" },
    sass_entrypoint: "styles/app.scss",
    sass_files: new Map([
      ["styles/app.scss", "$brand: #c0ffee; @import 'partials/buttons'; body { color: $brand; }"],
      ["styles/partials/_buttons.scss", ".button { padding: 4px; }"],
    ]),
    sass_load_paths: ["styles", "vendor"],
    sass_result: { tag: "Success", css: "body{color:#c0ffee}.button{padding:4px}" },
    svg_source: "<svg viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"10\" fill=\"red\"/></svg>",
    svgo_result: {
      tag: "Success",
      svg: "<svg viewBox=\"0 0 10 10\"><path fill=\"red\" d=\"M0 0h10v10H0z\"/></svg>",
    },
  };
}


function sampleDodecaTaskProgress(
  name: string,
  total: number,
  completed: number,
  status: "Pending" | "Running" | "Done" | "Error",
) {
  return {
    name,
    total,
    completed,
    status: { tag: status },
    message: status === "Error" ? `${name} failed` : null,
  };
}

function sampleDodecaSmallCellServicesFixture(): DodecaSmallCellServicesFixture {
  return {
    ready_msg: {
      peer_id: 42,
      cell_name: "ddc-cell-fonts",
      pid: 12_345,
      version: "1.0.0-dev",
      features: ["woff2", "subset"],
    },
    ready_ack: { ok: true, host_time_unix_ms: 1_778_000_000_000n },
    minify_result: { tag: "Success", content: "<main><h1>Hi</h1></main>" },
    js_input: {
      js: "import '/assets/theme.css'; console.log('/assets/app.js')",
      path_map: new Map([
        ["/assets/app.js", "/assets/app.1234.js"],
        ["/assets/theme.css", "/assets/theme.abcd.css"],
      ]),
    },
    js_result: { ok: true, value: "import '/assets/theme.abcd.css'; console.log('/assets/app.1234.js')" },
    html_diff_input: {
      old_html: "<main><h1>Old</h1></main>",
      new_html: "<main><h1>New</h1><p>body</p></main>",
    },
    html_diff_result: { ok: true, value: { patches_blob: new Uint8Array([0x91, 0xa4, 0x70, 0x61, 0x74, 0x68]) } },
    subset_font_input: {
      data: new Uint8Array([0x77, 0x4f, 0x46, 0x32]),
      chars: ["A", String.fromCodePoint(0x00e9), String.fromCodePoint(0x1f41d)],
    },
    font_results: [
      { tag: "DecompressSuccess", data: new Uint8Array([0x00, 0x01, 0x00, 0x00]) },
      { tag: "SubsetSuccess", data: new Uint8Array([0xde, 0xad, 0xbe, 0xef]) },
      { tag: "CompressSuccess", data: new Uint8Array([0x77, 0x4f, 0x46, 0x32, 0x01]) },
    ],
    webp_encode_input: {
      pixels: new Uint8Array([0, 32, 64, 255, 255, 128, 0, 255]),
      width: 2,
      height: 1,
      quality: 82,
    },
    webp_results: [
      { tag: "DecodeSuccess", pixels: new Uint8Array([0, 32, 64, 255]), width: 1, height: 1, channels: 4 },
      { tag: "EncodeSuccess", data: new Uint8Array([0x52, 0x49, 0x46, 0x46]) },
    ],
    jxl_encode_input: {
      pixels: new Uint8Array([0, 0, 0, 255, 255, 255, 255, 255]),
      width: 2,
      height: 1,
      quality: 90,
    },
    jxl_results: [
      { tag: "DecodeSuccess", pixels: new Uint8Array([255, 0, 255, 255]), width: 1, height: 1, channels: 4 },
      { tag: "Error", message: "unsupported color profile" },
    ],
    select_result: { tag: "Selected", index: 2n },
    confirm_result: { tag: "Yes" },
    record_config: { shell: "/bin/zsh" },
    term_result: { tag: "Success", html: "<t-b>cargo nextest</t-b>" },
    start_dev_server_result: { tag: "Success", port: 5173 },
    run_build_result: { tag: "Error", message: "vite config missing" },
    link_check_input: {
      urls: ["https://example.com/ok", "https://example.com/missing"],
      delay_ms: 250n,
      timeout_secs: 15n,
    },
    link_check_result: {
      tag: "Success",
      output: {
        results: new Map([
          ["https://example.com/ok", { tag: "Ok" }],
          [
            "https://example.com/missing",
            {
              tag: "HttpError",
              code: 404,
              diagnostics: {
                request_headers: [["accept", "text/html"]],
                response_headers: [["content-type", "text/html"]],
                response_body: "<h1>not found</h1>",
              },
            },
          ],
          ["https://slow.example.com", { tag: "Skipped" }],
        ]),
      },
    },
    build_progress: {
      parse: sampleDodecaTaskProgress("parse", 12, 12, "Done"),
      render: sampleDodecaTaskProgress("render", 48, 40, "Running"),
      sass: sampleDodecaTaskProgress("sass", 3, 3, "Done"),
      links: sampleDodecaTaskProgress("links", 10, 7, "Running"),
      search: sampleDodecaTaskProgress("search", 1, 0, "Pending"),
    },
    log_event: {
      level: { tag: "Warn" },
      kind: { tag: "Http", status: 404 },
      message: "dead link",
      fields: [["route", "/guide/"], ["href", "/missing/"]],
    },
    server_status: {
      urls: ["http://127.0.0.1:5173", "http://192.168.1.42:5173"],
      is_running: true,
      bind_mode: { tag: "Lan" },
      picante_cache_size: 4_096n,
      cas_cache_size: 8_192n,
      code_exec_cache_size: 1_024n,
    },
    server_command: { tag: "SetLogFilter", filter: "dodeca=debug,cell=trace" },
    command_result: { tag: "Ok" },
  };
}

function sampleDodecaSourceLines(): DodecaSourceLine[] {
  return [
    { number: 12, content: "{% for item in data.items %}" },
    { number: 13, content: "{{ item.title }}" },
  ];
}

function sampleDodecaSourceSnippet(): DodecaSourceSnippet {
  return {
    lines: sampleDodecaSourceLines(),
    error_line: 13,
  };
}

function sampleDodecaErrorInfo(): DodecaErrorInfo {
  return {
    route: "/guide/",
    message: "unknown filter `slugify`",
    template: "templates/page.html",
    line: 13,
    column: 8,
    source_snippet: sampleDodecaSourceSnippet(),
    snapshot_id: "snap-devtools-42",
    available_variables: ["page", "root", "data"],
  };
}

function sampleDodecaDevtoolsEvent(): DodecaDevtoolsEvent {
  return { tag: "Error", value: sampleDodecaErrorInfo() };
}

function sampleDodecaScopeEntries(): DodecaScopeEntry[] {
  return [
    { name: "title", value: { tag: "String", value: "Phon migration" }, expandable: false },
    {
      name: "items",
      value: { tag: "Array", length: 3n, preview: "[intro, install, api]" },
      expandable: true,
    },
    {
      name: "metrics",
      value: { tag: "Object", fields: 2n, preview: "{views, updated_at}" },
      expandable: true,
    },
    { name: "score", value: { tag: "Number", value: 42.5 }, expandable: false },
  ];
}

function sampleDodecaEvalResult(): DodecaEvalResult {
  return { tag: "Ok", value: { tag: "Object", fields: 2n, preview: "{title, route}" } };
}

function sampleDodecaDeadLinkTarget(): DodecaDeadLinkTarget {
  return { tag: "Wiki", key: "missing-page", title: "Missing Page" };
}

function sampleDodecaOpenSourceResult(): DodecaOpenSourceResult {
  return { tag: "Ok" };
}

function sampleDodecaSidLines(): DodecaSidLine[] {
  return [
    { sid: "p-1", line: 5 },
    { sid: "code-1", line: 17 },
  ];
}

function sampleDodecaEditLoad(): DodecaEditLoad {
  return {
    tag: "Ok",
    source_key: "content/guide.md",
    route: "/guide/",
    uri: "file:///workspace/content/guide.md",
    content: "# Guide\n\nWelcome to Phon.",
    base: "a1b2c3d4",
  };
}

function sampleDodecaEditPreview(): DodecaEditPreview {
  return {
    tag: "Ok",
    html: "<article><h1>Guide</h1><p>Welcome to Phon.</p></article>",
    source_map: sampleDodecaSidLines(),
  };
}

function sampleDodecaEditSaveReq(): DodecaEditSaveReq {
  return {
    source_key: "content/guide.md",
    buffer: "# Guide\n\nUpdated from browser.",
    base: "a1b2c3d4",
    message: "Update guide",
  };
}

function sampleDodecaEditSave(): DodecaEditSave {
  return { tag: "Ok", commit: "deadbeef1234", base: "b4c3d2a1" };
}

function sampleDodecaEditUploadReq(): DodecaEditUploadReq {
  return {
    source_key: "content/guide.md",
    filename: "diagram.png",
    bytes: byteRamp(128, 31),
  };
}

function sampleDodecaEditUpload(): DodecaEditUpload {
  return { tag: "Ok", markdown: "![diagram](./diagram.png)", path: "diagram.png" };
}

function sampleDodecaEditRead(): DodecaEditRead {
  return {
    tag: "Ok",
    content: "# Guide\n\nWelcome to Phon.",
    base: "a1b2c3d4",
  };
}

function sampleDodecaEditList(): DodecaEditList {
  return {
    tag: "Ok",
    entries: [
      {
        source_key: "content/guide.md",
        route: "/guide/",
        uri: "file:///workspace/content/guide.md",
        title: "Guide",
      },
      {
        source_key: "content/reference.md",
        route: "/reference/",
        uri: "file:///workspace/content/reference.md",
        title: "Reference",
      },
    ] satisfies DodecaEditEntry[],
  };
}

function sameDodecaTemplateCall(lhs: DodecaTemplateCall, rhs: DodecaTemplateCall): boolean {
  return lhs.context_id === rhs.context_id
    && lhs.name === rhs.name
    && lhs.args.length === rhs.args.length
    && lhs.args.every((value, idx) => sameValue(value, rhs.args[idx] as Value))
    && lhs.kwargs.length === rhs.kwargs.length
    && lhs.kwargs.every(([key, value], idx) => {
      const other = rhs.kwargs[idx];
      return other !== undefined && key === other[0] && sameValue(value, other[1]);
    });
}

function sampleDodecaResolvedDependency() {
  return {
    name: "facet",
    version: "0.46.0",
    source: {
      tag: "Git",
      url: "https://github.com/facet-rs/facet",
      commit: "abc1234",
    },
  } as const;
}

function sampleDodecaCodeMetadata(): DodecaCodeExecutionMetadata {
  return {
    rustc_version: "rustc 1.89.0",
    cargo_version: "cargo 1.89.0",
    target: "aarch64-apple-darwin",
    timestamp: "2026-06-05T00:00:00Z",
    cache_hit: true,
    platform: "macos",
    arch: "aarch64",
    dependencies: [sampleDodecaResolvedDependency()],
  };
}

function sampleDodecaResponsiveImageInfo(): DodecaResponsiveImageInfo {
  return {
    jxl_srcset: [["/assets/hero-640.jxl", 640], ["/assets/hero-1280.jxl", 1280]],
    webp_srcset: [["/assets/hero-640.webp", 640]],
    original_width: 1920,
    original_height: 1080,
    thumbhash_data_url: "data:image/png;base64,dGh1bWI=",
  };
}

function sampleDodecaHtmlProcessInput(): DodecaHtmlProcessInput {
  return {
    html: "<main><a href=\"/missing\">missing</a><img src=\"/hero.png\"></main>",
    path_map: new Map([["/old/hero.png", "/assets/hero.png"]]),
    known_routes: new Set(["/", "/guide/"]),
    code_metadata: new Map([["sample-1", sampleDodecaCodeMetadata()]]),
    injections: [
      { tag: "HeadStyle", css: "body { color: oklch(0.2 0.03 240); }" },
      { tag: "HeadScript", js: "console.log('dodeca')", module: true },
      { tag: "BodyScript", js: "window.__dodeca = true", module: false },
    ],
    minify: {
      minify_inline_css: true,
      minify_inline_js: true,
      minify_html: false,
    },
    source_to_route: new Map([["content/guide.md", "/guide/"]]),
    wiki_to_route: new Map([["getting-started", "/guide/"]]),
    base_route: "/guide/intro/",
    image_variants: new Map([["/hero.png", sampleDodecaResponsiveImageInfo()]]),
    vite_css_map: new Map([["/src/main.ts", ["/assets/main.css", "/assets/theme.css"]]]),
    mount: {
      segment: "wiki",
      routes: new Set(["/exec/", "/guide/"]),
    },
  };
}

function sampleDodecaHtmlProcessResult(): DodecaHtmlProcessResult {
  return {
    tag: "Success",
    html: "<main data-processed=\"true\"><a data-dead href=\"/missing\">missing</a></main>",
    had_dead_links: true,
    had_code_buttons: true,
    hrefs: ["/missing", "/guide/"],
    element_ids: ["intro", "sample-1"],
    unresolved_wiki_links: [{ key: "unknown", target: "Missing Page" }],
  };
}

function sampleDodecaDependencySpec(): DodecaDependencySpec {
  return {
    name: "facet",
    version: "0.46",
    git: "https://github.com/facet-rs/facet",
    rev: null,
    branch: "main",
    path: null,
    features: ["derive"],
  };
}

function sampleDodecaRustConfig(): DodecaRustConfig {
  return {
    command: "cargo",
    args: ["run", "--quiet"],
    extension: "rs",
    prepare_code: true,
    auto_imports: ["use std::collections::HashMap;", "use facet::Facet;"],
    show_output: true,
  };
}

function sampleDodecaCodeExecutionConfig(): DodecaCodeExecutionConfig {
  return {
    enabled: true,
    fail_on_error: true,
    timeout_secs: 30n,
    cache_dir: ".cache/code-execution",
    project_root: "/workspace/docs",
    dependencies: [sampleDodecaDependencySpec()],
    rust: sampleDodecaRustConfig(),
  };
}

function sampleDodecaCodeSample(): DodecaCodeSample {
  return {
    source_path: "content/guide.md",
    line: 42n,
    language: "rust",
    code: "#[derive(Facet)]\nstruct Card { title: String }",
    executable: true,
    expected_errors: [],
  };
}

function sampleDodecaBuildMetadata(): DodecaBuildMetadata {
  return {
    rustc_version: "rustc 1.89.0",
    cargo_version: "cargo 1.89.0",
    target: "aarch64-apple-darwin",
    timestamp: "2026-06-05T00:00:00Z",
    cache_hit: false,
    platform: "macos",
    arch: "aarch64",
    dependencies: [sampleDodecaResolvedDependency()],
  };
}

function sampleDodecaExecuteSamplesInput(): DodecaExecuteSamplesInput {
  return {
    samples: [sampleDodecaCodeSample()],
    config: sampleDodecaCodeExecutionConfig(),
  };
}

function sampleDodecaCodeExecutionResult(): DodecaCodeExecutionResult {
  return {
    tag: "ExecuteSuccess",
    output: {
      results: [[
        sampleDodecaCodeSample(),
        {
          status: { tag: "Success" },
          exit_code: 0,
          stdout: "Card { title: \"Phon\" }",
          stderr: "",
          duration_ms: 128n,
          error: null,
          metadata: sampleDodecaBuildMetadata(),
        } satisfies DodecaExecutionResult,
      ]],
    },
  };
}

function sampleDibsListRequest(): DibsListRequest {
  return {
    table: "products",
    filters: [
      { field: "active", op: { tag: "Eq" }, value: { tag: "Bool", value: true }, values: [] },
      {
        field: "id",
        op: { tag: "In" },
        value: { tag: "Null" },
        values: [{ tag: "I64", value: 1n }, { tag: "I64", value: 2n }],
      },
      {
        field: "metadata",
        op: { tag: "JsonGetText" },
        value: { tag: "String", value: "sku" },
        values: [],
      },
    ],
    sort: [{ field: "created_at", dir: { tag: "Desc" } }],
    limit: 2,
    offset: 0,
    select: ["id", "name", "active", "payload"],
  };
}

function sampleDibsListResponse(): DibsListResponse {
  return {
    rows: [sampleDibsRowOne(), sampleDibsRowTwo()],
    total: 2n,
  };
}

function sampleDibsRowOne(): DibsRow {
  return {
    fields: [
      { name: "id", value: { tag: "I64", value: 1n } },
      { name: "name", value: { tag: "String", value: "phon adapter" } },
      { name: "active", value: { tag: "Bool", value: true } },
      { name: "score", value: { tag: "F64", value: 9.5 } },
      { name: "payload", value: { tag: "Bytes", value: new Uint8Array([0, 1, 2, 255]) } },
    ],
  };
}

function sampleDibsRowTwo(): DibsRow {
  return {
    fields: [
      { name: "id", value: { tag: "I64", value: 2n } },
      { name: "name", value: { tag: "String", value: "vox bridge" } },
      { name: "active", value: { tag: "Bool", value: false } },
      { name: "small", value: { tag: "I16", value: 7 } },
      { name: "count", value: { tag: "I32", value: 42 } },
      { name: "ratio", value: { tag: "F32", value: 0.5 } },
      { name: "deleted_at", value: { tag: "Null" } },
      { name: "payload", value: { tag: "Bytes", value: new Uint8Array() } },
    ],
  };
}

function sampleDibsSchema(): DibsSchemaInfo {
  return {
    tables: [{
      name: "products",
      columns: [
        {
          name: "id",
          sql_type: "BIGINT",
          rust_type: "i64",
          nullable: false,
          default: "generated by default as identity",
          primary_key: true,
          unique: true,
          auto_generated: true,
          long: false,
          label: false,
          enum_variants: [],
          doc: "Product primary key",
          lang: null,
          icon: "hash",
          subtype: null,
        },
        {
          name: "name",
          sql_type: "TEXT",
          rust_type: "String",
          nullable: false,
          default: null,
          primary_key: false,
          unique: false,
          auto_generated: false,
          long: false,
          label: true,
          enum_variants: [],
          doc: "Display name",
          lang: null,
          icon: "text",
          subtype: null,
        },
        {
          name: "status",
          sql_type: "TEXT",
          rust_type: "ProductStatus",
          nullable: false,
          default: "'draft'",
          primary_key: false,
          unique: false,
          auto_generated: false,
          long: false,
          label: false,
          enum_variants: ["draft", "active"],
          doc: null,
          lang: null,
          icon: "badge",
          subtype: null,
        },
        {
          name: "metadata",
          sql_type: "JSONB",
          rust_type: "Jsonb<facet_value::Value>",
          nullable: true,
          default: null,
          primary_key: false,
          unique: false,
          auto_generated: false,
          long: true,
          label: false,
          enum_variants: [],
          doc: "Structured product metadata",
          lang: "json",
          icon: "braces",
          subtype: null,
        },
        {
          name: "category_id",
          sql_type: "BIGINT",
          rust_type: "Option<i64>",
          nullable: true,
          default: null,
          primary_key: false,
          unique: false,
          auto_generated: false,
          long: false,
          label: false,
          enum_variants: [],
          doc: null,
          lang: null,
          icon: "link",
          subtype: null,
        },
      ],
      foreign_keys: [{
        columns: ["category_id"],
        references_table: "categories",
        references_columns: ["id"],
      }],
      indices: [{
        name: "products_active_created_at_idx",
        columns: [
          { name: "active", order: "asc", nulls: "default" },
          { name: "created_at", order: "desc", nulls: "last" },
        ],
        unique: false,
        where_clause: "deleted_at IS NULL",
      }],
      source_file: "examples/my-app-workspace/my-app-db/src/lib.rs",
      source_line: 42,
      doc: "Products shown in the dynamic Dibs admin UI",
      icon: "package",
    }],
  };
}

function sampleDibsGetRequest(): DibsGetRequest {
  return { table: "products", pk: { tag: "I64", value: 1n } };
}

function sampleDibsCreateRequest(): DibsCreateRequest {
  return {
    table: "products",
    data: {
      fields: [
        { name: "name", value: { tag: "String", value: "new adapter" } },
        { name: "active", value: { tag: "Bool", value: true } },
      ],
    },
  };
}

function sampleDibsCreateResponse(): DibsRow {
  return {
    fields: [
      { name: "id", value: { tag: "I64", value: 3n } },
      { name: "name", value: { tag: "String", value: "new adapter" } },
      { name: "active", value: { tag: "Bool", value: true } },
    ],
  };
}

function sampleDibsUpdateRequest(): DibsUpdateRequest {
  return {
    table: "products",
    pk: { tag: "I64", value: 1n },
    data: {
      fields: [
        { name: "active", value: { tag: "Bool", value: false } },
        { name: "score", value: { tag: "F64", value: 10.0 } },
      ],
    },
  };
}

function sampleDibsUpdateResponse(): DibsRow {
  return {
    fields: [
      { name: "id", value: { tag: "I64", value: 1n } },
      { name: "name", value: { tag: "String", value: "phon adapter" } },
      { name: "active", value: { tag: "Bool", value: false } },
      { name: "score", value: { tag: "F64", value: 10.0 } },
    ],
  };
}

function sampleDibsDeleteRequest(): DibsDeleteRequest {
  return { table: "products", pk: { tag: "I64", value: 2n } };
}

function sampleDibsMigrationStatusRequest(): DibsMigrationStatusRequest {
  return { database_url: "postgres://localhost/dibs_fixture" };
}

function sampleDibsMigrationStatus(): DibsMigrationInfo[] {
  return [
    {
      version: "20240501000000",
      name: "create_users",
      applied: true,
      applied_at: "2024-05-01T00:00:00Z",
      source_file: "migrations/20240501000000_create_users.rs",
      source: "CREATE TABLE users (...)",
    },
    {
      version: "20240601000000",
      name: "create_products",
      applied: false,
      applied_at: null,
      source_file: "migrations/20240601000000_create_products.rs",
      source: "CREATE TABLE products (...)",
    },
  ];
}

function sampleDibsMigrateRequest(): DibsMigrateRequest {
  return {
    database_url: "postgres://localhost/dibs_fixture",
    migration: "20240601000000_create_products",
  };
}

function sampleDibsLogs(): DibsMigrationLog[] {
  const migration = "20240601000000_create_products";
  return [
    { level: { tag: "Info" }, message: "checking migrations", migration: null },
    { level: { tag: "Debug" }, message: "running migration", migration },
    { level: { tag: "Warn" }, message: "sample warning", migration },
    { level: { tag: "Info" }, message: "migration complete", migration },
  ];
}

function sampleDibsMigrateResult(): DibsMigrateResult {
  return {
    total_defined: 3,
    already_applied: [
      { version: "20240501000000_create_users", applied_at: "2024-05-01T00:00:00Z" },
    ],
    applied: [{ version: "20240601000000_create_products", duration_ms: 37n }],
    setup_ms: 5n,
    total_time_ms: 42n,
  };
}

function sameDibsLogLevel(lhs: DibsLogLevel, rhs: DibsLogLevel): boolean {
  return lhs.tag === rhs.tag;
}

function sameDibsValue(lhs: DibsValue, rhs: DibsValue): boolean {
  if (lhs.tag !== rhs.tag) return false;
  switch (lhs.tag) {
    case "Null":
      return true;
    case "Bool":
    case "I16":
    case "I32":
    case "I64":
    case "F32":
    case "F64":
    case "String":
      return Object.is(lhs.value, (rhs as typeof lhs).value);
    case "Bytes":
      return sameBytes(lhs.value, (rhs as typeof lhs).value);
  }
}

function sameDibsRowField(lhs: DibsRowField, rhs: DibsRowField): boolean {
  return lhs.name === rhs.name && sameDibsValue(lhs.value, rhs.value);
}

function sameDibsRow(lhs: DibsRow, rhs: DibsRow): boolean {
  return lhs.fields.length === rhs.fields.length
    && lhs.fields.every((field, idx) => sameDibsRowField(field, rhs.fields[idx] as DibsRowField));
}

function sameDibsListResponse(lhs: DibsListResponse, rhs: DibsListResponse): boolean {
  return lhs.total === rhs.total
    && lhs.rows.length === rhs.rows.length
    && lhs.rows.every((row, idx) => sameDibsRow(row, rhs.rows[idx] as DibsRow));
}

function sameDibsMigrationLog(lhs: DibsMigrationLog, rhs: DibsMigrationLog): boolean {
  return sameDibsLogLevel(lhs.level, rhs.level)
    && lhs.message === rhs.message
    && lhs.migration === rhs.migration;
}

function sameDibsLogs(lhs: DibsMigrationLog[], rhs: DibsMigrationLog[]): boolean {
  return lhs.length === rhs.length
    && lhs.every((logEntry, idx) => sameDibsMigrationLog(logEntry, rhs[idx] as DibsMigrationLog));
}

function sameDibsMigrateResult(lhs: DibsMigrateResult, rhs: DibsMigrateResult): boolean {
  return lhs.total_defined === rhs.total_defined
    && lhs.setup_ms === rhs.setup_ms
    && lhs.total_time_ms === rhs.total_time_ms
    && lhs.already_applied.length === rhs.already_applied.length
    && lhs.already_applied.every((migration, idx) => {
      const other = rhs.already_applied[idx];
      return other !== undefined
        && migration.version === other.version
        && migration.applied_at === other.applied_at;
    })
    && lhs.applied.length === rhs.applied.length
    && lhs.applied.every((migration, idx) => {
      const other = rhs.applied[idx];
      return other !== undefined
        && migration.version === other.version
        && migration.duration_ms === other.duration_ms;
    });
}

function styxSpan(start: number, end: number): StyxSpan {
  return { start, end };
}

function styxScalar(text: string, kind: StyxScalarKind, start: number, end: number): StyxValue {
  return {
    tag: null,
    payload: { tag: "Scalar", value: { text, kind, span: styxSpan(start, end) } },
    span: styxSpan(start, end),
  };
}

function sampleStyxValue(): StyxValue {
  return {
    tag: { name: "schema", span: styxSpan(0, 7) },
    payload: {
      tag: "Object",
      value: {
        entries: [
          {
            key: styxScalar("title", { tag: "Bare" }, 9, 14),
            value: styxScalar("Phon migration", { tag: "Quoted" }, 15, 31),
            doc_comment: "page title",
          },
          {
            key: styxScalar("features", { tag: "Bare" }, 33, 41),
            value: {
              tag: { name: "seq", span: styxSpan(42, 46) },
              payload: {
                tag: "Sequence",
                value: {
                  items: [
                    styxScalar("jit", { tag: "Bare" }, 47, 50),
                    {
                      tag: { name: "object", span: styxSpan(51, 58) },
                      payload: {
                        tag: "Object",
                        value: {
                          entries: [
                            {
                              key: styxScalar("lang", { tag: "Bare" }, 59, 63),
                              value: styxScalar("rust", { tag: "Raw" }, 64, 70),
                              doc_comment: null,
                            },
                          ],
                          span: styxSpan(58, 71),
                        },
                      },
                      span: styxSpan(51, 71),
                    },
                  ],
                  span: styxSpan(46, 72),
                },
              },
              span: styxSpan(42, 72),
            },
            doc_comment: null,
          },
        ],
        span: styxSpan(8, 73),
      },
    },
    span: styxSpan(0, 73),
  };
}

function sameStyxSpan(lhs: StyxSpan | null, rhs: StyxSpan | null): boolean {
  if (lhs === null || rhs === null) return lhs === rhs;
  return lhs.start === rhs.start && lhs.end === rhs.end;
}

function sameStyxTag(lhs: StyxTag | null, rhs: StyxTag | null): boolean {
  if (lhs === null || rhs === null) return lhs === rhs;
  return lhs.name === rhs.name && sameStyxSpan(lhs.span, rhs.span);
}

function sameStyxScalarKind(lhs: StyxScalarKind, rhs: StyxScalarKind): boolean {
  return lhs.tag === rhs.tag;
}

function sameStyxScalar(lhs: StyxScalar, rhs: StyxScalar): boolean {
  return lhs.text === rhs.text
    && sameStyxScalarKind(lhs.kind, rhs.kind)
    && sameStyxSpan(lhs.span, rhs.span);
}

function sameStyxSequence(lhs: StyxSequence, rhs: StyxSequence): boolean {
  return lhs.items.length === rhs.items.length
    && lhs.items.every((item, idx) => sameStyxValue(item, rhs.items[idx] as StyxValue))
    && sameStyxSpan(lhs.span, rhs.span);
}

function sameStyxEntry(lhs: StyxEntry, rhs: StyxEntry): boolean {
  return sameStyxValue(lhs.key, rhs.key)
    && sameStyxValue(lhs.value, rhs.value)
    && lhs.doc_comment === rhs.doc_comment;
}

function sameStyxObject(lhs: StyxObject, rhs: StyxObject): boolean {
  return lhs.entries.length === rhs.entries.length
    && lhs.entries.every((entry, idx) => sameStyxEntry(entry, rhs.entries[idx] as StyxEntry))
    && sameStyxSpan(lhs.span, rhs.span);
}

function sameStyxPayload(lhs: StyxPayload | null, rhs: StyxPayload | null): boolean {
  if (lhs === null || rhs === null) return lhs === rhs;
  if (lhs.tag !== rhs.tag) return false;
  switch (lhs.tag) {
    case "Scalar":
      return sameStyxScalar(lhs.value, (rhs as typeof lhs).value);
    case "Sequence":
      return sameStyxSequence(lhs.value, (rhs as typeof lhs).value);
    case "Object":
      return sameStyxObject(lhs.value, (rhs as typeof lhs).value);
  }
}

function sameStyxValue(lhs: StyxValue, rhs: StyxValue): boolean {
  return sameStyxTag(lhs.tag, rhs.tag)
    && sameStyxPayload(lhs.payload, rhs.payload)
    && sameStyxSpan(lhs.span, rhs.span);
}

function sampleStyxLspUri(): string {
  return "file:///workspace/queries.styx";
}

function sampleStyxLspSource(): string {
  return "@query { from products select (id name) }";
}

function sampleStyxLspCursor(): StyxLspCursor {
  return { line: 0, character: 16, offset: 16 };
}

function sampleStyxLspRange(): StyxLspRange {
  return {
    start: { line: 0, character: 0 },
    end: { line: 0, character: 38 },
  };
}

function sampleStyxLspInitializeParams(): StyxLspInitializeParams {
  return {
    styx_version: "4.0",
    document_uri: sampleStyxLspUri(),
    schema_id: "crate:dibs-queries@1",
  };
}

function sampleStyxLspInitializeResult(): StyxLspInitializeResult {
  const capabilities: StyxLspCapability[] = [
    { tag: "Completions" },
    { tag: "Hover" },
    { tag: "Diagnostics" },
    { tag: "CodeActions" },
    { tag: "Definition" },
  ];
  return {
    name: "dibs-styx-extension",
    version: "0.1.0",
    capabilities,
  };
}

function sampleStyxLspCompletionParams(): StyxLspCompletionParams {
  return {
    document_uri: sampleStyxLspUri(),
    cursor: sampleStyxLspCursor(),
    path: ["AllProducts", "@query", "select"],
    prefix: "na",
    context: sampleStyxValue(),
    tagged_context: sampleStyxValue(),
  };
}

function sampleStyxLspCompletions(): StyxLspCompletionItem[] {
  return [
    {
      label: "name",
      detail: "TEXT",
      documentation: "Product display name",
      kind: { tag: "Field" },
      sort_text: "0001",
      insert_text: null,
    },
    {
      label: "metadata",
      detail: "JSONB",
      documentation: null,
      kind: { tag: "Field" },
      sort_text: "0002",
      insert_text: "metadata",
    },
  ];
}

function sampleStyxLspHoverParams(): StyxLspHoverParams {
  return {
    document_uri: sampleStyxLspUri(),
    cursor: sampleStyxLspCursor(),
    path: ["AllProducts", "@query", "from"],
    context: sampleStyxValue(),
    tagged_context: sampleStyxValue(),
  };
}

function sampleStyxLspHoverResult(): StyxLspHoverResult {
  return {
    contents: "**products** table\n\nBacked by `Product`.",
    range: {
      start: { line: 0, character: 14 },
      end: { line: 0, character: 22 },
    },
  };
}

function sampleStyxLspInlayHintParams(): StyxLspInlayHintParams {
  return {
    document_uri: sampleStyxLspUri(),
    range: sampleStyxLspRange(),
    context: sampleStyxValue(),
  };
}

function sampleStyxLspInlayHints(): StyxLspInlayHint[] {
  return [{
    position: { line: 0, character: 9 },
    label: "Product",
    kind: { tag: "Type" },
    padding_left: true,
    padding_right: false,
  }];
}

function sampleStyxLspDiagnostic(): StyxLspDiagnostic {
  return {
    span: { start: 23, end: 29 },
    severity: { tag: "Warning" },
    message: "column `legacy` is deprecated",
    source: "dibs",
    code: "deprecated-column",
    data: sampleStyxValue(),
  };
}

function sampleStyxLspDiagnosticParams(): StyxLspDiagnosticParams {
  return {
    document_uri: sampleStyxLspUri(),
    tree: sampleStyxValue(),
    content: sampleStyxLspSource(),
  };
}

function sampleStyxLspDiagnostics(): StyxLspDiagnostic[] {
  return [sampleStyxLspDiagnostic()];
}

function sampleStyxLspCodeActionParams(): StyxLspCodeActionParams {
  return {
    document_uri: sampleStyxLspUri(),
    span: { start: 23, end: 29 },
    diagnostics: sampleStyxLspDiagnostics(),
  };
}

function sampleStyxLspCodeActions(): StyxLspCodeAction[] {
  return [{
    title: "Replace legacy column",
    kind: { tag: "QuickFix" },
    edit: {
      changes: [{
        uri: sampleStyxLspUri(),
        edits: [{
          span: { start: 23, end: 29 },
          new_text: "name",
        }],
      }],
    },
    is_preferred: true,
  }];
}

function sampleStyxLspDefinitionParams(): StyxLspDefinitionParams {
  return {
    document_uri: sampleStyxLspUri(),
    cursor: sampleStyxLspCursor(),
    path: ["AllProducts", "@query", "from"],
    context: sampleStyxValue(),
    tagged_context: sampleStyxValue(),
  };
}

function sampleStyxLspLocations(): StyxLspLocation[] {
  return [{
    uri: "file:///workspace/schema.styx",
    span: { start: 120, end: 128 },
  }];
}

function sampleStyxLspGetSubtreeParams(): StyxLspGetSubtreeParams {
  return {
    document_uri: sampleStyxLspUri(),
    path: ["AllProducts", "@query"],
  };
}

function sampleStyxLspGetDocumentParams(): StyxLspGetDocumentParams {
  return { document_uri: sampleStyxLspUri() };
}

function sampleStyxLspGetSourceParams(): StyxLspGetSourceParams {
  return { document_uri: sampleStyxLspUri() };
}

function sampleStyxLspGetSchemaParams(): StyxLspGetSchemaParams {
  return { document_uri: sampleStyxLspUri() };
}

function sampleStyxLspSchemaInfo(): StyxLspSchemaInfo {
  return {
    source: "@schema { @ @object{ name @string } }",
    uri: "styx-embedded://crate:dibs-queries@1",
  };
}

function sampleStyxLspOffsetToPositionParams(): StyxLspOffsetToPositionParams {
  return {
    document_uri: sampleStyxLspUri(),
    offset: 16,
  };
}

function sampleStyxLspPositionToOffsetParams(): StyxLspPositionToOffsetParams {
  return {
    document_uri: sampleStyxLspUri(),
    position: { line: 0, character: 16 },
  };
}

function sampleStyxLspPosition(): StyxLspPosition {
  return { line: 0, character: 16 };
}

function staxOffCpu(seed: bigint): StaxOffCpuBreakdown {
  return {
    idle_ns: seed + 1n,
    lock_ns: seed + 2n,
    semaphore_ns: seed + 3n,
    ipc_ns: seed + 4n,
    io_read_ns: seed + 5n,
    io_write_ns: seed + 6n,
    readiness_ns: seed + 7n,
    sleep_ns: seed + 8n,
    connect_ns: seed + 9n,
    other_ns: seed + 10n,
  };
}

function sampleStaxViewParams(): StaxViewParams {
  return {
    tid: 42,
    filter: {
      time_range: {
        start_ns: 1_000n,
        end_ns: 8_500n,
      },
      exclude_symbols: [
        {
          function_name: "malloc_zone_malloc",
          binary: "libsystem_malloc.dylib",
        },
        {
          function_name: null,
          binary: "libswift_Concurrency.dylib",
        },
      ],
    },
  };
}

function sampleStaxFlamegraphUpdate(params: StaxViewParams): StaxFlamegraphUpdate {
  const tid = params.tid ?? 0;
  const filterCount = BigInt(params.filter.exclude_symbols.length);
  const range = params.filter.time_range;
  const rangeNs = range === null
    ? 0n
    : (range.end_ns >= range.start_ns ? range.end_ns - range.start_ns : 0n);
  const totalOnCpuNs = 120_000n + BigInt(tid) + (rangeNs < 1_000n ? rangeNs : 1_000n);

  return {
    total_on_cpu_ns: totalOnCpuNs,
    total_off_cpu: staxOffCpu(100n + filterCount),
    strings: [
      "root",
      "bee::decode",
      "libbee.dylib",
      "rust",
      "phon::jit",
      "libphon.dylib",
    ],
    root: {
      address: 0n,
      function_name: 0,
      binary: null,
      is_main: true,
      language: 3,
      on_cpu_ns: totalOnCpuNs,
      off_cpu: staxOffCpu(200n + filterCount),
      pet_samples: 64n,
      off_cpu_intervals: 3n,
      cycles: 900_000n,
      instructions: 600_000n,
      l1d_misses: 42n,
      branch_mispreds: 7n,
      children: [
        {
          address: 0x1000n + BigInt(tid),
          function_name: 1,
          binary: 2,
          is_main: true,
          language: 3,
          on_cpu_ns: 80_000n + filterCount,
          off_cpu: staxOffCpu(300n + filterCount),
          pet_samples: 48n,
          off_cpu_intervals: 2n,
          cycles: 500_000n,
          instructions: 350_000n,
          l1d_misses: 30n,
          branch_mispreds: 5n,
          children: [
            {
              address: 0x2000n + BigInt(tid),
              function_name: 4,
              binary: 5,
              is_main: false,
              language: 3,
              on_cpu_ns: 45_000n,
              off_cpu: staxOffCpu(400n + filterCount),
              pet_samples: 32n,
              off_cpu_intervals: 1n,
              cycles: 250_000n,
              instructions: 180_000n,
              l1d_misses: 18n,
              branch_mispreds: 3n,
              children: [],
            },
          ],
        },
        {
          address: 0x3000n + BigInt(tid),
          function_name: null,
          binary: 2,
          is_main: false,
          language: 3,
          on_cpu_ns: 20_000n,
          off_cpu: staxOffCpu(500n + filterCount),
          pet_samples: 12n,
          off_cpu_intervals: 0n,
          cycles: 120_000n,
          instructions: 70_000n,
          l1d_misses: 4n,
          branch_mispreds: 1n,
          children: [],
        },
      ],
    },
  };
}

function sampleStaxSecondaryViewParams(): StaxViewParams {
  return {
    tid: null,
    filter: {
      time_range: {
        start_ns: 9_000n,
        end_ns: 9_640n,
      },
      exclude_symbols: [
        {
          function_name: "mach_msg2_trap",
          binary: null,
        },
      ],
    },
  };
}

function sampleStaxFlamegraphUpdates(): StaxFlamegraphUpdate[] {
  return [
    sampleStaxFlamegraphUpdate(sampleStaxViewParams()),
    sampleStaxFlamegraphUpdate(sampleStaxSecondaryViewParams()),
  ];
}

function sameStaxOffCpu(lhs: StaxOffCpuBreakdown, rhs: StaxOffCpuBreakdown): boolean {
  return lhs.idle_ns === rhs.idle_ns
    && lhs.lock_ns === rhs.lock_ns
    && lhs.semaphore_ns === rhs.semaphore_ns
    && lhs.ipc_ns === rhs.ipc_ns
    && lhs.io_read_ns === rhs.io_read_ns
    && lhs.io_write_ns === rhs.io_write_ns
    && lhs.readiness_ns === rhs.readiness_ns
    && lhs.sleep_ns === rhs.sleep_ns
    && lhs.connect_ns === rhs.connect_ns
    && lhs.other_ns === rhs.other_ns;
}

function sameStaxFlameNode(lhs: StaxFlameNode, rhs: StaxFlameNode): boolean {
  return lhs.address === rhs.address
    && lhs.function_name === rhs.function_name
    && lhs.binary === rhs.binary
    && lhs.is_main === rhs.is_main
    && lhs.language === rhs.language
    && lhs.on_cpu_ns === rhs.on_cpu_ns
    && sameStaxOffCpu(lhs.off_cpu, rhs.off_cpu)
    && lhs.pet_samples === rhs.pet_samples
    && lhs.off_cpu_intervals === rhs.off_cpu_intervals
    && lhs.cycles === rhs.cycles
    && lhs.instructions === rhs.instructions
    && lhs.l1d_misses === rhs.l1d_misses
    && lhs.branch_mispreds === rhs.branch_mispreds
    && lhs.children.length === rhs.children.length
    && lhs.children.every((child, idx) => sameStaxFlameNode(child, rhs.children[idx] as StaxFlameNode));
}

function sameStaxFlamegraphUpdate(
  lhs: StaxFlamegraphUpdate,
  rhs: StaxFlamegraphUpdate,
): boolean {
  return lhs.total_on_cpu_ns === rhs.total_on_cpu_ns
    && sameStaxOffCpu(lhs.total_off_cpu, rhs.total_off_cpu)
    && lhs.strings.length === rhs.strings.length
    && lhs.strings.every((value, idx) => value === rhs.strings[idx])
    && sameStaxFlameNode(lhs.root, rhs.root);
}

function sampleStaxLinuxBrokerControlFixture(): StaxLinuxBrokerControlFixture {
  const config: StaxLinuxPerfSessionConfig = {
    target_pid: 42_424,
    frequency_hz: 997,
    kernel_stacks: true,
    request_waking: true,
    request_pmu: true,
    request_dwarf_unwind: false,
  };
  const status: StaxLinuxDaemonStatus = {
    version: "1.0.0-dev",
    host_arch: "x86_64",
    privileged: true,
    perf_event_paranoid: 1,
  };
  const errors: StaxLinuxPerfSessionError[] = [
    {
      tag: "NotPrivileged",
      detail: "perf_event_paranoid=3 without CAP_PERFMON",
    },
    {
      tag: "PerfEventOpen",
      cpu: 3,
      errno: 24,
      detail: "EMFILE while opening PMU sibling",
    },
    {
      tag: "NoSuchTarget",
      value: 99_999,
    },
    {
      tag: "NotAuthorized",
      caller_uid: 501,
      target_uid: 0,
    },
  ];
  const wakingFieldOffsets: StaxLinuxWakingFieldOffsets = {
    wakee_pid_offset: 16,
    wakee_pid_size: 4,
  };
  return {
    config,
    status,
    errors,
    waking_field_offsets: wakingFieldOffsets,
  };
}

function sampleStaxMacosConfig(): StaxMacSessionConfig {
  return {
    target_pid: 42_424,
    frequency_hz: 997,
    buf_records: 1_048_576,
    samplers: 0x13,
    pmu_event_configs: [0xfeed_beefn, 0x1_0000_0001n],
    class_mask: 0b1011,
    filter_range_value1: 0x3100_0000,
    filter_range_value2: 0x31ff_ffff,
    typefilter_cscs: [0x3101, 0x3102, 0x3108],
  };
}

function sampleStaxMacosBatches(): StaxMacKdBufBatch[] {
  return [
    {
      records: [
        {
          timestamp: 900_000n,
          arg1: 0x1000n,
          arg2: 0x2000n,
          arg3: 0x3000n,
          arg4: 0x4000n,
          arg5: 0xfeed_facen,
          debugid: 0x3101_0004,
          cpuid: 3,
          unused: 0n,
        },
        {
          timestamp: 900_128n,
          arg1: 0x1008n,
          arg2: 0x2008n,
          arg3: 0x3008n,
          arg4: 0x4008n,
          arg5: 0xfeed_facen,
          debugid: 0x3101_0008,
          cpuid: 4,
          unused: 0n,
        },
      ],
      read_started_mach_ticks: 899_900n,
      drained_mach_ticks: 900_140n,
      queued_for_send_mach_ticks: 900_150n,
      send_started_mach_ticks: 900_180n,
      drained_at_unix_ns: 1_801_000_000_123_456_789n,
    },
    {
      records: [{
        timestamp: 900_256n,
        arg1: 0x1010n,
        arg2: 0x2010n,
        arg3: 0x3010n,
        arg4: 0x4010n,
        arg5: 0xfeed_facen,
        debugid: 0x3101_000c,
        cpuid: 5,
        unused: 0n,
      }],
      read_started_mach_ticks: 900_200n,
      drained_mach_ticks: 900_270n,
      queued_for_send_mach_ticks: 900_290n,
      send_started_mach_ticks: 900_310n,
      drained_at_unix_ns: 1_801_000_000_123_556_789n,
    },
  ];
}

function sampleStaxMacosRecordSummary(): StaxMacRecordSummary {
  return {
    records_drained: BigInt(sampleStaxMacosBatches().reduce((count, batch) => count + batch.records.length, 0)),
    session_ns: 240_000n,
  };
}

function sameStaxMacKdBuf(lhs: StaxMacKdBuf, rhs: StaxMacKdBuf): boolean {
  return lhs.timestamp === rhs.timestamp
    && lhs.arg1 === rhs.arg1
    && lhs.arg2 === rhs.arg2
    && lhs.arg3 === rhs.arg3
    && lhs.arg4 === rhs.arg4
    && lhs.arg5 === rhs.arg5
    && lhs.debugid === rhs.debugid
    && lhs.cpuid === rhs.cpuid
    && lhs.unused === rhs.unused;
}

function sameStaxMacBatch(lhs: StaxMacKdBufBatch, rhs: StaxMacKdBufBatch): boolean {
  return lhs.records.length === rhs.records.length
    && lhs.records.every((record, idx) => sameStaxMacKdBuf(record, rhs.records[idx] as StaxMacKdBuf))
    && lhs.read_started_mach_ticks === rhs.read_started_mach_ticks
    && lhs.drained_mach_ticks === rhs.drained_mach_ticks
    && lhs.queued_for_send_mach_ticks === rhs.queued_for_send_mach_ticks
    && lhs.send_started_mach_ticks === rhs.send_started_mach_ticks
    && lhs.drained_at_unix_ns === rhs.drained_at_unix_ns;
}

function sameStaxMacBatches(lhs: StaxMacKdBufBatch[], rhs: StaxMacKdBufBatch[]): boolean {
  return lhs.length === rhs.length
    && lhs.every((batch, idx) => sameStaxMacBatch(batch, rhs[idx] as StaxMacKdBufBatch));
}

function sampleHotmealLiveReloadEvents(): HotmealLiveReloadEvent[] {
  return [
    { tag: "Reload" },
    {
      tag: "Patches",
      route: "/guide/",
      patches_blob: new Uint8Array([0, 1, 2, 3, 255]),
    },
    {
      tag: "HeadChanged",
      route: "/guide/",
    },
  ];
}

function sampleHotmealRoute(): string {
  return "/guide/";
}

function sampleHotmealDomNode(): HotmealDomNode {
  return {
    $tag: "Element",
    tag: "main",
    attrs: [
      { name: "id", value: "app" },
      { name: "data-route", value: "/guide/" },
    ],
    children: [
      { $tag: "Text", value: "Hello " },
      {
        $tag: "Element",
        tag: "button",
        attrs: [{ name: "class", value: "primary" }],
        children: [{ $tag: "Text", value: "Reload" }],
      },
      { $tag: "Comment", value: "hotmeal-marker" },
    ],
  };
}

function sampleHotmealApplyPatchesResult(): HotmealApplyPatchesResult {
  const initial = sampleHotmealDomNode();
  return {
    result_html: "<main id=\"app\"><button class=\"primary\">Reload</button></main>",
    normalized_old_html: "<main id=\"app\">Hello</main>",
    initial_dom_tree: initial,
    patch_trace: [
      {
        index: 0,
        patch_debug: "ReplaceText(path=[0], text=\"Hello \")",
        html_after: "<main id=\"app\">Hello </main>",
        dom_tree: initial,
        error: null,
      },
      {
        index: 1,
        patch_debug: "InsertChild(path=[1], tag=\"button\")",
        html_after: "<main id=\"app\">Hello <button>Reload</button></main>",
        dom_tree: {
          $tag: "Element",
          tag: "main",
          attrs: [{ name: "id", value: "app" }],
          children: [
            { $tag: "Text", value: "Hello " },
            {
              $tag: "Element",
              tag: "button",
              attrs: [],
              children: [{ $tag: "Text", value: "Reload" }],
            },
          ],
        },
        error: "sample recoverable mismatch",
      },
    ],
  };
}

function sameHotmealDomAttr(lhs: HotmealDomAttr, rhs: HotmealDomAttr): boolean {
  return lhs.name === rhs.name && lhs.value === rhs.value;
}

function sameHotmealDomNode(lhs: HotmealDomNode, rhs: HotmealDomNode): boolean {
  if (lhs.$tag !== rhs.$tag) return false;
  switch (lhs.$tag) {
    case "Element": {
      const other = rhs as typeof lhs;
      return lhs.tag === other.tag
        && lhs.attrs.length === other.attrs.length
        && lhs.children.length === other.children.length
        && lhs.attrs.every((attr, idx) => sameHotmealDomAttr(attr, other.attrs[idx] as HotmealDomAttr))
        && lhs.children.every((child, idx) => sameHotmealDomNode(child, other.children[idx] as HotmealDomNode));
    }
    case "Text":
    case "Comment":
      return lhs.value === (rhs as typeof lhs).value;
  }
}

function sameHotmealPatchStep(lhs: HotmealPatchStep, rhs: HotmealPatchStep): boolean {
  return lhs.index === rhs.index
    && lhs.patch_debug === rhs.patch_debug
    && lhs.html_after === rhs.html_after
    && sameHotmealDomNode(lhs.dom_tree, rhs.dom_tree)
    && lhs.error === rhs.error;
}

function sameHotmealApplyPatchesResult(
  lhs: HotmealApplyPatchesResult,
  rhs: HotmealApplyPatchesResult,
): boolean {
  return lhs.result_html === rhs.result_html
    && lhs.normalized_old_html === rhs.normalized_old_html
    && sameHotmealDomNode(lhs.initial_dom_tree, rhs.initial_dom_tree)
    && lhs.patch_trace.length === rhs.patch_trace.length
    && lhs.patch_trace.every((step, idx) => sameHotmealPatchStep(step, rhs.patch_trace[idx] as HotmealPatchStep));
}

function sameHotmealLiveReloadEvent(
  lhs: HotmealLiveReloadEvent,
  rhs: HotmealLiveReloadEvent,
): boolean {
  if (lhs.tag !== rhs.tag) return false;
  switch (lhs.tag) {
    case "Reload":
      return true;
    case "Patches": {
      const other = rhs as typeof lhs;
      return lhs.route === other.route && sameBytes(lhs.patches_blob, other.patches_blob);
    }
    case "HeadChanged":
      return lhs.route === (rhs as typeof lhs).route;
  }
}

function sampleHelixStreamMetrics(): HelixStreamMetrics {
  return {
    pulse_ids: [101n, 102n, 103n],
    pulse_duration_us: [8100n, 8250n, 8400n],
    decoded_tokens: [4n, 5n, 3n],
    committed_tokens: [2n, 4n, 3n],
    retained_speculative_tokens: [1n, 2n, 1n],
    evicted_audio_tokens: [0n, 16n, 0n],
    evicted_committed_tokens: [0n, 0n, 1n],
    rewind_k: [0n, 2n, 1n],
    ar_token_count: [4n, 6n, 3n],
    rolling_wer: [0.25, 0.20, 0.18],
    s2d_p50_ms: [41.5, 39.0, 37.25],
  };
}

function helixAudioRange(start: number, end: number): HelixAudioTokenRange {
  return { start, end };
}

function sampleHelixVerifyEvidence(): HelixVerifyEvidenceDigest {
  return {
    pulse_id: 102n,
    rewind_k: 2n,
    accepted_prefix_len: 1n,
    divergence_row: 1n,
    drafts: [
      {
        draft_index: 0,
        draft_token_id: 812,
        verified_text_token_id: 44,
        text: "hel",
        status: { tag: "Accepted" },
        expected_observed_audio: helixAudioRange(10, 18),
        max_dominant_audio_mass: 0.73,
        record_count: 8,
        max_logit: 12.5,
        draft_logit: 12.4,
      },
      {
        draft_index: 1,
        draft_token_id: 927,
        verified_text_token_id: 45,
        text: "ix",
        status: { tag: "Divergent" },
        expected_observed_audio: helixAudioRange(18, 26),
        max_dominant_audio_mass: 0.61,
        record_count: 8,
        max_logit: 11.2,
        draft_logit: 9.9,
      },
      {
        draft_index: 2,
        draft_token_id: 415,
        verified_text_token_id: 46,
        text: "",
        status: { tag: "DiscardedAfterDivergence" },
        expected_observed_audio: helixAudioRange(26, 32),
        max_dominant_audio_mass: 0.0,
        record_count: 0,
        max_logit: 0.0,
        draft_logit: 0.0,
      },
    ],
    seed: {
      query_row: 3,
      next_token_seed: 1401,
      expected_observed_audio: helixAudioRange(32, 40),
      max_dominant_audio_mass: 0.58,
      record_count: 8,
      max_logit: 10.75,
    },
  };
}

function sampleHelixPulses(): HelixPulseAvailable[] {
  return [
    { pulse_id: 101n },
    { pulse_id: 102n },
    { pulse_id: 103n },
  ];
}

function sampleHelixPulseBundleFields(): HelixPulseBundleFields {
  return {
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
  };
}

function helixAudioSpan(start: number, end: number, version: number) {
  return {
    audio: helixAudioRange(start, end),
    audio_representation_version: version,
  };
}

function sampleHelixAudioProvenance(): HelixAudioTokenProvenance[] {
  return [
    {
      audio_token_id: 16,
      audio_representation_version: 7,
      mel_frames: [{ start: 128, end: 136 }],
      native_window: 2,
      conv_stem_chunk: 4,
      post_merge_audio_token_id: 16,
      merge: { tag: "NoMerge", pre_merge_audio_token_id: 16 },
      admission: { tag: "AdmitAll", admission_segment: 12 },
      cosine_to_previous: 0.9825,
    },
    {
      audio_token_id: 17,
      audio_representation_version: 7,
      mel_frames: [{ start: 136, end: 144 }, { start: 144, end: 152 }],
      native_window: 2,
      conv_stem_chunk: 4,
      post_merge_audio_token_id: 17,
      merge: { tag: "Merged", pre_merge: helixAudioRange(17, 19) },
      admission: { tag: "AdmitAll", admission_segment: 13 },
      cosine_to_previous: null,
    },
  ];
}

function sampleHelixTimeline(): HelixStreamingTraceEvent[] {
  return [
    {
      tag: "Pulse",
      start_us: 1_000_000n,
      duration_us: 8_250n,
      pulse_id: 102n,
      previous_consumed_mel_frames: 1_632n,
      consumed_mel_frames: 1_648n,
      pulse_mel_frames: 16n,
      committed_text_len_start: 36n,
      speculative_len_start: 3n,
      committed_tokens: 4n,
      retained_speculative_tokens: 2n,
      resident_committed_tokens: 38n,
      evicted_audio_tokens: 16n,
      evicted_committed_tokens: 0n,
    },
    {
      tag: "AudioEncoderUpdate",
      start_us: 1_000_200n,
      duration_us: 2_100n,
      pulse_id: 102n,
      num_audio_frames: 64n,
      first_audio_token_id: 10n,
      resident_audio_frames: 32n,
      changed_span_count: 2n,
      changed_audio_tokens: 8n,
      latest_audio_representation_version: 7n,
    },
    {
      tag: "AudioEviction",
      timestamp_us: 1_000_300n,
      pulse_id: 102n,
      evicted_audio_tokens: 16n,
      first_audio_token_id: 10n,
      resident_audio_frames: 32n,
      audio_ring_capacity: 96n,
    },
    {
      tag: "RefreshPrompt",
      start_us: 1_002_500n,
      duration_us: 1_400n,
      pulse_id: 102n,
      first_audio_token_id: 10n,
      resident_audio_frames: 32n,
      committed_text_len: 36n,
      resident_committed_len: 32n,
      resident_text_len: 35n,
      logical_start: 80n,
      logical_end: 117n,
      text_token_start: 40n,
      text_token_end: 44n,
      spans: [{ logical_start: 80n, rows: 16n, physical_start: 12n }],
    },
    {
      tag: "LayoutSnapshot",
      timestamp_us: 1_003_950n,
      pulse_id: 102n,
      audio_len: 32n,
      audio_head: 4n,
      first_audio_token_id: 10n,
      text_len: 35n,
      first_text_token_id: 40n,
      prompt_len: 67n,
      resident_committed_len: 32n,
      resident_text_len: 35n,
    },
    {
      tag: "Verify",
      start_us: 1_004_000n,
      duration_us: 900n,
      pulse_id: 102n,
      rewind_k: 2n,
      post_rewind_text_len: 37n,
      text_token_start: 44n,
      text_token_end: 47n,
      logical_start: 114n,
      logical_end: 117n,
      spans: [{ logical_start: 114n, rows: 3n, physical_start: 46n }],
      accepted_prefix_len: 1n,
      divergence_row: 1n,
      next_token_seed: 1401n,
      discarded_speculative_tokens: 1n,
      invalidated_speculative_slots: 2n,
    },
    {
      tag: "ArDecode",
      start_us: 1_005_000n,
      duration_us: 2_300n,
      pulse_id: 102n,
      decode_steps: 5n,
      decoded_tokens: 5n,
      speculative_len_entering: 1n,
      live_speculative_tokens: 6n,
      hit_eos: false,
      seed_token_id: 1401n,
      seed_token_text: "hel",
      early_exit_reason: { tag: "BudgetExhausted" },
      next_after_tail: 1502n,
    },
    {
      tag: "ArToken",
      start_us: 1_005_100n,
      duration_us: 300n,
      pulse_id: 102n,
      step_index: 0n,
      input_token_id: 1401n,
      input_text: "hel",
      text_token_id: 47n,
      query_position: 118n,
      physical_start: 49n,
      summary_records: 64n,
      next_token_id: 1502n,
      next_text: "ix",
    },
    {
      tag: "Commit",
      start_us: 1_007_500n,
      duration_us: 250n,
      pulse_id: 102n,
      speculative_len_pre: 6n,
      revisable_tail_target: 2n,
      committed_tokens: 4n,
      retained_speculative_tokens: 2n,
      committed_text_len: 40n,
      next_after_committed: 1502n,
    },
    {
      tag: "VerifySkipped",
      timestamp_us: 1_007_800n,
      pulse_id: 102n,
      reason: { tag: "PreCommitFullRewind" },
      rewind_k: 0n,
      resident_committed_len: 0n,
      speculative_len: 2n,
    },
    {
      tag: "TextEviction",
      timestamp_us: 1_007_900n,
      pulse_id: 102n,
      evicted_committed_tokens: 0n,
      resident_committed_capacity: 128n,
      committed_text_len: 40n,
    },
  ];
}

function sampleHelixPulseBundle(): HelixPulseBundle {
  const provenance = sampleHelixAudioProvenance();
  return {
    pulse_id: 102n,
    schema_version: 1,
    prompt_layout: {
      pulse_id: 102n,
      first_audio_token_id: 10,
      resident_audio_frames: 32n,
      changed_audio_spans: [helixAudioSpan(16, 20, 7), helixAudioSpan(24, 28, 8)],
      text_token_start: 40,
      text_token_end: 44,
      text_tokens: [
        {
          text_token_id: 40,
          text: "hel",
          text_before: "he",
          in_verify_batch: true,
          decoded_this_pulse: false,
        },
        {
          text_token_id: 41,
          text: "ix",
          text_before: null,
          in_verify_batch: false,
          decoded_this_pulse: true,
        },
      ],
    },
    audio_provenance: provenance,
    attention_heatmap: {
      pulse_id: 102n,
      first_audio_token_id: 10,
      audio_token_count: 6,
      text_token_start: 40,
      text_token_count: 2,
      record_count: 16,
      max_value: 0.42,
      mean_audio_mass: [0.02, 0.04, 0.08, 0.16, 0.28, 0.42, 0.03, 0.05, 0.09, 0.15, 0.24, 0.31],
      text_token_glyphs: ["hel", "ix"],
    },
    encoder_frontier: {
      pulse_id: 102n,
      layers: [
        {
          encoder_layer_index: 0,
          points: [
            { audio_token_id: 16, mean_frontier_debt: 0.12, head_count: 4 },
            { audio_token_id: 17, mean_frontier_debt: 0.18, head_count: 4 },
          ],
        },
        {
          encoder_layer_index: 1,
          points: [{ audio_token_id: 16, mean_frontier_debt: 0.09, head_count: 4 }],
        },
      ],
      min_audio_token_id: 16,
      max_audio_token_id: 17,
      min_frontier_debt: 0.09,
      max_frontier_debt: 0.18,
    },
    encoder_provenance: {
      pulse_id: 102n,
      records_checked: 32n,
      violations: [{
        audio_token_id: 18,
        encoder_layer_index: 2,
        head_index: 3,
        observed_audio_token_id: 21,
        kind: { tag: "VersionMismatch" },
        message: "observed audio provenance version lagged refresh",
      }],
    },
    audio_clip: {
      sample_rate: 16000,
      first_sample: 262144n,
      samples: [-0.25, -0.10, 0.0, 0.10, 0.25, 0.50, 0.25, 0.0],
    },
    mel_clip: {
      num_mel_bins: 4,
      first_mel_frame: 128,
      num_mel_frames: 3,
      values: [0.10, 0.20, 0.30, 0.40, 0.15, 0.25, 0.35, 0.45, 0.05, 0.12, 0.18, 0.22],
      min_value: 0.05,
      max_value: 0.45,
      corpus_min_value: -1.25,
      corpus_max_value: 2.75,
    },
    pulse_rollup: {
      pulse_id: 102n,
      pulse_start_us: 1_000_000n,
      pulse_duration_us: 8_250n,
      encoder_duration_us: 2_100n,
      refresh_duration_us: 1_400n,
      verify_duration_us: 900n,
      decode_duration_us: 2_300n,
      commit_duration_us: 250n,
      pulse_mel_frames: 16n,
      committed_tokens: 4n,
      retained_speculative_tokens: 2n,
      resident_committed_tokens: 38n,
      evicted_audio_tokens: 16n,
      evicted_committed_tokens: 0n,
      decoded_tokens: 5n,
      hit_eos: false,
      verify: {
        rewind_k: 2n,
        accepted_prefix_len: 1n,
        divergence_row: 1n,
        discarded_speculative_tokens: 1n,
      },
      has_attention_batch: true,
      ar_token_count: 6n,
    },
    timeline: sampleHelixTimeline(),
    gpu_chrome_events: [
      {
        name: "metal.dispatch",
        cat: "gpu",
        ph: "X",
        ts: 1_006_000.0,
        dur: 420.0,
        pid: 2,
        tid: 7,
        s: null,
        args: new Map<string, Value>(),
      },
      {
        name: "pulse_marker",
        cat: "scheduler",
        ph: "i",
        ts: 1_007_950.0,
        dur: null,
        pid: 1,
        tid: 0,
        s: "p",
        args: new Map<string, Value>(),
      },
    ],
    verify_evidence: sampleHelixVerifyEvidence(),
    scheduler_snapshot: {
      pulse_id: 102n,
      encoder: {
        refreshed_audio: helixAudioRange(16, 18),
        audio_representation_version: 7,
        provenance,
      },
      counts: {
        decode: 1,
        verify_prediction: 1,
        verify_seed: 1,
        prompt_prefill: 1,
      },
      decode: [{
        text_token_id: 47,
        query_position: 118,
        input_token_id: 1401,
        observed_audio: helixAudioRange(10, 18),
      }],
      verify_prediction: [{
        verified_text_token_id: 45,
        verified_draft_index: 1,
        draft_token_id: 927,
        query_row: 2,
        query_position: 116,
        observed_audio: helixAudioRange(18, 26),
      }],
      verify_seed: [{
        query_row: 3,
        query_position: 117,
        next_token_seed: 1401,
        observed_audio: helixAudioRange(32, 40),
      }],
      prompt_prefill: [{
        query_position: 80,
        observed_audio: helixAudioRange(10, 18),
      }],
    },
  };
}

function sampleHelixAudioClip() {
  return {
    sample_rate: 16_000,
    first_sample: 262_144n,
    samples: [-0.25, -0.10, 0.0, 0.10, 0.25, 0.50, 0.25, 0.0],
  };
}

function sampleHelixMelClip() {
  return {
    num_mel_bins: 4,
    first_mel_frame: 128,
    num_mel_frames: 3,
    values: [0.10, 0.20, 0.30, 0.40, 0.15, 0.25, 0.35, 0.45, 0.05, 0.12, 0.18, 0.22],
    min_value: 0.05,
    max_value: 0.45,
    corpus_min_value: -1.25,
    corpus_max_value: 2.75,
  };
}

function sampleHelixChromeEvents() {
  return [{
    name: "metal.dispatch",
    cat: "gpu",
    ph: "X",
    ts: 1_006_000.0,
    dur: 420.0,
    pid: 2,
    tid: 7,
    s: null,
    args: new Map<string, Value>(),
  }];
}

function sampleHelixSupport(): HelixAttentionSupportSummary {
  return {
    total_audio_mass: 0.42,
    observed_audio: helixAudioRange(10, 18),
    dominant_audio: helixAudioRange(16, 18),
    dominant_audio_mass: 0.21,
    center_audio_token: 17.25,
    width_audio_tokens: 3.5,
  };
}

function sampleHelixTextSupport(): HelixTextAttentionSupportRecord[] {
  return [{
    text_token_id: 47,
    query_position: 118,
    decoder_layer_index: 2,
    head_index: 3,
    support: sampleHelixSupport(),
    audio_weights: [0.03125, 0.0625, 0.125, 0.25, 0.5],
  }];
}

function sampleHelixAttentionBatch(): HelixAttentionSummaryBatch {
  return {
    schema_version: 2,
    pulse_id: 102n,
    audio_context_id: 77n,
    text_context_id: 99n,
    audio_representation_spans: [helixAudioSpan(10, 18, 7)],
    changed_audio_representation_spans: [helixAudioSpan(16, 18, 8)],
    text_support: sampleHelixTextSupport(),
    header_text_support: [{
      query_position: 80,
      decoder_layer_index: 1,
      head_index: 0,
      support: sampleHelixSupport(),
      audio_weights: [0.125, 0.25, 0.375, 0.25],
    } satisfies HelixQueryRowAttentionRecord],
    audio_encoder_support: [{
      audio_token_id: 16,
      audio_representation_version: 7,
      encoder_layer_index: 0,
      head_index: 1,
      support: sampleHelixSupport(),
      frontier_debt: 0.125,
    } satisfies HelixAudioEncoderSupportRecord],
    decoder_evidence: [
      {
        text_token_id: 47,
        query_position: 118,
        expected_observed_audio: helixAudioRange(10, 18),
        records: sampleHelixTextSupport(),
        kind: { tag: "Decode", input_token_id: 1401 },
      },
      {
        text_token_id: 45,
        query_position: 116,
        expected_observed_audio: helixAudioRange(18, 26),
        records: sampleHelixTextSupport(),
        kind: {
          tag: "VerifyPrediction",
          verified_draft_index: 1,
          draft_token_id: 927,
          query_row: 2,
          max_logit: 11.25,
          draft_logit: 9.875,
        },
      },
      {
        text_token_id: null,
        query_position: 117,
        expected_observed_audio: helixAudioRange(32, 40),
        records: sampleHelixTextSupport(),
        kind: { tag: "VerifySeed", query_row: 3, next_token_seed: 1401, max_logit: 10.75 },
      },
      {
        text_token_id: null,
        query_position: 80,
        expected_observed_audio: helixAudioRange(10, 18),
        records: sampleHelixTextSupport(),
        kind: { tag: "PromptPrefill" },
      },
    ],
  };
}

function sampleHelixTraceServiceSurface(): HelixTraceServiceSurface {
  const bundle = sampleHelixPulseBundle();
  return {
    meta: {
      schema_version: 2,
      pulse_ids: [101n, 102n],
      timeline_event_count: 420n,
      attention_batch_count: 17n,
    } satisfies HelixStreamMeta,
    pulse_rollup: bundle.pulse_rollup,
    timeline: sampleHelixTimeline(),
    attention_batch: sampleHelixAttentionBatch(),
    prompt_layout: bundle.prompt_layout,
    audio_attended_by: [{
      text_token_id: 47,
      decoder_layer_index: 2,
      head_index: 3,
      dominant_audio_mass: 0.21,
      total_audio_mass: 0.42,
      observed_audio: helixAudioRange(10, 18),
      dominant_audio: helixAudioRange(16, 18),
      audio_weights: [0.03125, 0.0625, 0.125, 0.25, 0.5],
      queried_audio_weight: 0.25,
    } satisfies HelixTextAttendanceRow],
    text_attends_to: [{
      decoder_layer_index: 2,
      head_index: 3,
      dominant_audio_mass: 0.21,
      total_audio_mass: 0.42,
      center_audio_token: 17.25,
      width_audio_tokens: 3.5,
      observed_audio: helixAudioRange(10, 18),
      dominant_audio: helixAudioRange(16, 18),
      audio_weights: [0.03125, 0.0625, 0.125, 0.25, 0.5],
    } satisfies HelixAudioAttendanceRow],
    refresh_attends_to: [{
      query_position: 80,
      decoder_layer_index: 1,
      head_index: 0,
      dominant_audio_mass: 0.375,
      total_audio_mass: 1.0,
      center_audio_token: 15.5,
      width_audio_tokens: 4.0,
      observed_audio: helixAudioRange(10, 18),
      dominant_audio: helixAudioRange(14, 18),
      audio_weights: [0.125, 0.25, 0.375, 0.25],
    } satisfies HelixRefreshAttendanceRow],
    audio_token_provenance: sampleHelixAudioProvenance()[0] ?? null,
    audio_provenance_for_pulse: sampleHelixAudioProvenance(),
    audio_tokens_for_mel_frame: [16, 17],
    audio_clip_for_audio_token: sampleHelixAudioClip(),
    audio_clip_for_prompt: sampleHelixAudioClip(),
    audio_clip_for_audio_range: sampleHelixAudioClip(),
    mel_clip_for_prompt: sampleHelixMelClip(),
    audio_self_attention: [{
      encoder_layer_index: 0,
      head_index: 1,
      audio_representation_version: 7,
      dominant_audio_mass: 0.25,
      total_audio_mass: 0.5,
      center_audio_token: 16.5,
      width_audio_tokens: 2.0,
      observed_audio: helixAudioRange(10, 18),
      dominant_audio: helixAudioRange(16, 18),
      frontier_debt: 0.125,
    } satisfies HelixAudioSelfAttentionRow],
    transcript: [
      { text_token_id: 40, decoded_in_pulse: 101n, text: "hel", committed: true },
      { text_token_id: 41, decoded_in_pulse: 102n, text: "ix", committed: false },
    ] satisfies HelixTranscriptToken[],
    pulse_attention_heatmap: bundle.attention_heatmap,
    encoder_frontier: bundle.encoder_frontier,
    stream_metrics: sampleHelixStreamMetrics(),
    verify_evidence: sampleHelixVerifyEvidence(),
    decoder_evidence_report: {
      total_batches: 7n,
      batches_without_decoder_evidence: 1n,
      pulses_without_decoder_evidence: [101n],
      variant_evidence_counts: {
        decode: 12n,
        verify_prediction: 6n,
        verify_seed: 3n,
        prompt_prefill: 4n,
      } satisfies HelixDecoderEvidenceVariantCounts,
      variant_record_counts: {
        decode: 96n,
        verify_prediction: 48n,
        verify_seed: 24n,
        prompt_prefill: 32n,
      } satisfies HelixDecoderEvidenceVariantCounts,
      observed_decoder_layer_indices: [0, 1, 2],
      observed_decoder_head_indices: [0, 1, 2, 3],
    } satisfies HelixDecoderEvidenceReport,
    pulse_evidence_snapshot: bundle.scheduler_snapshot,
    gpu_chrome_events_for_pulse: sampleHelixChromeEvents(),
    run_info: {
      backend: "metal",
      model_dir: "/models/helix-mini",
      input: "helix fixture",
      piece: "demo",
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
    } satisfies HelixRunInfo,
    piece_eval_reference: {
      piece: "demo",
      language: "en",
      words: ["helix", "fixture"],
    } satisfies HelixPieceEvalReference,
    piece_eval_for_pulse: {
      audio_now_ms: 1234.5,
      reference_words_available: 16,
      hypothesis_words: 15,
      substitutions: 1,
      deletions: 0,
      insertions: 1,
      rolling_wer: 0.125,
      s2d_matched_words: 14,
      s2d_new_words: 2,
      s2d_p50_ms: 41.5,
      s2d_p90_ms: 75.0,
      s2d_p100_ms: 101.25,
      s2d_avg_ms: 50.0,
      audio_frontier: 160,
      displayed_frontier: 156,
      committed_frontier: 152,
      lag_ms: 250.0,
    } satisfies HelixPieceEvalSnapshot,
    encoder_provenance_report: bundle.encoder_provenance,
    pulse_bundle_fields: sampleHelixPulseBundleFields(),
    pulse_bundle: bundle,
    pulse_available: { pulse_id: 102n },
  };
}

function sameBigints(lhs: bigint[], rhs: bigint[]): boolean {
  return lhs.length === rhs.length && lhs.every((value, idx) => rhs[idx] === value);
}

function sameNumbers(lhs: number[], rhs: number[]): boolean {
  return lhs.length === rhs.length && lhs.every((value, idx) => Object.is(value, rhs[idx]));
}

function sameF32(lhs: number, rhs: number): boolean {
  return Math.abs(lhs - rhs) <= 0.000001;
}

function sameHelixDeep(lhs: unknown, rhs: unknown): boolean {
  if (Object.is(lhs, rhs)) return true;
  if (typeof lhs === "number" && typeof rhs === "number") return sameF32(lhs, rhs);
  if (lhs === null || rhs === null) return lhs === rhs;
  if (lhs instanceof Uint8Array || rhs instanceof Uint8Array) {
    return lhs instanceof Uint8Array && rhs instanceof Uint8Array && sameBytes(lhs, rhs);
  }
  if (Array.isArray(lhs) || Array.isArray(rhs)) {
    if (!Array.isArray(lhs) || !Array.isArray(rhs) || lhs.length !== rhs.length) return false;
    return lhs.every((value, idx) => sameHelixDeep(value, rhs[idx]));
  }
  if (lhs instanceof Map || rhs instanceof Map) {
    if (!(lhs instanceof Map) || !(rhs instanceof Map) || lhs.size !== rhs.size) return false;
    for (const [key, value] of lhs) {
      if (!rhs.has(key) || !sameHelixDeep(value, rhs.get(key))) return false;
    }
    return true;
  }
  if (lhs instanceof Set || rhs instanceof Set) {
    if (!(lhs instanceof Set) || !(rhs instanceof Set) || lhs.size !== rhs.size) return false;
    const left = Array.from(lhs).sort();
    const right = Array.from(rhs).sort();
    return left.every((value, idx) => sameHelixDeep(value, right[idx]));
  }
  if (typeof lhs === "object" && typeof rhs === "object") {
    const left = lhs as { [key: string]: unknown };
    const right = rhs as { [key: string]: unknown };
    const leftKeys = Object.keys(left).sort();
    const rightKeys = Object.keys(right).sort();
    if (leftKeys.length !== rightKeys.length) return false;
    return leftKeys.every((key, idx) => key === rightKeys[idx] && sameHelixDeep(left[key], right[key]));
  }
  return false;
}

function sameHelixStreamMetrics(lhs: HelixStreamMetrics, rhs: HelixStreamMetrics): boolean {
  return sameBigints(lhs.pulse_ids, rhs.pulse_ids)
    && sameBigints(lhs.pulse_duration_us, rhs.pulse_duration_us)
    && sameBigints(lhs.decoded_tokens, rhs.decoded_tokens)
    && sameBigints(lhs.committed_tokens, rhs.committed_tokens)
    && sameBigints(lhs.retained_speculative_tokens, rhs.retained_speculative_tokens)
    && sameBigints(lhs.evicted_audio_tokens, rhs.evicted_audio_tokens)
    && sameBigints(lhs.evicted_committed_tokens, rhs.evicted_committed_tokens)
    && sameBigints(lhs.rewind_k, rhs.rewind_k)
    && sameBigints(lhs.ar_token_count, rhs.ar_token_count)
    && sameNumbers(lhs.rolling_wer, rhs.rolling_wer)
    && sameNumbers(lhs.s2d_p50_ms, rhs.s2d_p50_ms);
}

function sameHelixAudioTokenRange(
  lhs: HelixAudioTokenRange,
  rhs: HelixAudioTokenRange,
): boolean {
  return lhs.start === rhs.start && lhs.end === rhs.end;
}

function sameHelixVerifyDraftStatus(
  lhs: HelixVerifyDraftStatus,
  rhs: HelixVerifyDraftStatus,
): boolean {
  return lhs.tag === rhs.tag;
}

function sameHelixVerifyDraftRow(
  lhs: HelixVerifyDraftRow,
  rhs: HelixVerifyDraftRow,
): boolean {
  return lhs.draft_index === rhs.draft_index
    && lhs.draft_token_id === rhs.draft_token_id
    && lhs.verified_text_token_id === rhs.verified_text_token_id
    && lhs.text === rhs.text
    && sameHelixVerifyDraftStatus(lhs.status, rhs.status)
    && sameHelixAudioTokenRange(lhs.expected_observed_audio, rhs.expected_observed_audio)
    && sameF32(lhs.max_dominant_audio_mass, rhs.max_dominant_audio_mass)
    && lhs.record_count === rhs.record_count
    && sameF32(lhs.max_logit, rhs.max_logit)
    && sameF32(lhs.draft_logit, rhs.draft_logit);
}

function sameHelixVerifySeedRow(
  lhs: HelixVerifySeedRow | null,
  rhs: HelixVerifySeedRow | null,
): boolean {
  if (lhs === null || rhs === null) return lhs === rhs;
  return lhs.query_row === rhs.query_row
    && lhs.next_token_seed === rhs.next_token_seed
    && sameHelixAudioTokenRange(lhs.expected_observed_audio, rhs.expected_observed_audio)
    && sameF32(lhs.max_dominant_audio_mass, rhs.max_dominant_audio_mass)
    && lhs.record_count === rhs.record_count
    && sameF32(lhs.max_logit, rhs.max_logit);
}

function sameHelixVerifyEvidenceDigest(
  lhs: HelixVerifyEvidenceDigest,
  rhs: HelixVerifyEvidenceDigest,
): boolean {
  return lhs.pulse_id === rhs.pulse_id
    && lhs.rewind_k === rhs.rewind_k
    && lhs.accepted_prefix_len === rhs.accepted_prefix_len
    && lhs.divergence_row === rhs.divergence_row
    && lhs.drafts.length === rhs.drafts.length
    && lhs.drafts.every((draft, idx) => sameHelixVerifyDraftRow(draft, rhs.drafts[idx] as HelixVerifyDraftRow))
    && sameHelixVerifySeedRow(lhs.seed, rhs.seed);
}

function sameHelixPulses(lhs: HelixPulseAvailable[], rhs: HelixPulseAvailable[]): boolean {
  return lhs.length === rhs.length
    && lhs.every((pulse, idx) => pulse.pulse_id === rhs[idx]?.pulse_id);
}

function sameHelixPulseBundle(lhs: HelixPulseBundle, rhs: HelixPulseBundle): boolean {
  return sameHelixDeep(lhs, rhs);
}

function sameHelixTraceServiceSurface(lhs: HelixTraceServiceSurface, rhs: HelixTraceServiceSurface): boolean {
  return sameHelixDeep(lhs, rhs);
}

function traceyRuleId(base: string, version: number): TraceyRuleId {
  return { base, version };
}

function sampleTraceyStatusResponse(): TraceyStatusResponse {
  return {
    impls: [
      {
        spec: "vox",
        impl_name: "rust",
        total_rules: 59n,
        covered_rules: 59n,
        stale_rules: 0n,
        verified_rules: 59n,
      },
      {
        spec: "vox",
        impl_name: "typescript",
        total_rules: 173n,
        covered_rules: 173n,
        stale_rules: 0n,
        verified_rules: 100n,
      },
    ],
  };
}

function sampleTraceyQueryRequest(): TraceyUncoveredRequest {
  return {
    spec: "vox",
    impl_name: "rust",
    prefix: "rpc.channel",
  };
}

function sampleTraceyUntestedRequest(): TraceyUntestedRequest {
  return {
    spec: "vox",
    impl_name: "rust",
    prefix: "rpc.channel",
  };
}

function sampleTraceyStaleRequest(): TraceyStaleRequest {
  return {
    spec: "vox",
    impl_name: "rust",
    prefix: "rpc.channel",
  };
}

function sampleTraceyUnmappedRequest(): TraceyUnmappedRequest {
  return {
    spec: "vox",
    impl_name: "rust",
    path: "rust/vox-codegen/src",
  };
}

function sampleTraceySectionRules(): TraceySectionRules[] {
  return [
    {
      section: "Channel Binding",
      rules: [
        {
          id: traceyRuleId("rpc.channel.direct-args", 1),
          text: "Channels are direct service arguments.",
        },
        {
          id: traceyRuleId("rpc.channel.no-collections", 1),
          text: null,
        },
      ],
    },
  ];
}

function sampleTraceyUncoveredResponse(): TraceyUncoveredResponse {
  return {
    spec: "vox",
    impl_name: "rust",
    total_rules: 175n,
    uncovered_count: 2n,
    by_section: sampleTraceySectionRules(),
  };
}

function sampleTraceyUntestedResponse(): TraceyUntestedResponse {
  return {
    spec: "vox",
    impl_name: "rust",
    total_rules: 175n,
    untested_count: 3n,
    by_section: sampleTraceySectionRules(),
  };
}

function sampleTraceyStaleResponse(): TraceyStaleResponse {
  return {
    spec: "vox",
    impl_name: "rust",
    total_rules: 175n,
    stale_count: 1n,
    refs: [
      {
        current_id: traceyRuleId("rpc.channel.direct-args", 2),
        file: "rust/vox-codegen/src/targets/swift/mod.rs",
        line: 67n,
        reference_id: traceyRuleId("rpc.channel.direct-args", 1),
      },
    ],
  };
}

function sampleTraceyUnmappedResponse(): TraceyUnmappedResponse {
  return {
    spec: "vox",
    impl_name: "rust",
    total_units: 9n,
    unmapped_count: 2n,
    entries: [
      {
        path: "rust/vox-codegen/src/targets",
        is_dir: true,
        total_units: 5n,
        unmapped_units: 1n,
        units: [],
      },
      {
        path: "rust/vox-codegen/src/targets/swift/mod.rs",
        is_dir: false,
        total_units: 4n,
        unmapped_units: 1n,
        units: [
          {
            kind: "function",
            name: "emit_tracey_bridge",
            start_line: 41n,
            end_line: 78n,
          },
        ],
      },
    ],
  };
}

function sampleTraceyApiConfig(): TraceyApiConfig {
  return {
    project_root: "/workspace/vox",
    specs: [
      {
        name: "vox",
        prefix: "r",
        source: "docs/content/spec/*.md",
        source_url: "https://vixen.rs/vox/spec",
        implementations: ["rust", "swift", "typescript"],
      },
    ],
  };
}

function sampleTraceyReloadResponse(): TraceyReloadResponse {
  return {
    version: 13n,
    rebuild_time_ms: 42n,
  };
}

function sampleTraceyHealthResponse(): TraceyHealthResponse {
  return {
    version: 13n,
    watcher_active: true,
    watcher_error: null,
    config_error: "ignored include pattern failed to parse",
    watcher_last_event_ms: 1717000000123n,
    watcher_event_count: 7n,
    watched_directories: ["docs/content/spec", "rust"],
    uptime_secs: 3600n,
  };
}

function sampleTraceyRuleInfo(): TraceyRuleInfo {
  return {
    id: traceyRuleId("rpc.channel.direct-args", 1),
    raw: "Channels are direct service arguments.",
    html: "<p>Channels are direct service arguments.</p>",
    source_file: "docs/content/spec/vox.md",
    source_line: 42n,
    coverage: [
      {
        spec: "vox",
        impl_name: "rust",
        impl_refs: [
          {
            file: "rust/vox-codegen/src/targets/swift/mod.rs",
            line: 67n,
          },
        ],
        verify_refs: [
          {
            file: "spec/spec-tests/tests/cases/testbed.rs",
            line: 1450n,
          },
        ],
      },
    ],
    version_diff: "Added direct argument wording.",
  };
}

function sampleTraceyForwardResponse(): TraceyApiSpecForward {
  const staleRef: TraceyApiStaleRef = {
    file: "swift/subject/Sources/subject-swift/Subject.swift",
    line: 549n,
    reference_id: traceyRuleId("rpc.channel.direct-args", 1),
  };
  const rule: TraceyApiRule = {
    id: traceyRuleId("rpc.channel.direct-args", 2),
    raw: "Channels are direct service arguments.",
    html: "<p>Channels are direct service arguments.</p>",
    status: "stable",
    level: "must",
    source_file: "docs/content/spec/rpc.md",
    source_line: 42n,
    source_column: 3n,
    section: "channel-binding",
    section_title: "Channel Binding",
    impl_refs: [
      {
        file: "rust/vox-codegen/src/targets/typescript/mod.rs",
        line: 128n,
      },
    ],
    verify_refs: [
      {
        file: "spec/spec-tests/tests/cases/testbed.rs",
        line: 3662n,
      },
    ],
    depends_refs: [
      {
        file: "docs/content/guides/typescript.md",
        line: 18n,
      },
    ],
    is_stale: true,
    stale_refs: [staleRef],
  };
  return {
    name: "vox",
    rules: [rule],
  };
}

function sampleTraceyReverseResponse(): TraceyApiReverseData {
  const files: TraceyApiFileEntry[] = [
    {
      path: "rust/vox-codegen/src/targets/typescript/mod.rs",
      total_units: 4n,
      covered_units: 3n,
    },
    {
      path: "swift/subject/Sources/subject-swift/Subject.swift",
      total_units: 3n,
      covered_units: 2n,
    },
  ];
  return {
    total_units: 7n,
    covered_units: 5n,
    files,
  };
}

function sampleTraceyFileRequest(): TraceyFileRequest {
  return {
    spec: "vox",
    impl_name: "rust",
    path: "rust/vox-codegen/src/targets/typescript/mod.rs",
  };
}

function sampleTraceyFileResponse(): TraceyApiFileData {
  const unit: TraceyApiCodeUnit = {
    kind: "function",
    name: "emit_tracey_dashboard_bridge",
    start_line: 1n,
    end_line: 1n,
    rule_refs: ["rpc.channel.direct-args", "encoding.struct"],
  };
  return {
    path: "rust/vox-codegen/src/targets/typescript/mod.rs",
    content: "fn emit_tracey_dashboard_bridge() {}\n",
    html: "<pre><span>fn emit_tracey_dashboard_bridge() {}</span></pre>",
    units: [unit],
  };
}

function sampleTraceySpecContentResponse(): TraceyApiSpecData {
  const direct: TraceyOutlineCoverage = {
    impl_count: 1n,
    verify_count: 1n,
    total: 2n,
  };
  const aggregate: TraceyOutlineCoverage = {
    impl_count: 3n,
    verify_count: 2n,
    total: 4n,
  };
  const outline: TraceyOutlineEntry = {
    title: "Channel Binding",
    slug: "channel-binding",
    level: 2,
    coverage: direct,
    aggregated: aggregate,
  };
  const section: TraceySpecSection = {
    source_file: "docs/content/spec/rpc.md",
    html: "<h2 id=\"channel-binding\">Channel Binding</h2>",
    weight: 20,
  };
  return {
    name: "vox",
    sections: [section],
    outline: [outline],
    head_injections: ["<script type=\"module\">mermaid.initialize({});</script>"],
  };
}

function sampleTraceySearchResults(): TraceySearchResult[] {
  return [
    {
      kind: "rule",
      id: "rpc.channel.direct-args",
      line: 0n,
      content: "Channels are direct service arguments.",
      highlighted: "<mark>channel</mark> direct args",
      score: 12.5,
    },
    {
      kind: "source",
      id: "rust/vox-codegen/src/targets/typescript/mod.rs",
      line: 128n,
      content: "// r[impl rpc.channel.direct-args]",
      highlighted: null,
      score: 7.25,
    },
  ];
}

function sampleTraceyUpdateFileRangeRequest(): TraceyUpdateFileRangeRequest {
  return {
    path: "docs/content/spec/rpc.md",
    start: 120n,
    end: 144n,
    content: "Channels are direct service arguments.",
    file_hash: "sha256:tracey-dashboard-ok",
  };
}

function sampleTraceyUpdateFileRangeConflictRequest(): TraceyUpdateFileRangeRequest {
  return {
    ...sampleTraceyUpdateFileRangeRequest(),
    file_hash: "stale",
  };
}

function sampleTraceyUpdateError(): TraceyUpdateError {
  return {
    message: "file changed on disk",
  };
}

function sampleTraceyConfigPatternRequest(): TraceyConfigPatternRequest {
  return {
    spec: "vox",
    impl_name: "typescript",
    pattern: "typescript/**/*.generated.ts",
  };
}

function sampleTraceyBadConfigPatternRequest(): TraceyConfigPatternRequest {
  return {
    ...sampleTraceyConfigPatternRequest(),
    pattern: "bad[glob",
  };
}

function sampleTraceyValidateRequest(): TraceyValidateRequest {
  return {
    spec: "vox",
    impl_name: "rust",
  };
}

function sampleTraceyValidationResult(): TraceyValidationResult {
  return {
    spec: "vox",
    impl_name: "rust",
    errors: [
      {
        code: { tag: "StaleRequirement" },
        message: "reference points to an older rule version",
        file: "rust/subject-rust/src/lib.rs",
        line: 12n,
        column: 9n,
        related_rules: [traceyRuleId("rpc.channel.direct-args", 2)],
        reference_rule_id: traceyRuleId("rpc.channel.direct-args", 1),
        reference_text: "r[impl rpc.channel.direct-args]",
      },
      {
        code: { tag: "UnknownRequirement" },
        message: "unknown requirement",
        file: null,
        line: null,
        column: null,
        related_rules: [],
        reference_rule_id: null,
        reference_text: "r[verify typo.rule]",
      },
    ],
    warning_count: 1n,
    error_count: 1n,
  };
}

function sampleTraceyLspContent(): string {
  return "// r[impl rpc.channel.direct-args]\nfn main() {}\n";
}

function sampleTraceyLspPositionRequest(): TraceyLspPositionRequest {
  return {
    path: "src/lib.rs",
    content: sampleTraceyLspContent(),
    line: 0,
    character: 8,
  };
}

function sampleTraceyLspReferencesRequest(): TraceyLspReferencesRequest {
  return {
    path: "src/lib.rs",
    content: sampleTraceyLspContent(),
    line: 0,
    character: 8,
    include_declaration: true,
  };
}

function sampleTraceyLspDocumentRequest(): TraceyLspDocumentRequest {
  return {
    path: "src/lib.rs",
    content: sampleTraceyLspContent(),
  };
}

function sampleTraceyLspInlayHintsRequest(): TraceyLspInlayHintsRequest {
  return {
    path: "src/lib.rs",
    content: sampleTraceyLspContent(),
    start_line: 0,
    end_line: 2,
  };
}

function sampleTraceyLspRenameRequest(): TraceyLspRenameRequest {
  return {
    path: "src/lib.rs",
    content: sampleTraceyLspContent(),
    line: 0,
    character: 8,
    new_name: "rpc.channel.direct-args-renamed",
  };
}

function sampleTraceyLspLocations(): TraceyLspLocation[] {
  return [
    {
      path: "docs/content/spec/rpc.md",
      line: 211,
      character: 3,
    },
    {
      path: "spec/spec-tests/tests/cases/testbed.rs",
      line: 1450,
      character: 6,
    },
  ];
}

function sampleTraceyHoverInfo(): TraceyHoverInfo {
  return {
    rule_id: traceyRuleId("rpc.channel.direct-args", 1),
    raw: "Channels are direct service arguments.",
    spec_name: "vox",
    spec_url: "https://vixen.rs/vox/spec/rpc",
    source_file: "docs/content/spec/rpc.md",
    impl_count: 1n,
    verify_count: 1n,
    impl_refs: [
      {
        file: "rust/vox-codegen/src/targets/swift/mod.rs",
        line: 67n,
      },
    ],
    verify_refs: [
      {
        file: "spec/spec-tests/tests/cases/testbed.rs",
        line: 1450n,
      },
    ],
    range_start_line: 0,
    range_start_char: 3,
    range_end_line: 0,
    range_end_char: 36,
    version_diff: "Added direct argument wording.",
  };
}

function sampleTraceyLspCompletions(): TraceyLspCompletionItem[] {
  return [
    {
      label: "impl",
      kind: "verb",
      detail: "implementation reference",
      documentation: null,
      insert_text: "impl ",
    },
    {
      label: "rpc.channel.direct-args",
      kind: "rule",
      detail: "vox",
      documentation: "Channels are direct service arguments.",
      insert_text: null,
    },
  ];
}

function sampleTraceyLspWorkspaceDiagnostics(): TraceyLspFileDiagnostics[] {
  return [
    {
      path: "src/lib.rs",
      diagnostics: [
        {
          severity: "warning",
          code: "stale_requirement",
          message: "reference points to an older rule version",
          start_line: 7,
          start_char: 4,
          end_line: 7,
          end_char: 41,
        },
      ],
    },
  ];
}

function sampleTraceyLspSymbols(): TraceyLspSymbol[] {
  return [
    {
      name: "rpc.channel.direct-args",
      kind: "impl",
      path: "src/lib.rs",
      start_line: 0,
      start_char: 3,
      end_line: 0,
      end_char: 36,
    },
    {
      name: "rpc.channel.no-collections",
      kind: "verify",
      path: "spec/spec-tests/tests/cases/testbed.rs",
      start_line: 1450,
      start_char: 6,
      end_line: 1450,
      end_char: 41,
    },
  ];
}

function sampleTraceyLspSemanticTokens(): TraceyLspSemanticToken[] {
  return [
    {
      line: 0,
      start_char: 3,
      length: 4,
      token_type: 0,
      modifiers: 0,
    },
    {
      line: 0,
      start_char: 8,
      length: 23,
      token_type: 1,
      modifiers: 2,
    },
  ];
}

function sampleTraceyLspCodeLens(): TraceyLspCodeLens[] {
  return [
    {
      line: 0,
      start_char: 3,
      end_char: 36,
      title: "1 impl, 1 verify",
      command: "tracey.showRule",
      arguments: ["rpc.channel.direct-args"],
    },
  ];
}

function sampleTraceyLspInlayHints(): TraceyLspInlayHint[] {
  return [
    {
      line: 0,
      character: 36,
      label: "covered",
    },
  ];
}

function sampleTraceyPrepareRenameResult(): TraceyPrepareRenameResult {
  return {
    start_line: 0,
    start_char: 8,
    end_line: 0,
    end_char: 31,
    placeholder: "rpc.channel.direct-args",
  };
}

function sampleTraceyLspTextEdits(): TraceyLspTextEdit[] {
  return [
    {
      path: "src/lib.rs",
      start_line: 0,
      start_char: 8,
      end_line: 0,
      end_char: 31,
      new_text: "rpc.channel.direct-args-renamed",
    },
    {
      path: "docs/content/spec/rpc.md",
      start_line: 211,
      start_char: 3,
      end_line: 211,
      end_char: 26,
      new_text: "rpc.channel.direct-args-renamed",
    },
  ];
}

function sampleTraceyLspCodeActions(): TraceyLspCodeAction[] {
  return [
    {
      title: "Open requirement",
      kind: "quickfix",
      command: "tracey.openRule",
      arguments: ["rpc.channel.direct-args"],
      is_preferred: true,
    },
  ];
}

function sampleTraceyUpdates(): TraceyDataUpdate[] {
  return [
    {
      version: 11n,
      delta: null,
    },
    {
      version: 12n,
      delta: {
        newly_covered: [
          {
            rule_id: traceyRuleId("rpc.channel.direct-args", 1),
            file: "rust/vox-codegen/src/targets/swift/mod.rs",
            line: 67n,
          },
        ],
        newly_uncovered: [traceyRuleId("rpc.channel.no-collections", 1)],
      },
    },
  ];
}

function sameTraceyRuleId(lhs: TraceyRuleId, rhs: TraceyRuleId): boolean {
  return lhs.base === rhs.base && lhs.version === rhs.version;
}

function sameTraceyCodeRef(lhs: TraceyCodeRef, rhs: TraceyCodeRef): boolean {
  return lhs.file === rhs.file && lhs.line === rhs.line;
}

function sameTraceyImplStatus(lhs: TraceyImplStatus, rhs: TraceyImplStatus): boolean {
  return lhs.spec === rhs.spec
    && lhs.impl_name === rhs.impl_name
    && lhs.total_rules === rhs.total_rules
    && lhs.covered_rules === rhs.covered_rules
    && lhs.stale_rules === rhs.stale_rules
    && lhs.verified_rules === rhs.verified_rules;
}

function sameTraceyStatusResponse(lhs: TraceyStatusResponse, rhs: TraceyStatusResponse): boolean {
  return lhs.impls.length === rhs.impls.length
    && lhs.impls.every((entry, idx) => sameTraceyImplStatus(entry, rhs.impls[idx] as TraceyImplStatus));
}

function sameTraceyUncoveredRequest(
  lhs: TraceyUncoveredRequest,
  rhs: TraceyUncoveredRequest,
): boolean {
  return lhs.spec === rhs.spec && lhs.impl_name === rhs.impl_name && lhs.prefix === rhs.prefix;
}

function sameTraceyUntestedRequest(lhs: TraceyUntestedRequest, rhs: TraceyUntestedRequest): boolean {
  return lhs.spec === rhs.spec && lhs.impl_name === rhs.impl_name && lhs.prefix === rhs.prefix;
}

function sameTraceyStaleRequest(lhs: TraceyStaleRequest, rhs: TraceyStaleRequest): boolean {
  return lhs.spec === rhs.spec && lhs.impl_name === rhs.impl_name && lhs.prefix === rhs.prefix;
}

function sameTraceyUnmappedRequest(
  lhs: TraceyUnmappedRequest,
  rhs: TraceyUnmappedRequest,
): boolean {
  return lhs.spec === rhs.spec && lhs.impl_name === rhs.impl_name && lhs.path === rhs.path;
}

function sameTraceyRuleRef(lhs: TraceyRuleRef, rhs: TraceyRuleRef): boolean {
  return sameTraceyRuleId(lhs.id, rhs.id) && lhs.text === rhs.text;
}

function sameTraceySectionRules(lhs: TraceySectionRules[], rhs: TraceySectionRules[]): boolean {
  return lhs.length === rhs.length
    && lhs.every((section, idx) => {
      const other = rhs[idx] as TraceySectionRules;
      return section.section === other.section
        && section.rules.length === other.rules.length
        && section.rules.every((rule, ruleIdx) =>
          sameTraceyRuleRef(rule, other.rules[ruleIdx] as TraceyRuleRef)
        );
    });
}

function sameTraceyUncoveredResponse(
  lhs: TraceyUncoveredResponse,
  rhs: TraceyUncoveredResponse,
): boolean {
  return lhs.spec === rhs.spec
    && lhs.impl_name === rhs.impl_name
    && lhs.total_rules === rhs.total_rules
    && lhs.uncovered_count === rhs.uncovered_count
    && sameTraceySectionRules(lhs.by_section, rhs.by_section);
}

function sameTraceyUntestedResponse(
  lhs: TraceyUntestedResponse,
  rhs: TraceyUntestedResponse,
): boolean {
  return lhs.spec === rhs.spec
    && lhs.impl_name === rhs.impl_name
    && lhs.total_rules === rhs.total_rules
    && lhs.untested_count === rhs.untested_count
    && sameTraceySectionRules(lhs.by_section, rhs.by_section);
}

function sameTraceyStaleEntry(lhs: TraceyStaleEntry, rhs: TraceyStaleEntry): boolean {
  return sameTraceyRuleId(lhs.current_id, rhs.current_id)
    && lhs.file === rhs.file
    && lhs.line === rhs.line
    && sameTraceyRuleId(lhs.reference_id, rhs.reference_id);
}

function sameTraceyStaleResponse(lhs: TraceyStaleResponse, rhs: TraceyStaleResponse): boolean {
  return lhs.spec === rhs.spec
    && lhs.impl_name === rhs.impl_name
    && lhs.total_rules === rhs.total_rules
    && lhs.stale_count === rhs.stale_count
    && lhs.refs.length === rhs.refs.length
    && lhs.refs.every((entry, idx) => sameTraceyStaleEntry(entry, rhs.refs[idx] as TraceyStaleEntry));
}

function sameTraceyUnmappedUnit(lhs: TraceyUnmappedUnit, rhs: TraceyUnmappedUnit): boolean {
  return lhs.kind === rhs.kind
    && lhs.name === rhs.name
    && lhs.start_line === rhs.start_line
    && lhs.end_line === rhs.end_line;
}

function sameTraceyUnmappedEntry(lhs: TraceyUnmappedEntry, rhs: TraceyUnmappedEntry): boolean {
  return lhs.path === rhs.path
    && lhs.is_dir === rhs.is_dir
    && lhs.total_units === rhs.total_units
    && lhs.unmapped_units === rhs.unmapped_units
    && lhs.units.length === rhs.units.length
    && lhs.units.every((unit, idx) => sameTraceyUnmappedUnit(unit, rhs.units[idx] as TraceyUnmappedUnit));
}

function sameTraceyUnmappedResponse(
  lhs: TraceyUnmappedResponse,
  rhs: TraceyUnmappedResponse,
): boolean {
  return lhs.spec === rhs.spec
    && lhs.impl_name === rhs.impl_name
    && lhs.total_units === rhs.total_units
    && lhs.unmapped_count === rhs.unmapped_count
    && lhs.entries.length === rhs.entries.length
    && lhs.entries.every((entry, idx) =>
      sameTraceyUnmappedEntry(entry, rhs.entries[idx] as TraceyUnmappedEntry)
    );
}

function sameTraceyApiSpecInfo(lhs: TraceyApiSpecInfo, rhs: TraceyApiSpecInfo): boolean {
  return lhs.name === rhs.name
    && lhs.prefix === rhs.prefix
    && lhs.source === rhs.source
    && lhs.source_url === rhs.source_url
    && sameStringArray(lhs.implementations, rhs.implementations);
}

function sameTraceyApiConfig(lhs: TraceyApiConfig, rhs: TraceyApiConfig): boolean {
  return lhs.project_root === rhs.project_root
    && lhs.specs.length === rhs.specs.length
    && lhs.specs.every((spec, idx) => sameTraceyApiSpecInfo(spec, rhs.specs[idx] as TraceyApiSpecInfo));
}

function sameTraceyReloadResponse(lhs: TraceyReloadResponse, rhs: TraceyReloadResponse): boolean {
  return lhs.version === rhs.version && lhs.rebuild_time_ms === rhs.rebuild_time_ms;
}

function sameTraceyHealthResponse(lhs: TraceyHealthResponse, rhs: TraceyHealthResponse): boolean {
  return lhs.version === rhs.version
    && lhs.watcher_active === rhs.watcher_active
    && lhs.watcher_error === rhs.watcher_error
    && lhs.config_error === rhs.config_error
    && lhs.watcher_last_event_ms === rhs.watcher_last_event_ms
    && lhs.watcher_event_count === rhs.watcher_event_count
    && sameStringArray(lhs.watched_directories, rhs.watched_directories)
    && lhs.uptime_secs === rhs.uptime_secs;
}

function sameTraceyRuleCoverage(lhs: TraceyRuleCoverage, rhs: TraceyRuleCoverage): boolean {
  return lhs.spec === rhs.spec
    && lhs.impl_name === rhs.impl_name
    && lhs.impl_refs.length === rhs.impl_refs.length
    && lhs.verify_refs.length === rhs.verify_refs.length
    && lhs.impl_refs.every((entry, idx) => sameTraceyCodeRef(entry, rhs.impl_refs[idx] as TraceyCodeRef))
    && lhs.verify_refs.every((entry, idx) => sameTraceyCodeRef(entry, rhs.verify_refs[idx] as TraceyCodeRef));
}

function sameTraceyRuleInfo(lhs: TraceyRuleInfo | null, rhs: TraceyRuleInfo | null): boolean {
  if (lhs === null || rhs === null) return lhs === rhs;
  return sameTraceyRuleId(lhs.id, rhs.id)
    && lhs.raw === rhs.raw
    && lhs.html === rhs.html
    && lhs.source_file === rhs.source_file
    && lhs.source_line === rhs.source_line
    && lhs.coverage.length === rhs.coverage.length
    && lhs.coverage.every((entry, idx) => sameTraceyRuleCoverage(entry, rhs.coverage[idx] as TraceyRuleCoverage))
    && lhs.version_diff === rhs.version_diff;
}

function traceyStableString(value: unknown): string {
  return JSON.stringify(value, (_key, val) => typeof val === "bigint" ? `${val}n` : val) ?? "undefined";
}

function sameTraceyDashboardValue<T>(lhs: T, rhs: T): boolean {
  return traceyStableString(lhs) === traceyStableString(rhs);
}

function sameTraceyValidationErrorCode(
  lhs: TraceyValidationErrorCode,
  rhs: TraceyValidationErrorCode,
): boolean {
  return lhs.tag === rhs.tag;
}

function sameTraceyValidationError(
  lhs: TraceyValidationError,
  rhs: TraceyValidationError,
): boolean {
  const sameReferenceRule = lhs.reference_rule_id === null || rhs.reference_rule_id === null
    ? lhs.reference_rule_id === rhs.reference_rule_id
    : sameTraceyRuleId(lhs.reference_rule_id, rhs.reference_rule_id);
  return sameTraceyValidationErrorCode(lhs.code, rhs.code)
    && lhs.message === rhs.message
    && lhs.file === rhs.file
    && lhs.line === rhs.line
    && lhs.column === rhs.column
    && lhs.related_rules.length === rhs.related_rules.length
    && lhs.related_rules.every((rule, idx) => sameTraceyRuleId(rule, rhs.related_rules[idx] as TraceyRuleId))
    && sameReferenceRule
    && lhs.reference_text === rhs.reference_text;
}

function sameTraceyValidationResult(
  lhs: TraceyValidationResult,
  rhs: TraceyValidationResult,
): boolean {
  return lhs.spec === rhs.spec
    && lhs.impl_name === rhs.impl_name
    && lhs.warning_count === rhs.warning_count
    && lhs.error_count === rhs.error_count
    && lhs.errors.length === rhs.errors.length
    && lhs.errors.every((entry, idx) => sameTraceyValidationError(entry, rhs.errors[idx] as TraceyValidationError));
}

function sameTraceyLspPositionRequest(
  lhs: TraceyLspPositionRequest,
  rhs: TraceyLspPositionRequest,
): boolean {
  return lhs.path === rhs.path
    && lhs.content === rhs.content
    && lhs.line === rhs.line
    && lhs.character === rhs.character;
}

function sameTraceyLspReferencesRequest(
  lhs: TraceyLspReferencesRequest,
  rhs: TraceyLspReferencesRequest,
): boolean {
  return lhs.path === rhs.path
    && lhs.content === rhs.content
    && lhs.line === rhs.line
    && lhs.character === rhs.character
    && lhs.include_declaration === rhs.include_declaration;
}

function sameTraceyLspDocumentRequest(
  lhs: TraceyLspDocumentRequest,
  rhs: TraceyLspDocumentRequest,
): boolean {
  return lhs.path === rhs.path && lhs.content === rhs.content;
}

function sameTraceyLspInlayHintsRequest(
  lhs: TraceyLspInlayHintsRequest,
  rhs: TraceyLspInlayHintsRequest,
): boolean {
  return lhs.path === rhs.path
    && lhs.content === rhs.content
    && lhs.start_line === rhs.start_line
    && lhs.end_line === rhs.end_line;
}

function sameTraceyLspRenameRequest(
  lhs: TraceyLspRenameRequest,
  rhs: TraceyLspRenameRequest,
): boolean {
  return lhs.path === rhs.path
    && lhs.content === rhs.content
    && lhs.line === rhs.line
    && lhs.character === rhs.character
    && lhs.new_name === rhs.new_name;
}

function sameTraceyLspLocation(lhs: TraceyLspLocation, rhs: TraceyLspLocation): boolean {
  return lhs.path === rhs.path && lhs.line === rhs.line && lhs.character === rhs.character;
}

function sameTraceyLspLocations(lhs: TraceyLspLocation[], rhs: TraceyLspLocation[]): boolean {
  return lhs.length === rhs.length
    && lhs.every((entry, idx) => sameTraceyLspLocation(entry, rhs[idx] as TraceyLspLocation));
}

function sameTraceyHoverInfo(lhs: TraceyHoverInfo | null, rhs: TraceyHoverInfo | null): boolean {
  if (lhs === null || rhs === null) return lhs === rhs;
  return sameTraceyRuleId(lhs.rule_id, rhs.rule_id)
    && lhs.raw === rhs.raw
    && lhs.spec_name === rhs.spec_name
    && lhs.spec_url === rhs.spec_url
    && lhs.source_file === rhs.source_file
    && lhs.impl_count === rhs.impl_count
    && lhs.verify_count === rhs.verify_count
    && lhs.impl_refs.length === rhs.impl_refs.length
    && lhs.verify_refs.length === rhs.verify_refs.length
    && lhs.impl_refs.every((entry, idx) => sameTraceyCodeRef(entry, rhs.impl_refs[idx] as TraceyCodeRef))
    && lhs.verify_refs.every((entry, idx) => sameTraceyCodeRef(entry, rhs.verify_refs[idx] as TraceyCodeRef))
    && lhs.range_start_line === rhs.range_start_line
    && lhs.range_start_char === rhs.range_start_char
    && lhs.range_end_line === rhs.range_end_line
    && lhs.range_end_char === rhs.range_end_char
    && lhs.version_diff === rhs.version_diff;
}

function sameTraceyLspCompletionItem(
  lhs: TraceyLspCompletionItem,
  rhs: TraceyLspCompletionItem,
): boolean {
  return lhs.label === rhs.label
    && lhs.kind === rhs.kind
    && lhs.detail === rhs.detail
    && lhs.documentation === rhs.documentation
    && lhs.insert_text === rhs.insert_text;
}

function sameTraceyLspCompletions(
  lhs: TraceyLspCompletionItem[],
  rhs: TraceyLspCompletionItem[],
): boolean {
  return lhs.length === rhs.length
    && lhs.every((entry, idx) =>
      sameTraceyLspCompletionItem(entry, rhs[idx] as TraceyLspCompletionItem)
    );
}

function sameTraceyLspDiagnostic(lhs: TraceyLspDiagnostic, rhs: TraceyLspDiagnostic): boolean {
  return lhs.severity === rhs.severity
    && lhs.code === rhs.code
    && lhs.message === rhs.message
    && lhs.start_line === rhs.start_line
    && lhs.start_char === rhs.start_char
    && lhs.end_line === rhs.end_line
    && lhs.end_char === rhs.end_char;
}

function sameTraceyLspFileDiagnostics(
  lhs: TraceyLspFileDiagnostics[],
  rhs: TraceyLspFileDiagnostics[],
): boolean {
  return lhs.length === rhs.length
    && lhs.every((file, idx) => {
      const other = rhs[idx] as TraceyLspFileDiagnostics;
      return file.path === other.path
        && file.diagnostics.length === other.diagnostics.length
        && file.diagnostics.every((diag, diagIdx) =>
          sameTraceyLspDiagnostic(diag, other.diagnostics[diagIdx] as TraceyLspDiagnostic)
        );
    });
}

function sameTraceyLspSymbol(lhs: TraceyLspSymbol, rhs: TraceyLspSymbol): boolean {
  return lhs.name === rhs.name
    && lhs.kind === rhs.kind
    && lhs.path === rhs.path
    && lhs.start_line === rhs.start_line
    && lhs.start_char === rhs.start_char
    && lhs.end_line === rhs.end_line
    && lhs.end_char === rhs.end_char;
}

function sameTraceyLspSymbols(lhs: TraceyLspSymbol[], rhs: TraceyLspSymbol[]): boolean {
  return lhs.length === rhs.length
    && lhs.every((entry, idx) => sameTraceyLspSymbol(entry, rhs[idx] as TraceyLspSymbol));
}

function sameTraceyLspSemanticToken(
  lhs: TraceyLspSemanticToken,
  rhs: TraceyLspSemanticToken,
): boolean {
  return lhs.line === rhs.line
    && lhs.start_char === rhs.start_char
    && lhs.length === rhs.length
    && lhs.token_type === rhs.token_type
    && lhs.modifiers === rhs.modifiers;
}

function sameTraceyLspSemanticTokens(
  lhs: TraceyLspSemanticToken[],
  rhs: TraceyLspSemanticToken[],
): boolean {
  return lhs.length === rhs.length
    && lhs.every((entry, idx) =>
      sameTraceyLspSemanticToken(entry, rhs[idx] as TraceyLspSemanticToken)
    );
}

function sameStringArray(lhs: string[], rhs: string[]): boolean {
  return lhs.length === rhs.length && lhs.every((entry, idx) => entry === rhs[idx]);
}

function sameTraceyLspCodeLens(lhs: TraceyLspCodeLens, rhs: TraceyLspCodeLens): boolean {
  return lhs.line === rhs.line
    && lhs.start_char === rhs.start_char
    && lhs.end_char === rhs.end_char
    && lhs.title === rhs.title
    && lhs.command === rhs.command
    && sameStringArray(lhs.arguments, rhs.arguments);
}

function sameTraceyLspCodeLensList(lhs: TraceyLspCodeLens[], rhs: TraceyLspCodeLens[]): boolean {
  return lhs.length === rhs.length
    && lhs.every((entry, idx) => sameTraceyLspCodeLens(entry, rhs[idx] as TraceyLspCodeLens));
}

function sameTraceyLspInlayHint(lhs: TraceyLspInlayHint, rhs: TraceyLspInlayHint): boolean {
  return lhs.line === rhs.line && lhs.character === rhs.character && lhs.label === rhs.label;
}

function sameTraceyLspInlayHints(
  lhs: TraceyLspInlayHint[],
  rhs: TraceyLspInlayHint[],
): boolean {
  return lhs.length === rhs.length
    && lhs.every((entry, idx) => sameTraceyLspInlayHint(entry, rhs[idx] as TraceyLspInlayHint));
}

function sameTraceyPrepareRenameResult(
  lhs: TraceyPrepareRenameResult | null,
  rhs: TraceyPrepareRenameResult | null,
): boolean {
  if (lhs === null || rhs === null) return lhs === rhs;
  return lhs.start_line === rhs.start_line
    && lhs.start_char === rhs.start_char
    && lhs.end_line === rhs.end_line
    && lhs.end_char === rhs.end_char
    && lhs.placeholder === rhs.placeholder;
}

function sameTraceyLspTextEdit(lhs: TraceyLspTextEdit, rhs: TraceyLspTextEdit): boolean {
  return lhs.path === rhs.path
    && lhs.start_line === rhs.start_line
    && lhs.start_char === rhs.start_char
    && lhs.end_line === rhs.end_line
    && lhs.end_char === rhs.end_char
    && lhs.new_text === rhs.new_text;
}

function sameTraceyLspTextEdits(lhs: TraceyLspTextEdit[], rhs: TraceyLspTextEdit[]): boolean {
  return lhs.length === rhs.length
    && lhs.every((entry, idx) => sameTraceyLspTextEdit(entry, rhs[idx] as TraceyLspTextEdit));
}

function sameTraceyLspCodeAction(lhs: TraceyLspCodeAction, rhs: TraceyLspCodeAction): boolean {
  return lhs.title === rhs.title
    && lhs.kind === rhs.kind
    && lhs.command === rhs.command
    && sameStringArray(lhs.arguments, rhs.arguments)
    && lhs.is_preferred === rhs.is_preferred;
}

function sameTraceyLspCodeActions(
  lhs: TraceyLspCodeAction[],
  rhs: TraceyLspCodeAction[],
): boolean {
  return lhs.length === rhs.length
    && lhs.every((entry, idx) => sameTraceyLspCodeAction(entry, rhs[idx] as TraceyLspCodeAction));
}

function sameTraceyCoverageChange(
  lhs: TraceyCoverageChange,
  rhs: TraceyCoverageChange,
): boolean {
  return sameTraceyRuleId(lhs.rule_id, rhs.rule_id)
    && lhs.file === rhs.file
    && lhs.line === rhs.line;
}

function sameTraceyDeltaSummary(
  lhs: TraceyDeltaSummary | null,
  rhs: TraceyDeltaSummary | null,
): boolean {
  if (lhs === null || rhs === null) return lhs === rhs;
  return lhs.newly_covered.length === rhs.newly_covered.length
    && lhs.newly_uncovered.length === rhs.newly_uncovered.length
    && lhs.newly_covered.every((entry, idx) => sameTraceyCoverageChange(entry, rhs.newly_covered[idx] as TraceyCoverageChange))
    && lhs.newly_uncovered.every((rule, idx) => sameTraceyRuleId(rule, rhs.newly_uncovered[idx] as TraceyRuleId));
}

function sameTraceyDataUpdate(lhs: TraceyDataUpdate, rhs: TraceyDataUpdate): boolean {
  return lhs.version === rhs.version && sameTraceyDeltaSummary(lhs.delta, rhs.delta);
}

function sameTraceyUpdates(lhs: TraceyDataUpdate[], rhs: TraceyDataUpdate[]): boolean {
  return lhs.length === rhs.length
    && lhs.every((update, idx) => sameTraceyDataUpdate(update, rhs[idx] as TraceyDataUpdate));
}

// Service implementation
class TestbedService implements TestbedHandler {
  private async streamValues(count: number, output: Tx<number>): Promise<void> {
    for (let i = 0; i < count; i++) {
      await output.send(i);
    }
    output.close();
  }

  // Echo methods
  echo(message: string): string {
    return message;
  }

  reverse(message: string): string {
    return Array.from(message).toReversed().join("");
  }

  // Fallible methods
  divide(
    dividend: bigint,
    divisor: bigint,
  ): { ok: true; value: bigint } | { ok: false; error: MathError } {
    if (divisor === 0n) {
      return { ok: false, error: { tag: "DivisionByZero" } };
    }
    // Detect overflow: i64::MIN / -1 overflows
    if (dividend === -9223372036854775808n && divisor === -1n) {
      return { ok: false, error: { tag: "Overflow" } };
    }
    return { ok: true, value: dividend / divisor };
  }

  lookup(id: number): { ok: true; value: Person } | { ok: false; error: LookupError } {
    switch (id) {
      case 1:
        return { ok: true, value: { name: "Alice", age: 30, email: "alice@example.com" } };
      case 2:
        return { ok: true, value: { name: "Bob", age: 25, email: null } };
      case 3:
        return { ok: true, value: { name: "Charlie", age: 35, email: "charlie@example.com" } };
      default:
        if (id >= 100 && id <= 199) {
          return { ok: false, error: { tag: "AccessDenied" } };
        }
        return { ok: false, error: { tag: "NotFound" } };
    }
  }

  // Streaming methods
  async sum(numbers: Rx<number>): Promise<bigint> {
    // Server receives numbers via Rx channel and sums them
    let total = 0n;
    for await (const n of numbers) {
      total += BigInt(n);
    }
    return total;
  }

  async generate(count: number, output: Tx<number>): Promise<void> {
    await this.streamValues(count, output);
  }

  async transform(input: Rx<string>, output: Tx<string>): Promise<void> {
    // Server receives via Rx, sends via Tx (echo back as-is)
    for await (const s of input) {
      await output.send(s);
    }
    output.close();
  }

  async postReplyGenerate(output: Tx<number>): Promise<void> {
    for (let i = 0; i < 5; i++) {
      await output.send(i);
    }
    output.close();
  }

  async postReplySum(input: Rx<number>, result: Tx<bigint>): Promise<void> {
    let total = 0n;
    for await (const n of input) {
      total += BigInt(n);
    }
    await result.send(total);
    result.close();
  }

  // Complex type methods
  echoPoint(point: Point): Point {
    return point;
  }

  createPerson(name: string, age: number, email: string | null): Person {
    return { name, age, email };
  }

  rectangleArea(rect: Rectangle): number {
    const width = Math.abs(rect.bottom_right.x - rect.top_left.x);
    const height = Math.abs(rect.bottom_right.y - rect.top_left.y);
    return width * height;
  }

  parseColor(name: string): Color | null {
    switch (name.toLowerCase()) {
      case "red":
        return { tag: "Red" };
      case "green":
        return { tag: "Green" };
      case "blue":
        return { tag: "Blue" };
      default:
        return null;
    }
  }

  shapeArea(shape: Shape): number {
    switch (shape.tag) {
      case "Circle":
        return Math.PI * shape.radius * shape.radius;
      case "Rectangle":
        return shape.width * shape.height;
      case "Point":
        return 0;
    }
  }

  createCanvas(name: string, shapes: Shape[], background: Color): Canvas {
    return { name, shapes, background };
  }

  echoGnarly(payload: GnarlyPayload): GnarlyPayload {
    return payload;
  }

  processMessage(msg: Message): Message {
    switch (msg.tag) {
      case "Text":
        return { tag: "Text", value: `processed: ${msg.value}` };
      case "Number":
        return { tag: "Number", value: msg.value * 2n };
      case "Data":
        return { tag: "Data", value: msg.value.toReversed() };
    }
  }

  getPoints(count: number): Point[] {
    const points: Point[] = [];
    for (let i = 0; i < count; i++) {
      points.push({ x: i, y: i * 2 });
    }
    return points;
  }

  swapPair(pair: [number, string]): [string, number] {
    return [pair[1], pair[0]];
  }

  echoBytes(data: Uint8Array): Uint8Array {
    return data;
  }

  echoBool(b: boolean): boolean {
    return b;
  }

  echoU64(n: bigint): bigint {
    return n;
  }

  echoOptionString(s: string | null): string | null {
    return s;
  }

  async sumLarge(numbers: Rx<number>): Promise<bigint> {
    let total = 0n;
    for await (const n of numbers) {
      total += BigInt(n);
    }
    return total;
  }

  async generateLarge(count: number, output: Tx<number>): Promise<void> {
    await this.streamValues(count, output);
  }

  allColors(): Color[] {
    return [{ tag: "Red" }, { tag: "Green" }, { tag: "Blue" }];
  }

  describePoint(label: string, x: number, y: number, active: boolean): TaggedPoint {
    return { label, x, y, active };
  }

  echoShape(shape: Shape): Shape {
    return shape;
  }

  echoStatusV1(status: Status): Status {
    return status;
  }

  echoTagV1(tag: Tag): Tag {
    return tag;
  }

  // Schema evolution methods
  echoProfile(profile: Profile): Profile {
    return profile;
  }

  echoRecord(record: Record): Record {
    return record;
  }

  echoStatus(status: Status): Status {
    return status;
  }

  echoTag(tag: Tag): Tag {
    return tag;
  }

  echoMeasurement(m: Measurement): Measurement {
    return m;
  }

  echoConfig(c: Config): Config {
    return c;
  }

  echoTree(tree: Tree): Tree {
    return tree;
  }

  async dodecaByteTunnel(inbound: unknown, outbound: unknown): Promise<void> {
    const rx = inbound as Rx<Uint8Array>;
    const tx = outbound as Tx<Uint8Array>;
    for await (const chunk of rx) {
      await tx.send(chunk);
    }
    tx.close();
  }

  async dodecaDevtoolsLsp(token: string, clientToServer: unknown, serverToClient: unknown): Promise<void> {
    const rx = clientToServer as Rx<string>;
    const tx = serverToClient as Tx<string>;
    if (token !== "editor-token") {
      tx.close();
      return;
    }
    for await (const chunk of rx) {
      await tx.send(`lsp:${chunk}`);
    }
    tx.close();
  }

  dibsList(
    request: DibsListRequest,
  ): { ok: true; value: DibsListResponse } | { ok: false; error: DibsError } {
    if (!sameHelixDeep(request, sampleDibsListRequest())) {
      return { ok: false, error: { tag: "UnknownTable", value: request.table } };
    }
    return { ok: true, value: sampleDibsListResponse() };
  }

  dibsSchema(): DibsSchemaInfo {
    return sampleDibsSchema();
  }

  dibsGet(
    request: DibsGetRequest,
  ): { ok: true; value: DibsRow | null } | { ok: false; error: DibsError } {
    if (!sameHelixDeep(request, sampleDibsGetRequest())) {
      return { ok: false, error: { tag: "InvalidRequest", value: "unexpected get request" } };
    }
    return { ok: true, value: sampleDibsRowOne() };
  }

  dibsCreate(
    request: DibsCreateRequest,
  ): { ok: true; value: DibsRow } | { ok: false; error: DibsError } {
    if (!sameHelixDeep(request, sampleDibsCreateRequest())) {
      return { ok: false, error: { tag: "InvalidRequest", value: "unexpected create request" } };
    }
    return { ok: true, value: sampleDibsCreateResponse() };
  }

  dibsUpdate(
    request: DibsUpdateRequest,
  ): { ok: true; value: DibsRow } | { ok: false; error: DibsError } {
    if (!sameHelixDeep(request, sampleDibsUpdateRequest())) {
      return { ok: false, error: { tag: "InvalidRequest", value: "unexpected update request" } };
    }
    return { ok: true, value: sampleDibsUpdateResponse() };
  }

  dibsDelete(
    request: DibsDeleteRequest,
  ): { ok: true; value: bigint } | { ok: false; error: DibsError } {
    if (!sameHelixDeep(request, sampleDibsDeleteRequest())) {
      return { ok: false, error: { tag: "InvalidRequest", value: "unexpected delete request" } };
    }
    return { ok: true, value: 1n };
  }

  dibsMigrationStatus(
    request: DibsMigrationStatusRequest,
  ): { ok: true; value: DibsMigrationInfo[] } | { ok: false; error: DibsError } {
    if (!sameHelixDeep(request, sampleDibsMigrationStatusRequest())) {
      return { ok: false, error: { tag: "InvalidRequest", value: "unexpected migration status request" } };
    }
    return { ok: true, value: sampleDibsMigrationStatus() };
  }

  async dibsMigrate(
    request: DibsMigrateRequest,
    logs: unknown,
  ): Promise<{ ok: true; value: DibsMigrateResult } | { ok: false; error: DibsError }> {
    if (!sameHelixDeep(request, sampleDibsMigrateRequest())) {
      return { ok: false, error: { tag: "InvalidRequest", value: "unexpected migrate request" } };
    }
    const tx = logs as Tx<DibsMigrationLog>;
    for (const logEntry of sampleDibsLogs()) {
      await tx.send(logEntry);
    }
    tx.close();
    return { ok: true, value: sampleDibsMigrateResult() };
  }

  echoEcosystemBridge(payload: EcosystemBridgePayload): EcosystemBridgePayload {
    return payload;
  }

  echoDodecaTemplateCall(call: DodecaTemplateCall): DodecaTemplateCall {
    return call;
  }

  dodecaHtmlProcess(input: DodecaHtmlProcessInput): DodecaHtmlProcessResult {
    if (sameHelixDeep(input, sampleDodecaHtmlProcessInput())) {
      return sampleDodecaHtmlProcessResult();
    }
    return { tag: "Error", message: `unexpected input: ${String(input.html)}` };
  }

  dodecaExecuteCodeSamples(input: DodecaExecuteSamplesInput): DodecaCodeExecutionResult {
    if (sameHelixDeep(input, sampleDodecaExecuteSamplesInput())) {
      return sampleDodecaCodeExecutionResult();
    }
    return { tag: "Error", message: `unexpected input: ${String(input.samples.length)}` };
  }

  dodecaLoadData(content: string, format: DodecaDataFormat): DodecaLoadDataResult {
    if (content === sampleDodecaDataContent() && sameHelixDeep(format, sampleDodecaDataFormat())) {
      return sampleDodecaLoadDataResult();
    }
    return { tag: "Error", message: `unexpected load_data input: ${content}` };
  }

  dodecaParseAndRender(sourcePath: string, content: string, sourceMap: boolean): DodecaParseResult {
    if (
      sourcePath === sampleDodecaMarkdownSourcePath()
      && content === sampleDodecaMarkdownContent()
      && sourceMap
    ) {
      return sampleDodecaParseResult();
    }
    return { tag: "Error", message: `unexpected parse input: ${sourcePath}` };
  }

  echoDodecaImageProcessorFixture(fixture: DodecaImageProcessorFixture): DodecaImageProcessorFixture {
    return fixture;
  }

  echoDodecaSearchIndexerFixture(fixture: DodecaSearchIndexerFixture): DodecaSearchIndexerFixture {
    return fixture;
  }

  echoDodecaAssetProcessingFixture(fixture: DodecaAssetProcessingFixture): DodecaAssetProcessingFixture {
    return fixture;
  }


  echoDodecaSmallCellServicesFixture(fixture: DodecaSmallCellServicesFixture): DodecaSmallCellServicesFixture {
    return fixture;
  }

  echoDodecaDevtoolsEvent(event: DodecaDevtoolsEvent): DodecaDevtoolsEvent {
    return event;
  }

  dodecaDevtoolsGetScope(path: string[] | null): DodecaScopeEntry[] {
    if (path !== null && sameHelixDeep(path, ["page"])) {
      return sampleDodecaScopeEntries();
    }
    return [];
  }

  dodecaDevtoolsEval(snapshotId: string, expression: string): DodecaEvalResult {
    if (snapshotId === "snap-devtools-42" && expression === "page.title") {
      return sampleDodecaEvalResult();
    }
    return { tag: "Err", value: `unexpected eval input: ${snapshotId} ${expression}` };
  }

  dodecaDevtoolsOpenDeadLink(route: string, target: DodecaDeadLinkTarget): DodecaOpenSourceResult {
    if (route === "/guide/" && sameHelixDeep(target, sampleDodecaDeadLinkTarget())) {
      return sampleDodecaOpenSourceResult();
    }
    return { tag: "Err", value: `unexpected dead-link input: ${route}` };
  }

  dodecaDevtoolsEditLoad(token: string, route: string): DodecaEditLoad {
    if (token === "editor-token" && route === "/guide/") {
      return sampleDodecaEditLoad();
    }
    return { tag: "Denied" };
  }

  dodecaDevtoolsEditPreview(token: string, sourceKey: string, buffer: string): DodecaEditPreview {
    if (token === "editor-token" && sourceKey === "content/guide.md" && buffer === "# Guide\n\nUpdated from browser.") {
      return sampleDodecaEditPreview();
    }
    return { tag: "Denied" };
  }

  dodecaDevtoolsEditSave(token: string, req: DodecaEditSaveReq): DodecaEditSave {
    if (token === "editor-token" && sameHelixDeep(req, sampleDodecaEditSaveReq())) {
      return sampleDodecaEditSave();
    }
    return { tag: "Denied" };
  }

  dodecaDevtoolsEditUpload(token: string, req: DodecaEditUploadReq): DodecaEditUpload {
    if (token === "editor-token" && sameHelixDeep(req, sampleDodecaEditUploadReq())) {
      return sampleDodecaEditUpload();
    }
    return { tag: "Denied" };
  }

  dodecaDevtoolsEditRead(token: string, uri: string): DodecaEditRead {
    if (token === "editor-token" && uri === "file:///workspace/content/guide.md") {
      return sampleDodecaEditRead();
    }
    return { tag: "Denied" };
  }

  dodecaDevtoolsEditList(token: string): DodecaEditList {
    if (token === "editor-token") {
      return sampleDodecaEditList();
    }
    return { tag: "Denied" };
  }

  echoStyxValue(value: StyxValue): StyxValue {
    return value;
  }

  styxLspInitialize(params: StyxLspInitializeParams): StyxLspInitializeResult {
    if (!sameHelixDeep(params, sampleStyxLspInitializeParams())) {
      throw new Error("styx_lsp_initialize: unexpected params");
    }
    return sampleStyxLspInitializeResult();
  }

  styxLspCompletions(params: StyxLspCompletionParams): StyxLspCompletionItem[] {
    if (!sameHelixDeep(params, sampleStyxLspCompletionParams())) {
      throw new Error("styx_lsp_completions: unexpected params");
    }
    return sampleStyxLspCompletions();
  }

  styxLspHover(params: StyxLspHoverParams): StyxLspHoverResult | null {
    if (!sameHelixDeep(params, sampleStyxLspHoverParams())) {
      throw new Error("styx_lsp_hover: unexpected params");
    }
    return sampleStyxLspHoverResult();
  }

  styxLspInlayHints(params: StyxLspInlayHintParams): StyxLspInlayHint[] {
    if (!sameHelixDeep(params, sampleStyxLspInlayHintParams())) {
      throw new Error("styx_lsp_inlay_hints: unexpected params");
    }
    return sampleStyxLspInlayHints();
  }

  styxLspDiagnostics(params: StyxLspDiagnosticParams): StyxLspDiagnostic[] {
    if (!sameHelixDeep(params, sampleStyxLspDiagnosticParams())) {
      throw new Error("styx_lsp_diagnostics: unexpected params");
    }
    return sampleStyxLspDiagnostics();
  }

  styxLspCodeActions(params: StyxLspCodeActionParams): StyxLspCodeAction[] {
    if (!sameHelixDeep(params, sampleStyxLspCodeActionParams())) {
      throw new Error("styx_lsp_code_actions: unexpected params");
    }
    return sampleStyxLspCodeActions();
  }

  styxLspDefinition(params: StyxLspDefinitionParams): StyxLspLocation[] {
    if (!sameHelixDeep(params, sampleStyxLspDefinitionParams())) {
      throw new Error("styx_lsp_definition: unexpected params");
    }
    return sampleStyxLspLocations();
  }

  styxLspShutdown(): void {}

  styxHostGetSubtree(params: StyxLspGetSubtreeParams): StyxValue | null {
    if (!sameHelixDeep(params, sampleStyxLspGetSubtreeParams())) {
      throw new Error("styx_host_get_subtree: unexpected params");
    }
    return sampleStyxValue();
  }

  styxHostGetDocument(params: StyxLspGetDocumentParams): StyxValue | null {
    if (!sameHelixDeep(params, sampleStyxLspGetDocumentParams())) {
      throw new Error("styx_host_get_document: unexpected params");
    }
    return sampleStyxValue();
  }

  styxHostGetSource(params: StyxLspGetSourceParams): string | null {
    if (!sameHelixDeep(params, sampleStyxLspGetSourceParams())) {
      throw new Error("styx_host_get_source: unexpected params");
    }
    return sampleStyxLspSource();
  }

  styxHostGetSchema(params: StyxLspGetSchemaParams): StyxLspSchemaInfo | null {
    if (!sameHelixDeep(params, sampleStyxLspGetSchemaParams())) {
      throw new Error("styx_host_get_schema: unexpected params");
    }
    return sampleStyxLspSchemaInfo();
  }

  styxHostOffsetToPosition(params: StyxLspOffsetToPositionParams): StyxLspPosition | null {
    if (!sameHelixDeep(params, sampleStyxLspOffsetToPositionParams())) {
      throw new Error("styx_host_offset_to_position: unexpected params");
    }
    return sampleStyxLspPosition();
  }

  styxHostPositionToOffset(params: StyxLspPositionToOffsetParams): number | null {
    if (!sameHelixDeep(params, sampleStyxLspPositionToOffsetParams())) {
      throw new Error("styx_host_position_to_offset: unexpected params");
    }
    return 16;
  }

  staxFlamegraph(params: StaxViewParams): StaxFlamegraphUpdate {
    return sampleStaxFlamegraphUpdate(params);
  }

  echoStaxFlamegraphUpdate(update: StaxFlamegraphUpdate): StaxFlamegraphUpdate {
    return update;
  }

  async staxSubscribeFlamegraphUpdates(output: unknown): Promise<void> {
    const tx = output as Tx<StaxFlamegraphUpdate>;
    for (const update of sampleStaxFlamegraphUpdates()) {
      await tx.send(update);
    }
    tx.close();
  }

  echoStaxLinuxBrokerControl(fixture: StaxLinuxBrokerControlFixture): StaxLinuxBrokerControlFixture {
    return fixture;
  }

  async staxMacosRecord(
    config: StaxMacSessionConfig,
    records: unknown,
  ): Promise<{ ok: true; value: StaxMacRecordSummary } | { ok: false; error: StaxMacRecordError }> {
    if (!sameHelixDeep(config, sampleStaxMacosConfig())) {
      throw new Error("stax_macos_record: unexpected config");
    }
    const tx = records as Tx<StaxMacKdBufBatch>;
    for (const batch of sampleStaxMacosBatches()) {
      await tx.send(batch);
    }
    tx.close();
    return { ok: true, value: sampleStaxMacosRecordSummary() };
  }

  echoHotmealLiveReloadEvent(event: HotmealLiveReloadEvent): HotmealLiveReloadEvent {
    return event;
  }

  echoHotmealApplyPatchesResult(result: HotmealApplyPatchesResult): HotmealApplyPatchesResult {
    return result;
  }

  hotmealLiveReloadSubscribe(route: string): void {
    if (route !== sampleHotmealRoute()) {
      throw new Error(`unexpected Hotmeal route: ${route}`);
    }
  }

  hotmealLiveReloadOnEvent(event: HotmealLiveReloadEvent): void {
    if (!sampleHotmealLiveReloadEvents().some((expected) => sameHotmealLiveReloadEvent(event, expected))) {
      throw new Error("unexpected Hotmeal live-reload event");
    }
  }

  echoHelixStreamMetrics(metrics: HelixStreamMetrics): HelixStreamMetrics {
    return metrics;
  }

  echoHelixVerifyEvidence(digest: HelixVerifyEvidenceDigest): HelixVerifyEvidenceDigest {
    return digest;
  }

  async helixSubscribePulses(output: unknown): Promise<void> {
    const tx = output as Tx<HelixPulseAvailable>;
    for (const pulse of sampleHelixPulses()) {
      await tx.send(pulse);
    }
    tx.close();
  }

  helixPulseBundle(_pulseId: bigint, _fields: HelixPulseBundleFields): HelixPulseBundle {
    return sampleHelixPulseBundle();
  }

  helixTraceServiceSurface(): HelixTraceServiceSurface {
    return sampleHelixTraceServiceSurface();
  }

  traceyStatus(): TraceyStatusResponse {
    return sampleTraceyStatusResponse();
  }

  traceyUncovered(req: TraceyUncoveredRequest): TraceyUncoveredResponse {
    if (!sameTraceyUncoveredRequest(req, sampleTraceyQueryRequest())) {
      throw new Error("tracey_uncovered: request mismatch");
    }
    return sampleTraceyUncoveredResponse();
  }

  traceyUntested(req: TraceyUntestedRequest): TraceyUntestedResponse {
    if (!sameTraceyUntestedRequest(req, sampleTraceyUntestedRequest())) {
      throw new Error("tracey_untested: request mismatch");
    }
    return sampleTraceyUntestedResponse();
  }

  traceyStale(req: TraceyStaleRequest): TraceyStaleResponse {
    if (!sameTraceyStaleRequest(req, sampleTraceyStaleRequest())) {
      throw new Error("tracey_stale: request mismatch");
    }
    return sampleTraceyStaleResponse();
  }

  traceyUnmapped(req: TraceyUnmappedRequest): TraceyUnmappedResponse {
    if (!sameTraceyUnmappedRequest(req, sampleTraceyUnmappedRequest())) {
      throw new Error("tracey_unmapped: request mismatch");
    }
    return sampleTraceyUnmappedResponse();
  }

  traceyRule(ruleId: TraceyRuleId): TraceyRuleInfo | null {
    return sameTraceyRuleId(ruleId, traceyRuleId("rpc.channel.direct-args", 1))
      ? sampleTraceyRuleInfo()
      : null;
  }

  traceyForward(spec: string, implName: string): TraceyApiSpecForward | null {
    if (implName !== "rust") {
      throw new Error("tracey_forward: implementation mismatch");
    }
    return spec === "vox" ? sampleTraceyForwardResponse() : null;
  }

  traceyReverse(spec: string, implName: string): TraceyApiReverseData | null {
    if (spec !== "vox" || implName !== "rust") {
      throw new Error("tracey_reverse: request mismatch");
    }
    return sampleTraceyReverseResponse();
  }

  traceyFile(req: TraceyFileRequest): TraceyApiFileData | null {
    if (!sameTraceyDashboardValue(req, sampleTraceyFileRequest())) {
      throw new Error("tracey_file: request mismatch");
    }
    return sampleTraceyFileResponse();
  }

  traceySpecContent(spec: string, implName: string): TraceyApiSpecData | null {
    if (spec !== "vox" || implName !== "rust") {
      throw new Error("tracey_spec_content: request mismatch");
    }
    return sampleTraceySpecContentResponse();
  }

  traceySearch(query: string, limit: number): TraceySearchResult[] {
    if (query !== "channel" || limit !== 10) {
      throw new Error("tracey_search: request mismatch");
    }
    return sampleTraceySearchResults();
  }

  traceyUpdateFileRange(
    req: TraceyUpdateFileRangeRequest,
  ): { ok: true; value: void } | { ok: false; error: TraceyUpdateError } {
    if (sameTraceyDashboardValue(req, sampleTraceyUpdateFileRangeRequest())) {
      return { ok: true, value: undefined };
    }
    if (sameTraceyDashboardValue(req, sampleTraceyUpdateFileRangeConflictRequest())) {
      return { ok: false, error: sampleTraceyUpdateError() };
    }
    throw new Error("tracey_update_file_range: request mismatch");
  }

  traceyConfigAddExclude(req: TraceyConfigPatternRequest): { ok: true; value: void } | { ok: false; error: string } {
    if (sameTraceyDashboardValue(req, sampleTraceyConfigPatternRequest())) {
      return { ok: true, value: undefined };
    }
    if (sameTraceyDashboardValue(req, sampleTraceyBadConfigPatternRequest())) {
      return { ok: false, error: "invalid pattern" };
    }
    throw new Error("tracey_config_add_exclude: request mismatch");
  }

  traceyConfigAddInclude(req: TraceyConfigPatternRequest): { ok: true; value: void } | { ok: false; error: string } {
    if (!sameTraceyDashboardValue(req, sampleTraceyConfigPatternRequest())) {
      throw new Error("tracey_config_add_include: request mismatch");
    }
    return { ok: true, value: undefined };
  }

  traceyConfig(): TraceyApiConfig {
    return sampleTraceyApiConfig();
  }

  traceyVfsOpen(path: string, content: string): void {
    if (path !== "src/lib.rs" || content !== sampleTraceyLspContent()) {
      throw new Error("tracey_vfs_open: request mismatch");
    }
  }

  traceyVfsChange(path: string, content: string): void {
    if (path !== "src/lib.rs" || content !== "// r[verify rpc.channel.direct-args]\n") {
      throw new Error("tracey_vfs_change: request mismatch");
    }
  }

  traceyVfsClose(path: string): void {
    if (path !== "src/lib.rs") {
      throw new Error("tracey_vfs_close: request mismatch");
    }
  }

  traceyReload(): TraceyReloadResponse {
    return sampleTraceyReloadResponse();
  }

  traceyVersion(): bigint {
    return 13n;
  }

  traceyHealth(): TraceyHealthResponse {
    return sampleTraceyHealthResponse();
  }

  traceyShutdown(): void {}

  traceyValidate(req: TraceyValidateRequest): TraceyValidationResult {
    void req;
    return sampleTraceyValidationResult();
  }

  traceyIsTestFile(path: string): boolean {
    return path.endsWith("_test.rs") || path.includes("/tests/");
  }

  traceyLspHover(req: TraceyLspPositionRequest): TraceyHoverInfo | null {
    if (!sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest())) {
      throw new Error("tracey_lsp_hover: request mismatch");
    }
    return sampleTraceyHoverInfo();
  }

  traceyLspDefinition(req: TraceyLspPositionRequest): TraceyLspLocation[] {
    if (!sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest())) {
      throw new Error("tracey_lsp_definition: request mismatch");
    }
    return sampleTraceyLspLocations();
  }

  traceyLspImplementation(req: TraceyLspPositionRequest): TraceyLspLocation[] {
    if (!sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest())) {
      throw new Error("tracey_lsp_implementation: request mismatch");
    }
    return sampleTraceyLspLocations();
  }

  traceyLspReferences(req: TraceyLspReferencesRequest): TraceyLspLocation[] {
    if (!sameTraceyLspReferencesRequest(req, sampleTraceyLspReferencesRequest())) {
      throw new Error("tracey_lsp_references: request mismatch");
    }
    return sampleTraceyLspLocations();
  }

  traceyLspCompletions(req: TraceyLspPositionRequest): TraceyLspCompletionItem[] {
    if (!sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest())) {
      throw new Error("tracey_lsp_completions: request mismatch");
    }
    return sampleTraceyLspCompletions();
  }

  traceyLspWorkspaceDiagnostics(): TraceyLspFileDiagnostics[] {
    return sampleTraceyLspWorkspaceDiagnostics();
  }

  traceyLspDocumentSymbols(req: TraceyLspDocumentRequest): TraceyLspSymbol[] {
    if (!sameTraceyLspDocumentRequest(req, sampleTraceyLspDocumentRequest())) {
      throw new Error("tracey_lsp_document_symbols: request mismatch");
    }
    return sampleTraceyLspSymbols();
  }

  traceyLspWorkspaceSymbols(query: string): TraceyLspSymbol[] {
    if (query !== "rpc.channel") {
      throw new Error("tracey_lsp_workspace_symbols: query mismatch");
    }
    return sampleTraceyLspSymbols();
  }

  traceyLspSemanticTokens(req: TraceyLspDocumentRequest): TraceyLspSemanticToken[] {
    if (!sameTraceyLspDocumentRequest(req, sampleTraceyLspDocumentRequest())) {
      throw new Error("tracey_lsp_semantic_tokens: request mismatch");
    }
    return sampleTraceyLspSemanticTokens();
  }

  traceyLspCodeLens(req: TraceyLspDocumentRequest): TraceyLspCodeLens[] {
    if (!sameTraceyLspDocumentRequest(req, sampleTraceyLspDocumentRequest())) {
      throw new Error("tracey_lsp_code_lens: request mismatch");
    }
    return sampleTraceyLspCodeLens();
  }

  traceyLspInlayHints(req: TraceyLspInlayHintsRequest): TraceyLspInlayHint[] {
    if (!sameTraceyLspInlayHintsRequest(req, sampleTraceyLspInlayHintsRequest())) {
      throw new Error("tracey_lsp_inlay_hints: request mismatch");
    }
    return sampleTraceyLspInlayHints();
  }

  traceyLspPrepareRename(req: TraceyLspPositionRequest): TraceyPrepareRenameResult | null {
    if (!sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest())) {
      throw new Error("tracey_lsp_prepare_rename: request mismatch");
    }
    return sampleTraceyPrepareRenameResult();
  }

  traceyLspRename(req: TraceyLspRenameRequest): TraceyLspTextEdit[] {
    if (!sameTraceyLspRenameRequest(req, sampleTraceyLspRenameRequest())) {
      throw new Error("tracey_lsp_rename: request mismatch");
    }
    return sampleTraceyLspTextEdits();
  }

  traceyLspCodeActions(req: TraceyLspPositionRequest): TraceyLspCodeAction[] {
    if (!sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest())) {
      throw new Error("tracey_lsp_code_actions: request mismatch");
    }
    return sampleTraceyLspCodeActions();
  }

  traceyLspDocumentHighlight(req: TraceyLspPositionRequest): TraceyLspLocation[] {
    if (!sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest())) {
      throw new Error("tracey_lsp_document_highlight: request mismatch");
    }
    return sampleTraceyLspLocations();
  }

  async traceySubscribeUpdates(updates: unknown): Promise<void> {
    const tx = updates as Tx<TraceyDataUpdate>;
    for (const update of sampleTraceyUpdates()) {
      await tx.send(update);
    }
    tx.close();
  }
}

function makeConnector(addr: string) {
  if (addr.startsWith("ws://") || addr.startsWith("wss://")) {
    return wsConnector(addr);
  }
  return tcpConnector(addr);
}

async function runServer() {
  const addr = process.env.PEER_ADDR;
  if (!addr) {
    throw new Error("PEER_ADDR env var not set");
  }

  // r[impl rpc.virtual-connection.accept] - Check if we should accept incoming service lanes.
  const acceptLanes = process.env.ACCEPT_CONNECTIONS !== "0";

  console.error(`server mode: connecting to ${addr}, acceptLanes=${acceptLanes}`);
  const connection = await connect(makeConnector(addr), {
    metadata: voxServiceMetadata("Testbed"),
    onLane: acceptLanes
      ? (lane) => {
          const driver = new Driver(
            lane,
            new TestbedDispatcher(new TestbedService()),
          );
          void driver.run();
        }
      : undefined,
  });
  const driver = new Driver(connection.lane(), new TestbedDispatcher(new TestbedService()));
  const handle = connection.handle();

  try {
    await driver.run();
  } catch (e) {
    if (e instanceof ConnectionError) {
      // Clean shutdown
      return;
    }
    throw e;
  } finally {
    // r[impl hosted.subject.lifecycle]
    handle.shutdown();
  }
}

async function runClient() {
  const addr = process.env.PEER_ADDR;
  if (!addr) {
    throw new Error("PEER_ADDR env var not set");
  }

  const scenario = process.env.CLIENT_SCENARIO ?? "echo";
  console.error(`client mode: connecting to ${addr}, scenario=${scenario}`);

  const connection = await connect(makeConnector(addr));
  const client = await connection.openLane(TestbedClient, {
    metadata: voxServiceMetadata("Testbed"),
  });
  const handle = connection.handle();

  try {
    switch (scenario) {
    case "echo": {
      const result = await client.echo("hello from client");
      console.error(`echo result: ${result}`);
      break;
    }
    case "sum": {
      // Client-to-server streaming: create channel, start call, then send
      const [tx, rx] = channel<number>();

      // Start the call first - this binds the channels
      const resultPromise = client.sum(rx);

      // Now send data through the bound Tx
      for (let i = 1; i <= 5; i++) {
        console.error(`sending ${i}`);
        await tx.send(i);
      }
      console.error("closing tx");
      tx.close();

      // Wait for result
      const result = await resultPromise;
      console.error(`sum result: ${result}`);
      break;
    }
    case "generate": {
      // Server-to-client streaming: create channel, call, receive
      const [tx, rx] = channel<number>();

      // Start the call - server will send through our Rx
      await client.generate(5, tx);

      // Receive values from Rx
      const received: number[] = [];
      for await (const n of rx) {
        console.error(`received ${n}`);
        received.push(n);
      }
      console.error(`generate received: [${received.join(", ")}]`);
      break;
    }
    case "shape_area": {
      const result = await client.shapeArea({ tag: "Rectangle", width: 3, height: 4 });
      if (result !== 12) {
        throw new Error(`shape_area expected 12, got ${result}`);
      }
      console.error(`shape_area result: ${result}`);
      break;
    }
    case "create_canvas": {
      const result = await client.createCanvas(
        "enum-canvas",
        [{ tag: "Point" }, { tag: "Circle", radius: 2.5 }],
        { tag: "Green" },
      );
      if (result.name !== "enum-canvas") {
        throw new Error(`create_canvas expected name enum-canvas, got ${result.name}`);
      }
      if (result.background.tag !== "Green") {
        throw new Error(`create_canvas expected background Green, got ${result.background.tag}`);
      }
      if (
        result.shapes.length !== 2 ||
        result.shapes[0]?.tag !== "Point" ||
        result.shapes[1]?.tag !== "Circle" ||
        result.shapes[1].radius !== 2.5
      ) {
        throw new Error(
          `create_canvas returned unexpected shapes: ${JSON.stringify(result.shapes)}`,
        );
      }
      console.error(`create_canvas result OK`);
      break;
    }
    case "process_message": {
      const result = await client.processMessage({
        tag: "Data",
        value: new Uint8Array([1, 2, 3, 4]),
      });
      if (
        result.tag !== "Data" ||
        result.value.length !== 4 ||
        result.value.join(",") !== "4,3,2,1"
      ) {
        throw new Error(`process_message returned unexpected value`);
      }
      console.error(`process_message result OK`);
      break;
    }
    case "reverse": {
      const result = await client.reverse("hello");
      if (result !== "olleh") throw new Error(`reverse: expected 'olleh', got ${result}`);
      console.error(`reverse OK`);
      break;
    }
    case "divide_success": {
      const r = await client.divide(10n, 3n);
      if (!r.ok || r.value !== 3n) throw new Error(`divide_success: expected 3, got ${JSON.stringify(r)}`);
      console.error(`divide_success OK`);
      break;
    }
    case "divide_zero": {
      const r = await client.divide(10n, 0n);
      if (r.ok || r.error.tag !== "DivisionByZero") throw new Error(`divide_zero: expected DivisionByZero, got ${JSON.stringify(r)}`);
      console.error(`divide_zero OK`);
      break;
    }
    case "divide_overflow": {
      const r = await client.divide(-9223372036854775808n, -1n);
      if (r.ok || r.error.tag !== "Overflow") throw new Error(`divide_overflow: expected Overflow, got ${JSON.stringify(r)}`);
      console.error(`divide_overflow OK`);
      break;
    }
    case "lookup_found": {
      const r = await client.lookup(1);
      if (!r.ok || r.value.name !== "Alice") throw new Error(`lookup_found: expected Alice, got ${JSON.stringify(r)}`);
      console.error(`lookup_found OK: ${r.value.name}`);
      break;
    }
    case "lookup_found_no_email": {
      const r = await client.lookup(2);
      if (!r.ok || r.value.name !== "Bob" || r.value.email !== null) throw new Error(`lookup_found_no_email: ${JSON.stringify(r)}`);
      console.error(`lookup_found_no_email OK`);
      break;
    }
    case "lookup_not_found": {
      const r = await client.lookup(999);
      if (r.ok || r.error.tag !== "NotFound") throw new Error(`lookup_not_found: expected NotFound, got ${JSON.stringify(r)}`);
      console.error(`lookup_not_found OK`);
      break;
    }
    case "lookup_access_denied": {
      const r = await client.lookup(100);
      if (r.ok || r.error.tag !== "AccessDenied") throw new Error(`lookup_access_denied: expected AccessDenied, got ${JSON.stringify(r)}`);
      console.error(`lookup_access_denied OK`);
      break;
    }
    case "echo_point": {
      const pt = { x: 42, y: -7 };
      const result = await client.echoPoint(pt);
      if (result.x !== 42 || result.y !== -7) throw new Error(`echo_point: ${JSON.stringify(result)}`);
      console.error(`echo_point OK`);
      break;
    }
    case "create_person": {
      const p = await client.createPerson("Dave", 40, "dave@example.com");
      if (p.name !== "Dave" || p.age !== 40 || p.email !== "dave@example.com") throw new Error(`create_person: ${JSON.stringify(p)}`);
      const p2 = await client.createPerson("Eve", 25, null);
      if (p2.name !== "Eve" || p2.email !== null) throw new Error(`create_person null email: ${JSON.stringify(p2)}`);
      console.error(`create_person OK`);
      break;
    }
    case "rectangle_area": {
      const area = await client.rectangleArea({ top_left: { x: 0, y: 10 }, bottom_right: { x: 5, y: 0 }, label: null });
      if (Math.abs(area - 50) > 1e-9) throw new Error(`rectangle_area: expected 50, got ${area}`);
      console.error(`rectangle_area OK: ${area}`);
      break;
    }
    case "parse_color": {
      const r = await client.parseColor("red");
      if (r?.tag !== "Red") throw new Error(`parse_color red: ${JSON.stringify(r)}`);
      const g = await client.parseColor("green");
      if (g?.tag !== "Green") throw new Error(`parse_color green: ${JSON.stringify(g)}`);
      const b = await client.parseColor("blue");
      if (b?.tag !== "Blue") throw new Error(`parse_color blue: ${JSON.stringify(b)}`);
      const n = await client.parseColor("purple");
      if (n !== null) throw new Error(`parse_color purple: expected null, got ${JSON.stringify(n)}`);
      console.error(`parse_color OK (all variants)`);
      break;
    }
    case "get_points": {
      const pts = await client.getPoints(5);
      if (pts.length !== 5) throw new Error(`get_points: expected 5, got ${pts.length}`);
      if (pts[0].x !== 0 || pts[4].x !== 4) throw new Error(`get_points: unexpected values`);
      console.error(`get_points OK: ${pts.length} points`);
      break;
    }
    case "swap_pair": {
      const result = await client.swapPair([99, "hello"]);
      if (result[0] !== "hello" || result[1] !== 99) throw new Error(`swap_pair: ${JSON.stringify(result)}`);
      console.error(`swap_pair OK`);
      break;
    }
    case "echo_bytes": {
      const data = new Uint8Array([1, 2, 3, 255, 0, 128]);
      const result = await client.echoBytes(data);
      if (result.length !== data.length || !data.every((v, i) => result[i] === v)) throw new Error(`echo_bytes mismatch`);
      console.error(`echo_bytes OK`);
      break;
    }
    case "echo_bool": {
      if (await client.echoBool(true) !== true) throw new Error(`echo_bool true failed`);
      if (await client.echoBool(false) !== false) throw new Error(`echo_bool false failed`);
      console.error(`echo_bool OK`);
      break;
    }
    case "echo_u64": {
      for (const n of [0n, 1n, 18446744073709551615n, 1000000000000n]) {
        const result = await client.echoU64(n);
        if (result !== n) throw new Error(`echo_u64 ${n}: got ${result}`);
      }
      console.error(`echo_u64 OK`);
      break;
    }
    case "echo_option_string": {
      const s = await client.echoOptionString("hello");
      if (s !== "hello") throw new Error(`echo_option_string Some: ${s}`);
      const n = await client.echoOptionString(null);
      if (n !== null) throw new Error(`echo_option_string None: ${n}`);
      console.error(`echo_option_string OK`);
      break;
    }
    case "describe_point": {
      const tp = await client.describePoint("origin", 0, 0, true);
      if (tp.label !== "origin" || tp.x !== 0 || tp.y !== 0 || !tp.active) throw new Error(`describe_point: ${JSON.stringify(tp)}`);
      const tp2 = await client.describePoint("far", -100, 200, false);
      if (tp2.label !== "far" || tp2.x !== -100 || tp2.y !== 200 || tp2.active) throw new Error(`describe_point 2: ${JSON.stringify(tp2)}`);
      console.error(`describe_point OK`);
      break;
    }
    case "all_colors": {
      const colors = await client.allColors();
      if (colors.length !== 3) throw new Error(`all_colors: expected 3, got ${colors.length}`);
      if (colors[0].tag !== "Red" || colors[1].tag !== "Green" || colors[2].tag !== "Blue") throw new Error(`all_colors order wrong: ${JSON.stringify(colors)}`);
      console.error(`all_colors OK`);
      break;
    }
    case "echo_shape": {
      const shapes = [
        { tag: "Point" } as const,
        { tag: "Circle", radius: 3.14 } as const,
        { tag: "Rectangle", width: 2.0, height: 5.0 } as const,
      ];
      for (const shape of shapes) {
        const result = await client.echoShape(shape);
        if (result.tag !== shape.tag) throw new Error(`echo_shape ${shape.tag}: got ${JSON.stringify(result)}`);
      }
      console.error(`echo_shape OK (all 3 variants)`);
      break;
    }
    case "echo_tree": {
      const tree: Tree = {
        value: 1,
        children: [
          { value: 2, children: [] },
          { value: 3, children: [{ value: 4, children: [] }] },
        ],
      };
      const result = await client.echoTree(tree);
      if (JSON.stringify(result) !== JSON.stringify(tree)) {
        throw new Error(`echo_tree: got ${JSON.stringify(result)}`);
      }
      console.error(`echo_tree OK`);
      break;
    }
    case "echo_ecosystem_bridge": {
      const payload = sampleEcosystemBridgePayload();
      const result = await client.echoEcosystemBridge(payload);
      if (!sameEcosystemBridgePayload(result, payload)) {
        throw new Error("echo_ecosystem_bridge: payload mismatch");
      }
      console.error(`echo_ecosystem_bridge OK`);
      break;
    }
    case "echo_dodeca_template_call": {
      const payload = sampleDodecaTemplateCall();
      const result = await client.echoDodecaTemplateCall(payload);
      if (!sameDodecaTemplateCall(result, payload)) {
        throw new Error("echo_dodeca_template_call: payload mismatch");
      }
      console.error(`echo_dodeca_template_call OK`);
      break;
    }
    case "dodeca_html_process": {
      const expected = sampleDodecaHtmlProcessResult();
      const result = await client.dodecaHtmlProcess(sampleDodecaHtmlProcessInput());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_html_process: payload mismatch");
      }
      console.error(`dodeca_html_process OK`);
      break;
    }
    case "dodeca_execute_code_samples": {
      const expected = sampleDodecaCodeExecutionResult();
      const result = await client.dodecaExecuteCodeSamples(sampleDodecaExecuteSamplesInput());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_execute_code_samples: payload mismatch");
      }
      console.error(`dodeca_execute_code_samples OK`);
      break;
    }
    case "dodeca_load_data": {
      const expected = sampleDodecaLoadDataResult();
      const result = await client.dodecaLoadData(sampleDodecaDataContent(), sampleDodecaDataFormat());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_load_data: payload mismatch");
      }
      console.error(`dodeca_load_data OK`);
      break;
    }
    case "dodeca_parse_and_render": {
      const expected = sampleDodecaParseResult();
      const result = await client.dodecaParseAndRender(
        sampleDodecaMarkdownSourcePath(),
        sampleDodecaMarkdownContent(),
        true,
      );
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_parse_and_render: payload mismatch");
      }
      console.error(`dodeca_parse_and_render OK`);
      break;
    }
    case "echo_dodeca_image_processor_fixture": {
      const payload = sampleDodecaImageProcessorFixture();
      const result = await client.echoDodecaImageProcessorFixture(payload);
      if (!sameHelixDeep(result, payload)) {
        throw new Error("echo_dodeca_image_processor_fixture: payload mismatch");
      }
      console.error(`echo_dodeca_image_processor_fixture OK`);
      break;
    }
    case "echo_dodeca_search_indexer_fixture": {
      const payload = sampleDodecaSearchIndexerFixture();
      const result = await client.echoDodecaSearchIndexerFixture(payload);
      if (!sameHelixDeep(result, payload)) {
        throw new Error("echo_dodeca_search_indexer_fixture: payload mismatch");
      }
      console.error(`echo_dodeca_search_indexer_fixture OK`);
      break;
    }
    case "echo_dodeca_asset_processing_fixture": {
      const payload = sampleDodecaAssetProcessingFixture();
      const result = await client.echoDodecaAssetProcessingFixture(payload);
      if (!sameHelixDeep(result, payload)) {
        throw new Error("echo_dodeca_asset_processing_fixture: payload mismatch");
      }
      console.error(`echo_dodeca_asset_processing_fixture OK`);
      break;
    }

    case "echo_dodeca_small_cell_services_fixture": {
      const payload = sampleDodecaSmallCellServicesFixture();
      const result = await client.echoDodecaSmallCellServicesFixture(payload);
      if (!sameHelixDeep(result, payload)) {
        throw new Error("echo_dodeca_small_cell_services_fixture: payload mismatch");
      }
      console.error(`echo_dodeca_small_cell_services_fixture OK`);
      break;
    }
    case "echo_dodeca_devtools_event": {
      const payload = sampleDodecaDevtoolsEvent();
      const result = await client.echoDodecaDevtoolsEvent(payload);
      if (!sameHelixDeep(result, payload)) {
        throw new Error("echo_dodeca_devtools_event: payload mismatch");
      }
      console.error(`echo_dodeca_devtools_event OK`);
      break;
    }
    case "dodeca_devtools_get_scope": {
      const expected = sampleDodecaScopeEntries();
      const result = await client.dodecaDevtoolsGetScope(["page"]);
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_devtools_get_scope: payload mismatch");
      }
      console.error(`dodeca_devtools_get_scope OK`);
      break;
    }
    case "dodeca_devtools_eval": {
      const expected = sampleDodecaEvalResult();
      const result = await client.dodecaDevtoolsEval("snap-devtools-42", "page.title");
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_devtools_eval: payload mismatch");
      }
      console.error(`dodeca_devtools_eval OK`);
      break;
    }
    case "dodeca_devtools_open_dead_link": {
      const expected = sampleDodecaOpenSourceResult();
      const result = await client.dodecaDevtoolsOpenDeadLink("/guide/", sampleDodecaDeadLinkTarget());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_devtools_open_dead_link: payload mismatch");
      }
      console.error(`dodeca_devtools_open_dead_link OK`);
      break;
    }
    case "dodeca_devtools_edit_load": {
      const expected = sampleDodecaEditLoad();
      const result = await client.dodecaDevtoolsEditLoad("editor-token", "/guide/");
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_devtools_edit_load: payload mismatch");
      }
      console.error(`dodeca_devtools_edit_load OK`);
      break;
    }
    case "dodeca_devtools_edit_preview": {
      const expected = sampleDodecaEditPreview();
      const result = await client.dodecaDevtoolsEditPreview(
        "editor-token",
        "content/guide.md",
        "# Guide\n\nUpdated from browser.",
      );
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_devtools_edit_preview: payload mismatch");
      }
      console.error(`dodeca_devtools_edit_preview OK`);
      break;
    }
    case "dodeca_devtools_edit_save": {
      const expected = sampleDodecaEditSave();
      const result = await client.dodecaDevtoolsEditSave("editor-token", sampleDodecaEditSaveReq());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_devtools_edit_save: payload mismatch");
      }
      console.error(`dodeca_devtools_edit_save OK`);
      break;
    }
    case "dodeca_devtools_edit_upload": {
      const expected = sampleDodecaEditUpload();
      const result = await client.dodecaDevtoolsEditUpload("editor-token", sampleDodecaEditUploadReq());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_devtools_edit_upload: payload mismatch");
      }
      console.error(`dodeca_devtools_edit_upload OK`);
      break;
    }
    case "dodeca_devtools_edit_read": {
      const expected = sampleDodecaEditRead();
      const result = await client.dodecaDevtoolsEditRead("editor-token", "file:///workspace/content/guide.md");
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_devtools_edit_read: payload mismatch");
      }
      console.error(`dodeca_devtools_edit_read OK`);
      break;
    }
    case "dodeca_devtools_edit_list": {
      const expected = sampleDodecaEditList();
      const result = await client.dodecaDevtoolsEditList("editor-token");
      if (!sameHelixDeep(result, expected)) {
        throw new Error("dodeca_devtools_edit_list: payload mismatch");
      }
      console.error(`dodeca_devtools_edit_list OK`);
      break;
    }
    case "echo_styx_value": {
      const payload = sampleStyxValue();
      const result = await client.echoStyxValue(payload);
      if (!sameStyxValue(result, payload)) {
        throw new Error("echo_styx_value: payload mismatch");
      }
      console.error(`echo_styx_value OK`);
      break;
    }
    case "styx_lsp_initialize": {
      const expected = sampleStyxLspInitializeResult();
      const result = await client.styxLspInitialize(sampleStyxLspInitializeParams());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("styx_lsp_initialize: payload mismatch");
      }
      console.error(`styx_lsp_initialize OK`);
      break;
    }
    case "styx_lsp_completions": {
      const expected = sampleStyxLspCompletions();
      const result = await client.styxLspCompletions(sampleStyxLspCompletionParams());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("styx_lsp_completions: payload mismatch");
      }
      console.error(`styx_lsp_completions OK`);
      break;
    }
    case "styx_lsp_hover": {
      const expected = sampleStyxLspHoverResult();
      const result = await client.styxLspHover(sampleStyxLspHoverParams());
      if (result === null || !sameHelixDeep(result, expected)) {
        throw new Error("styx_lsp_hover: payload mismatch");
      }
      console.error(`styx_lsp_hover OK`);
      break;
    }
    case "styx_lsp_inlay_hints": {
      const expected = sampleStyxLspInlayHints();
      const result = await client.styxLspInlayHints(sampleStyxLspInlayHintParams());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("styx_lsp_inlay_hints: payload mismatch");
      }
      console.error(`styx_lsp_inlay_hints OK`);
      break;
    }
    case "styx_lsp_diagnostics": {
      const expected = sampleStyxLspDiagnostics();
      const result = await client.styxLspDiagnostics(sampleStyxLspDiagnosticParams());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("styx_lsp_diagnostics: payload mismatch");
      }
      console.error(`styx_lsp_diagnostics OK`);
      break;
    }
    case "styx_lsp_code_actions": {
      const expected = sampleStyxLspCodeActions();
      const result = await client.styxLspCodeActions(sampleStyxLspCodeActionParams());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("styx_lsp_code_actions: payload mismatch");
      }
      console.error(`styx_lsp_code_actions OK`);
      break;
    }
    case "styx_lsp_definition": {
      const expected = sampleStyxLspLocations();
      const result = await client.styxLspDefinition(sampleStyxLspDefinitionParams());
      if (!sameHelixDeep(result, expected)) {
        throw new Error("styx_lsp_definition: payload mismatch");
      }
      console.error(`styx_lsp_definition OK`);
      break;
    }
    case "styx_lsp_shutdown": {
      await client.styxLspShutdown();
      console.error(`styx_lsp_shutdown OK`);
      break;
    }
    case "styx_host_get_subtree": {
      const result = await client.styxHostGetSubtree(sampleStyxLspGetSubtreeParams());
      if (result === null || !sameStyxValue(result, sampleStyxValue())) {
        throw new Error("styx_host_get_subtree: payload mismatch");
      }
      console.error(`styx_host_get_subtree OK`);
      break;
    }
    case "styx_host_get_document": {
      const result = await client.styxHostGetDocument(sampleStyxLspGetDocumentParams());
      if (result === null || !sameStyxValue(result, sampleStyxValue())) {
        throw new Error("styx_host_get_document: payload mismatch");
      }
      console.error(`styx_host_get_document OK`);
      break;
    }
    case "styx_host_get_source": {
      const result = await client.styxHostGetSource(sampleStyxLspGetSourceParams());
      if (result !== sampleStyxLspSource()) {
        throw new Error("styx_host_get_source: payload mismatch");
      }
      console.error(`styx_host_get_source OK`);
      break;
    }
    case "styx_host_get_schema": {
      const expected = sampleStyxLspSchemaInfo();
      const result = await client.styxHostGetSchema(sampleStyxLspGetSchemaParams());
      if (result === null || !sameHelixDeep(result, expected)) {
        throw new Error("styx_host_get_schema: payload mismatch");
      }
      console.error(`styx_host_get_schema OK`);
      break;
    }
    case "styx_host_offset_to_position": {
      const expected = sampleStyxLspPosition();
      const result = await client.styxHostOffsetToPosition(sampleStyxLspOffsetToPositionParams());
      if (result === null || !sameHelixDeep(result, expected)) {
        throw new Error("styx_host_offset_to_position: payload mismatch");
      }
      console.error(`styx_host_offset_to_position OK`);
      break;
    }
    case "styx_host_position_to_offset": {
      const result = await client.styxHostPositionToOffset(sampleStyxLspPositionToOffsetParams());
      if (result !== 16) {
        throw new Error("styx_host_position_to_offset: payload mismatch");
      }
      console.error(`styx_host_position_to_offset OK`);
      break;
    }
    case "stax_flamegraph": {
      const params = sampleStaxViewParams();
      const expected = sampleStaxFlamegraphUpdate(params);
      const result = await client.staxFlamegraph(params);
      if (!sameStaxFlamegraphUpdate(result, expected)) {
        throw new Error("stax_flamegraph: payload mismatch");
      }
      console.error(`stax_flamegraph OK`);
      break;
    }
    case "echo_stax_flamegraph_update": {
      const params = sampleStaxViewParams();
      const update = sampleStaxFlamegraphUpdate(params);
      const result = await client.echoStaxFlamegraphUpdate(update);
      if (!sameStaxFlamegraphUpdate(result, update)) {
        throw new Error("echo_stax_flamegraph_update: payload mismatch");
      }
      console.error(`echo_stax_flamegraph_update OK`);
      break;
    }
    case "stax_subscribe_flamegraph_updates": {
      const [updateTx, updateRx] = channel<StaxFlamegraphUpdate>();
      const resultPromise = client.staxSubscribeFlamegraphUpdates(updateTx);
      await waitForBound(updateRx);
      const received: StaxFlamegraphUpdate[] = [];
      const recvTask = (async () => {
        for await (const update of updateRx) received.push(update);
      })();
      await resultPromise;
      await recvTask;
      const expected = sampleStaxFlamegraphUpdates();
      if (
        received.length !== expected.length
        || !received.every((update, idx) => sameStaxFlamegraphUpdate(update, expected[idx] as StaxFlamegraphUpdate))
      ) {
        throw new Error("stax_subscribe_flamegraph_updates: payload mismatch");
      }
      console.error(`stax_subscribe_flamegraph_updates OK`);
      break;
    }
    case "echo_stax_linux_broker_control": {
      const fixture = sampleStaxLinuxBrokerControlFixture();
      const result = await client.echoStaxLinuxBrokerControl(fixture);
      if (!sameHelixDeep(result, fixture)) {
        throw new Error("echo_stax_linux_broker_control: payload mismatch");
      }
      console.error(`echo_stax_linux_broker_control OK`);
      break;
    }
    case "stax_macos_record": {
      const [batchTx, batchRx] = channel<StaxMacKdBufBatch>();
      const resultPromise = client.staxMacosRecord(sampleStaxMacosConfig(), batchTx);
      await waitForBound(batchRx);
      const received: StaxMacKdBufBatch[] = [];
      const recvTask = (async () => {
        for await (const batch of batchRx) received.push(batch);
      })();
      const result = await resultPromise;
      await recvTask;
      if (!result.ok || !sameHelixDeep(result.value, sampleStaxMacosRecordSummary())) {
        throw new Error("stax_macos_record: summary mismatch");
      }
      const expected = sampleStaxMacosBatches();
      if (!sameStaxMacBatches(received, expected)) {
        throw new Error("stax_macos_record: batches mismatch");
      }
      console.error(`stax_macos_record OK`);
      break;
    }
    case "echo_hotmeal_live_reload_event": {
      for (const event of sampleHotmealLiveReloadEvents()) {
        const result = await client.echoHotmealLiveReloadEvent(event);
        if (!sameHotmealLiveReloadEvent(result, event)) {
          throw new Error("echo_hotmeal_live_reload_event: payload mismatch");
        }
      }
      console.error(`echo_hotmeal_live_reload_event OK`);
      break;
    }
    case "echo_hotmeal_apply_patches_result": {
      const payload = sampleHotmealApplyPatchesResult();
      const result = await client.echoHotmealApplyPatchesResult(payload);
      if (!sameHotmealApplyPatchesResult(result, payload)) {
        throw new Error("echo_hotmeal_apply_patches_result: payload mismatch");
      }
      console.error(`echo_hotmeal_apply_patches_result OK`);
      break;
    }
    case "hotmeal_live_reload_subscribe": {
      await client.hotmealLiveReloadSubscribe(sampleHotmealRoute());
      console.error(`hotmeal_live_reload_subscribe OK`);
      break;
    }
    case "hotmeal_live_reload_on_event": {
      for (const event of sampleHotmealLiveReloadEvents()) {
        await client.hotmealLiveReloadOnEvent(event);
      }
      console.error(`hotmeal_live_reload_on_event OK`);
      break;
    }
    case "echo_helix_stream_metrics": {
      const metrics = sampleHelixStreamMetrics();
      const result = await client.echoHelixStreamMetrics(metrics);
      if (!sameHelixStreamMetrics(result, metrics)) {
        throw new Error("echo_helix_stream_metrics: payload mismatch");
      }
      console.error(`echo_helix_stream_metrics OK`);
      break;
    }
    case "echo_helix_verify_evidence": {
      const digest = sampleHelixVerifyEvidence();
      const result = await client.echoHelixVerifyEvidence(digest);
      if (!sameHelixVerifyEvidenceDigest(result, digest)) {
        throw new Error("echo_helix_verify_evidence: payload mismatch");
      }
      console.error(`echo_helix_verify_evidence OK`);
      break;
    }
    case "helix_subscribe_pulses": {
      const [pulseTx, pulseRx] = channel<HelixPulseAvailable>();
      const resultPromise = client.helixSubscribePulses(pulseTx);
      await waitForBound(pulseRx);
      const received: HelixPulseAvailable[] = [];
      const recvTask = (async () => {
        for await (const pulse of pulseRx) received.push(pulse);
      })();
      await resultPromise;
      await recvTask;
      const expected = sampleHelixPulses();
      if (!sameHelixPulses(received, expected)) {
        throw new Error("helix_subscribe_pulses: payload mismatch");
      }
      console.error(`helix_subscribe_pulses OK`);
      break;
    }
    case "helix_pulse_bundle": {
      const expected = sampleHelixPulseBundle();
      const result = await client.helixPulseBundle(102n, sampleHelixPulseBundleFields());
      if (!sameHelixPulseBundle(result, expected)) {
        throw new Error("helix_pulse_bundle: payload mismatch");
      }
      console.error(`helix_pulse_bundle OK`);
      break;
    }
    case "helix_trace_service_surface": {
      const expected = sampleHelixTraceServiceSurface();
      const result = await client.helixTraceServiceSurface();
      if (!sameHelixTraceServiceSurface(result, expected)) {
        throw new Error("helix_trace_service_surface: payload mismatch");
      }
      console.error(`helix_trace_service_surface OK`);
      break;
    }
    case "tracey_status": {
      const expected = sampleTraceyStatusResponse();
      const result = await client.traceyStatus();
      if (!sameTraceyStatusResponse(result, expected)) {
        throw new Error("tracey_status: payload mismatch");
      }
      console.error(`tracey_status OK`);
      break;
    }
    case "tracey_core_control": {
      const uncovered = await client.traceyUncovered(sampleTraceyQueryRequest());
      if (!sameTraceyUncoveredResponse(uncovered, sampleTraceyUncoveredResponse())) {
        throw new Error("tracey_uncovered: payload mismatch");
      }

      const untested = await client.traceyUntested(sampleTraceyUntestedRequest());
      if (!sameTraceyUntestedResponse(untested, sampleTraceyUntestedResponse())) {
        throw new Error("tracey_untested: payload mismatch");
      }

      const stale = await client.traceyStale(sampleTraceyStaleRequest());
      if (!sameTraceyStaleResponse(stale, sampleTraceyStaleResponse())) {
        throw new Error("tracey_stale: payload mismatch");
      }

      const unmapped = await client.traceyUnmapped(sampleTraceyUnmappedRequest());
      if (!sameTraceyUnmappedResponse(unmapped, sampleTraceyUnmappedResponse())) {
        throw new Error("tracey_unmapped: payload mismatch");
      }

      const config = await client.traceyConfig();
      if (!sameTraceyApiConfig(config, sampleTraceyApiConfig())) {
        throw new Error("tracey_config: payload mismatch");
      }

      await client.traceyVfsOpen("src/lib.rs", sampleTraceyLspContent());
      await client.traceyVfsChange("src/lib.rs", "// r[verify rpc.channel.direct-args]\n");
      await client.traceyVfsClose("src/lib.rs");

      const reload = await client.traceyReload();
      if (!sameTraceyReloadResponse(reload, sampleTraceyReloadResponse())) {
        throw new Error("tracey_reload: payload mismatch");
      }

      const version = await client.traceyVersion();
      if (version !== 13n) {
        throw new Error(`tracey_version: expected 13, got ${version}`);
      }

      const health = await client.traceyHealth();
      if (!sameTraceyHealthResponse(health, sampleTraceyHealthResponse())) {
        throw new Error("tracey_health: payload mismatch");
      }

      await client.traceyShutdown();
      console.error(`tracey_core_control OK`);
      break;
    }
    case "tracey_rule": {
      const result = await client.traceyRule(traceyRuleId("rpc.channel.direct-args", 1));
      if (!sameTraceyRuleInfo(result, sampleTraceyRuleInfo())) {
        throw new Error("tracey_rule known: payload mismatch");
      }
      const missing = await client.traceyRule(traceyRuleId("missing.rule", 1));
      if (missing !== null) {
        throw new Error("tracey_rule missing: expected null");
      }
      console.error(`tracey_rule OK`);
      break;
    }
    case "tracey_dashboard": {
      const forward = await client.traceyForward("vox", "rust");
      if (!sameTraceyDashboardValue(forward, sampleTraceyForwardResponse())) {
        throw new Error("tracey_forward: payload mismatch");
      }
      const missingForward = await client.traceyForward("missing", "rust");
      if (missingForward !== null) {
        throw new Error("tracey_forward missing: expected null");
      }

      const reverse = await client.traceyReverse("vox", "rust");
      if (!sameTraceyDashboardValue(reverse, sampleTraceyReverseResponse())) {
        throw new Error("tracey_reverse: payload mismatch");
      }

      const file = await client.traceyFile(sampleTraceyFileRequest());
      if (!sameTraceyDashboardValue(file, sampleTraceyFileResponse())) {
        throw new Error("tracey_file: payload mismatch");
      }

      const specContent = await client.traceySpecContent("vox", "rust");
      if (!sameTraceyDashboardValue(specContent, sampleTraceySpecContentResponse())) {
        throw new Error("tracey_spec_content: payload mismatch");
      }

      const search = await client.traceySearch("channel", 10);
      if (!sameTraceyDashboardValue(search, sampleTraceySearchResults())) {
        throw new Error("tracey_search: payload mismatch");
      }

      const updateOk = await client.traceyUpdateFileRange(sampleTraceyUpdateFileRangeRequest());
      if (!updateOk.ok) {
        throw new Error("tracey_update_file_range ok: expected success");
      }
      const updateConflict = await client.traceyUpdateFileRange(sampleTraceyUpdateFileRangeConflictRequest());
      if (updateConflict.ok || !sameTraceyDashboardValue(updateConflict.error, sampleTraceyUpdateError())) {
        throw new Error("tracey_update_file_range conflict: expected user error");
      }

      const excludeOk = await client.traceyConfigAddExclude(sampleTraceyConfigPatternRequest());
      if (!excludeOk.ok) {
        throw new Error("tracey_config_add_exclude ok: expected success");
      }
      const excludeBad = await client.traceyConfigAddExclude(sampleTraceyBadConfigPatternRequest());
      if (excludeBad.ok || excludeBad.error !== "invalid pattern") {
        throw new Error("tracey_config_add_exclude bad pattern: expected user error");
      }
      const includeOk = await client.traceyConfigAddInclude(sampleTraceyConfigPatternRequest());
      if (!includeOk.ok) {
        throw new Error("tracey_config_add_include: expected success");
      }

      console.error(`tracey_dashboard OK`);
      break;
    }
    case "tracey_validate": {
      const expected = sampleTraceyValidationResult();
      const result = await client.traceyValidate(sampleTraceyValidateRequest());
      if (!sameTraceyValidationResult(result, expected)) {
        throw new Error("tracey_validate: payload mismatch");
      }
      console.error(`tracey_validate OK`);
      break;
    }
    case "tracey_lsp_surface": {
      const testFile = await client.traceyIsTestFile("spec/spec-tests/tests/cases/testbed.rs");
      if (!testFile) {
        throw new Error("tracey_is_test_file: expected true for tests path");
      }
      const sourceFile = await client.traceyIsTestFile("src/lib.rs");
      if (sourceFile) {
        throw new Error("tracey_is_test_file: expected false for source path");
      }

      const hover = await client.traceyLspHover(sampleTraceyLspPositionRequest());
      if (!sameTraceyHoverInfo(hover, sampleTraceyHoverInfo())) {
        throw new Error("tracey_lsp_hover: payload mismatch");
      }

      const definition = await client.traceyLspDefinition(sampleTraceyLspPositionRequest());
      if (!sameTraceyLspLocations(definition, sampleTraceyLspLocations())) {
        throw new Error("tracey_lsp_definition: payload mismatch");
      }

      const implementation = await client.traceyLspImplementation(sampleTraceyLspPositionRequest());
      if (!sameTraceyLspLocations(implementation, sampleTraceyLspLocations())) {
        throw new Error("tracey_lsp_implementation: payload mismatch");
      }

      const references = await client.traceyLspReferences(sampleTraceyLspReferencesRequest());
      if (!sameTraceyLspLocations(references, sampleTraceyLspLocations())) {
        throw new Error("tracey_lsp_references: payload mismatch");
      }

      const completions = await client.traceyLspCompletions(sampleTraceyLspPositionRequest());
      if (!sameTraceyLspCompletions(completions, sampleTraceyLspCompletions())) {
        throw new Error("tracey_lsp_completions: payload mismatch");
      }

      const documentSymbols = await client.traceyLspDocumentSymbols(sampleTraceyLspDocumentRequest());
      if (!sameTraceyLspSymbols(documentSymbols, sampleTraceyLspSymbols())) {
        throw new Error("tracey_lsp_document_symbols: payload mismatch");
      }

      const workspaceSymbols = await client.traceyLspWorkspaceSymbols("rpc.channel");
      if (!sameTraceyLspSymbols(workspaceSymbols, sampleTraceyLspSymbols())) {
        throw new Error("tracey_lsp_workspace_symbols: payload mismatch");
      }

      const semanticTokens = await client.traceyLspSemanticTokens(sampleTraceyLspDocumentRequest());
      if (!sameTraceyLspSemanticTokens(semanticTokens, sampleTraceyLspSemanticTokens())) {
        throw new Error("tracey_lsp_semantic_tokens: payload mismatch");
      }

      const codeLens = await client.traceyLspCodeLens(sampleTraceyLspDocumentRequest());
      if (!sameTraceyLspCodeLensList(codeLens, sampleTraceyLspCodeLens())) {
        throw new Error("tracey_lsp_code_lens: payload mismatch");
      }

      const inlayHints = await client.traceyLspInlayHints(sampleTraceyLspInlayHintsRequest());
      if (!sameTraceyLspInlayHints(inlayHints, sampleTraceyLspInlayHints())) {
        throw new Error("tracey_lsp_inlay_hints: payload mismatch");
      }

      const prepareRename = await client.traceyLspPrepareRename(sampleTraceyLspPositionRequest());
      if (!sameTraceyPrepareRenameResult(prepareRename, sampleTraceyPrepareRenameResult())) {
        throw new Error("tracey_lsp_prepare_rename: payload mismatch");
      }

      const textEdits = await client.traceyLspRename(sampleTraceyLspRenameRequest());
      if (!sameTraceyLspTextEdits(textEdits, sampleTraceyLspTextEdits())) {
        throw new Error("tracey_lsp_rename: payload mismatch");
      }

      const codeActions = await client.traceyLspCodeActions(sampleTraceyLspPositionRequest());
      if (!sameTraceyLspCodeActions(codeActions, sampleTraceyLspCodeActions())) {
        throw new Error("tracey_lsp_code_actions: payload mismatch");
      }

      const highlights = await client.traceyLspDocumentHighlight(sampleTraceyLspPositionRequest());
      if (!sameTraceyLspLocations(highlights, sampleTraceyLspLocations())) {
        throw new Error("tracey_lsp_document_highlight: payload mismatch");
      }

      console.error(`tracey_lsp_surface OK`);
      break;
    }
    case "tracey_lsp_workspace_diagnostics": {
      const expected = sampleTraceyLspWorkspaceDiagnostics();
      const result = await client.traceyLspWorkspaceDiagnostics();
      if (!sameTraceyLspFileDiagnostics(result, expected)) {
        throw new Error("tracey_lsp_workspace_diagnostics: payload mismatch");
      }
      console.error(`tracey_lsp_workspace_diagnostics OK`);
      break;
    }
    case "tracey_subscribe_updates": {
      const [updateTx, updateRx] = channel<TraceyDataUpdate>();
      const resultPromise = client.traceySubscribeUpdates(updateTx);
      await waitForBound(updateRx);
      const received: TraceyDataUpdate[] = [];
      const recvTask = (async () => {
        for await (const update of updateRx) received.push(update);
      })();
      await resultPromise;
      await recvTask;
      const expected = sampleTraceyUpdates();
      if (!sameTraceyUpdates(received, expected)) {
        throw new Error("tracey_subscribe_updates: payload mismatch");
      }
      console.error(`tracey_subscribe_updates OK`);
      break;
    }
    case "dibs_list": {
      const result = await client.dibsList(sampleDibsListRequest());
      if (!result.ok || !sameDibsListResponse(result.value, sampleDibsListResponse())) {
        throw new Error("dibs_list: response mismatch");
      }
      console.error(`dibs_list OK`);
      break;
    }
    case "dibs_schema": {
      const result = await client.dibsSchema();
      if (!sameHelixDeep(result, sampleDibsSchema())) {
        throw new Error("dibs_schema: response mismatch");
      }
      console.error(`dibs_schema OK`);
      break;
    }
    case "dibs_get": {
      const result = await client.dibsGet(sampleDibsGetRequest());
      if (!result.ok || result.value === null || !sameDibsRow(result.value, sampleDibsRowOne())) {
        throw new Error("dibs_get: response mismatch");
      }
      console.error(`dibs_get OK`);
      break;
    }
    case "dibs_create": {
      const result = await client.dibsCreate(sampleDibsCreateRequest());
      if (!result.ok || !sameDibsRow(result.value, sampleDibsCreateResponse())) {
        throw new Error("dibs_create: response mismatch");
      }
      console.error(`dibs_create OK`);
      break;
    }
    case "dibs_update": {
      const result = await client.dibsUpdate(sampleDibsUpdateRequest());
      if (!result.ok || !sameDibsRow(result.value, sampleDibsUpdateResponse())) {
        throw new Error("dibs_update: response mismatch");
      }
      console.error(`dibs_update OK`);
      break;
    }
    case "dibs_delete": {
      const result = await client.dibsDelete(sampleDibsDeleteRequest());
      if (!result.ok || result.value !== 1n) {
        throw new Error("dibs_delete: response mismatch");
      }
      console.error(`dibs_delete OK`);
      break;
    }
    case "dibs_migration_status": {
      const result = await client.dibsMigrationStatus(sampleDibsMigrationStatusRequest());
      if (!result.ok || !sameHelixDeep(result.value, sampleDibsMigrationStatus())) {
        throw new Error("dibs_migration_status: response mismatch");
      }
      console.error(`dibs_migration_status OK`);
      break;
    }
    case "dibs_migrate": {
      const [logTx, logRx] = channel<DibsMigrationLog>();
      const resultPromise = client.dibsMigrate(sampleDibsMigrateRequest(), logTx);
      await waitForBound(logRx);
      const received: DibsMigrationLog[] = [];
      const recvTask = (async () => {
        for await (const logEntry of logRx) received.push(logEntry);
      })();
      const result = await resultPromise;
      await recvTask;
      if (!result.ok || !sameDibsMigrateResult(result.value, sampleDibsMigrateResult())) {
        throw new Error("dibs_migrate: result mismatch");
      }
      if (!sameDibsLogs(received, sampleDibsLogs())) {
        throw new Error("dibs_migrate: logs mismatch");
      }
      console.error(`dibs_migrate OK`);
      break;
    }
    case "pipelining": {
      const promises = Array.from({ length: 10 }, (_, i) =>
        client.echo(`msg${i}`).then(r => {
          if (r !== `msg${i}`) throw new Error(`pipelining[${i}]: expected msg${i}, got ${r}`);
        })
      );
      await Promise.all(promises);
      console.error(`pipelining OK (10 concurrent echo calls)`);
      break;
    }
    case "sum_large": {
      // Client gives Rx to server (server receives), client keeps Tx and sends.
      // Bind rx first via the call, then start sending via tx.
      const [tx, rx] = channel<number>();
      const n = 100;
      const callPromise = client.sumLarge(rx);  // give rx to server — binds it
      // tx is now bound; send n items (> initial credit, tests flow control)
      for (let i = 0; i < n; i++) await tx.send(i);
      tx.close();
      const result = await callPromise;
      const expected = BigInt(n * (n - 1) / 2);
      if (result !== expected) throw new Error(`sum_large: expected ${expected}, got ${result}`);
      console.error(`sum_large OK: ${result}`);
      break;
    }
    case "generate_large": {
      // Client gives Tx to server (server sends), client keeps Rx and receives.
      // Bind tx first via the call, then start draining rx concurrently to grant credit.
      const [tx, rx] = channel<number>();
      const n = 100;
      const callPromise = client.generateLarge(n, tx);  // give tx to server — binds it
      // rx is now bound; drain it concurrently so we grant credit back to the server
      const received: number[] = [];
      const recvTask = (async () => {
        for await (const v of rx) received.push(v);
      })();
      await Promise.all([callPromise, recvTask]);
      if (received.length !== n) throw new Error(`generate_large: expected ${n}, got ${received.length}`);
      for (let i = 0; i < n; i++) {
        if (received[i] !== i) throw new Error(`generate_large[${i}]: expected ${i}, got ${received[i]}`);
      }
      console.error(`generate_large OK: ${received.length} items`);
      break;
    }
    case "sum_client_to_server": {
      // Client gives Rx to server (server receives), client keeps Tx and sends.
      const [tx, rx] = channel<number>();
      const callPromise = client.sum(rx);  // give rx to server — binds it
      for (const n of [1, 2, 3, 4, 5]) await tx.send(n);
      tx.close();
      const result = await callPromise;
      if (result !== 15n) throw new Error(`sum_client_to_server: expected 15n, got ${result}`);
      console.error(`sum_client_to_server OK: ${result}`);
      break;
    }
    case "transform_bidi": {
      // Client gives inputRx to server (server receives strings from client).
      // Client gives outputTx to server (server sends strings back to client).
      // Client keeps inputTx (sends) and outputRx (receives).
      const [inputTx, inputRx] = channel<string>();
      const [outputTx, outputRx] = channel<string>();
      const messages = ["alpha", "beta", "gamma"];
      const callPromise = client.transform(inputRx, outputTx);  // bind both — now inputTx & outputRx usable
      const received: string[] = [];
      const recvTask = (async () => {
        for await (const s of outputRx) received.push(s);
      })();
      for (const m of messages) await inputTx.send(m);
      inputTx.close();
      await callPromise;
      await recvTask;
      if (received.length !== messages.length || messages.some((m, i) => received[i] !== m)) {
        throw new Error(`transform_bidi: expected ${JSON.stringify(messages)}, got ${JSON.stringify(received)}`);
      }
      console.error(`transform_bidi OK`);
      break;
    }
    case "dodeca_byte_tunnel": {
      const [inboundTx, inboundRx] = channel<Uint8Array>();
      const [outboundTx, outboundRx] = channel<Uint8Array>();
      const chunks = [
        new Uint8Array([0, 1, 2, 3]),
        new Uint8Array(),
        new Uint8Array([255, 254, 253]),
      ];
      const callPromise = client.dodecaByteTunnel(inboundRx, outboundTx);
      const received: Uint8Array[] = [];
      await waitForBound(inboundTx, outboundRx);
      const recvTask = (async () => {
        for await (const chunk of outboundRx) received.push(chunk);
      })();
      for (const chunk of chunks) await inboundTx.send(chunk);
      inboundTx.close();
      await callPromise;
      await recvTask;
      if (
        received.length !== chunks.length
        || chunks.some((chunk, idx) => !sameBytes(chunk, received[idx] ?? new Uint8Array()))
      ) {
        throw new Error(`dodeca_byte_tunnel: expected ${chunks.length} chunks, got ${received.length}`);
      }
      console.error(`dodeca_byte_tunnel OK`);
      break;
    }
    case "dodeca_devtools_lsp": {
      const [clientTx, clientRx] = channel<string>();
      const [serverTx, serverRx] = channel<string>();
      const chunks = [
        "Content-Length: 37\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1}",
        "{\"method\":\"textDocument/didOpen\"}",
      ];
      const expected = chunks.map((chunk) => `lsp:${chunk}`);
      const callPromise = client.dodecaDevtoolsLsp("editor-token", clientRx, serverTx);
      const received: string[] = [];
      await waitForBound(clientTx, serverRx);
      const recvTask = (async () => {
        for await (const chunk of serverRx) received.push(chunk);
      })();
      for (const chunk of chunks) await clientTx.send(chunk);
      clientTx.close();
      await callPromise;
      await recvTask;
      if (received.length !== expected.length || expected.some((chunk, idx) => received[idx] !== chunk)) {
        throw new Error(`dodeca_devtools_lsp: expected ${JSON.stringify(expected)}, got ${JSON.stringify(received)}`);
      }
      console.error(`dodeca_devtools_lsp OK`);
      break;
    }
    case "post_reply_generate": {
      const [tx, rx] = channel<number>();

      await client.postReplyGenerate(tx);

      const received: number[] = [];
      for await (const n of rx) {
        received.push(n);
      }

      const expected = [0, 1, 2, 3, 4];
      if (received.length !== expected.length || expected.some((value, idx) => received[idx] !== value)) {
        throw new Error(`post_reply_generate: expected ${JSON.stringify(expected)}, got ${JSON.stringify(received)}`);
      }
      console.error(`post_reply_generate OK`);
      break;
    }
    case "post_reply_sum": {
      const [inputTx, inputRx] = channel<number>();
      const [resultTx, resultRx] = channel<bigint>();

      const call = client.postReplySum(inputRx, resultTx);

      for (const n of [1, 2, 3, 4, 5]) {
        await inputTx.send(n);
      }
      inputTx.close();

      const total = await resultRx.recv();
      if (total !== 15n) {
        throw new Error(`post_reply_sum: expected 15n, got ${String(total)}`);
      }

      const extra = await resultRx.recv();
      if (extra !== null) {
        throw new Error(`post_reply_sum: expected result channel close, got extra value ${String(extra)}`);
      }

      await call;
      console.error(`post_reply_sum OK`);
      break;
    }
    default:
      throw new Error(`unknown CLIENT_SCENARIO: ${scenario}`);
  }
  } finally {
    // r[impl hosted.subject.lifecycle]
    handle.shutdown();
    await connection.closed().catch(() => {});
  }

}

async function runServerListen() {
  // Bind a TCP server, announce the address, serve one connection.
  // Used by cross-language harness tests where another subject is the client.
  const listenPort = process.env.LISTEN_PORT ? parseInt(process.env.LISTEN_PORT) : 0;

  const tcpServer = createTcpServer();
  await new Promise<void>((resolve) => tcpServer.listen(listenPort, "127.0.0.1", resolve));
  const { port } = tcpServer.address() as AddressInfo;

  // Signal readiness to the harness — it reads this line from stdout.
  process.stdout.write(`LISTEN_ADDR=127.0.0.1:${port}\n`);
  console.error(`server-listen mode: bound to 127.0.0.1:${port}`);

  const socket = await new Promise<import("net").Socket>((resolve) => {
    tcpServer.once("connection", (s) => {
      tcpServer.close();
      resolve(s);
    });
  });

  const connection = await accept(acceptTcp(socket), {
    metadata: voxServiceMetadata("Testbed"),
    onLane: (lane) => {
      const driver = new Driver(
        lane,
        new TestbedDispatcher(new TestbedService()),
      );
      void driver.run();
    },
  });
  const driver = new Driver(
    connection.lane(),
    new TestbedDispatcher(new TestbedService()),
  );
  const handle = connection.handle();

  try {
    await driver.run();
  } catch (e) {
    if (e instanceof ConnectionError) return;
    throw e;
  } finally {
    // r[impl hosted.subject.lifecycle]
    handle.shutdown();
  }
}

async function main() {
  const mode = process.env.SUBJECT_MODE ?? "server";

  await withSubjectTimeout(mode, async () => {
    if (mode === "client") {
      await runClient();
    } else if (mode === "server-listen") {
      await runServerListen();
    } else {
      await runServer();
    }
  });
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
