import { useEffect, useRef } from "react";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  GutterMarker,
  gutterLineClass,
  drawSelection,
  keymap,
  lineNumbers,
  ViewPlugin,
  WidgetType,
  type ViewUpdate,
} from "@codemirror/view";
import { type EditorState, RangeSet, StateEffect, StateField } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { byteOffsetMap, captureClass, type CaptureRange, selectNonOverlapping } from "./highlight";

export interface EditorSpan {
  start_byte: number;
  end_byte: number;
  start_row: number;
  start_column: number;
  end_row: number;
  end_column: number;
}

export interface EditorDiagnostic {
  stage: string;
  message: string;
  span: EditorSpan;
}

export interface SourceEdit {
  start_byte: number;
  old_end_byte: number;
  new_end_byte: number;
}

export interface EditorJump {
  start_byte: number;
  end_byte: number;
  nonce: number;
}

/** Language-level IDE bindings (vix Ring 2): symbols, references, unresolved names. */
export interface IdeSymbol {
  name: string;
  kind: string;
  start: number;
  end: number;
}

export interface IdeRef {
  start: number;
  end: number;
  symbol: number;
}

export interface IdeUnresolved {
  name: string;
  start: number;
  end: number;
}

export interface IdeInfo {
  error: string | null;
  symbols: IdeSymbol[];
  refs: IdeRef[];
  unresolved: IdeUnresolved[];
}

/** Bindings plus the exact input they were computed for — stale bindings are unusable. */
export type IdeState = { ide: IdeInfo; input: string } | null;

export interface SourceEditorProps {
  input: string;
  captures: CaptureRange[];
  diagnostic: EditorDiagnostic | null;
  ide: IdeState;
  jump: EditorJump | null;
  onCursorByte?: (byte: number) => void;
  onChange: (value: string, edit: SourceEdit | null) => void;
}

type DecorationState = {
  decorations: DecorationSet;
  errorLine: number | null;
};

const setDecorations = StateEffect.define<DecorationState>();

const decorationField = StateField.define<DecorationState>({
  create: () => ({ decorations: Decoration.none, errorLine: null }),
  update(value, tr) {
    let next: DecorationState = {
      decorations: value.decorations.map(tr.changes),
      errorLine: value.errorLine,
    };
    for (const effect of tr.effects) {
      if (effect.is(setDecorations)) {
        next = effect.value;
      }
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field, (value) => value.decorations),
});

class ErrorGutterMarker extends GutterMarker {
  elementClass = "cm-gutter-error";
}

const errorGutterMarker = new ErrorGutterMarker();

const errorGutterClass = gutterLineClass.compute([decorationField], (state) => {
  const errorLine = state.field(decorationField).errorLine;
  if (errorLine === null || errorLine < 0 || errorLine >= state.doc.lines) {
    return RangeSet.empty;
  }
  const line = state.doc.line(errorLine + 1);
  return RangeSet.of([errorGutterMarker.range(line.from)]);
});

class LintWidget extends WidgetType {
  constructor(
    readonly stage: string,
    readonly location: string,
    readonly message: string,
  ) {
    super();
  }

  eq(other: LintWidget) {
    return other.stage === this.stage && other.location === this.location && other.message === this.message;
  }

  toDOM() {
    const wrap = document.createElement("div");
    wrap.className = "code-lint cm-lint-widget";

    const head = document.createElement("div");
    head.className = "code-lint-head";
    const stage = document.createElement("span");
    stage.className = "code-lint-stage";
    stage.textContent = this.stage;
    const location = document.createElement("span");
    location.className = "code-lint-loc";
    location.textContent = this.location;
    head.append(stage, location);

    const body = document.createElement("div");
    body.className = "code-lint-body";
    body.textContent = this.message;

    wrap.append(head, body);
    return wrap;
  }

  ignoreEvent() {
    return false;
  }
}

function buildDecorations(
  state: EditorState,
  captures: CaptureRange[],
  diagnostic: EditorDiagnostic | null,
): DecorationState {
  const doc = state.doc.toString();
  const length = doc.length;
  const byteMap = byteOffsetMap(doc);
  const entries: { from: number; to: number; deco: Decoration }[] = [];

  for (const selected of selectNonOverlapping(captures, byteMap, length)) {
    entries.push({
      from: selected.from,
      to: selected.to,
      deco: Decoration.mark({ class: `source-capture ${captureClass(selected.capture.capture_name)}` }),
    });
  }

  let errorLine: number | null = null;
  if (diagnostic) {
    const span = diagnostic.span;
    const from = Math.min(byteMap[span.start_byte] ?? length, length);
    const sameLine = span.end_row === span.start_row;
    const to = Math.min(sameLine ? byteMap[span.end_byte] ?? from : from, length);
    if (to > from) {
      entries.push({ from, to, deco: Decoration.mark({ class: "cm-err-token" }) });
    }

    if (span.start_row >= 0 && span.start_row < state.doc.lines) {
      errorLine = span.start_row;
      const line = state.doc.line(span.start_row + 1);
      entries.push({
        from: line.to,
        to: line.to,
        deco: Decoration.widget({
          widget: new LintWidget(
            diagnostic.stage,
            `${span.start_row + 1}:${span.start_column + 1}`,
            diagnostic.message,
          ),
          block: true,
          side: 1,
        }),
      });
    }
  }

  entries.sort((left, right) => left.from - right.from || left.to - right.to);
  const decorations = Decoration.set(
    entries.map((entry) => entry.deco.range(entry.from, entry.to)),
    true,
  );
  return { decorations, errorLine };
}

// ---------------------------------------------------------------------------
// IDE ops (vix Ring 2): occurrence highlighting, cmd-click go-to-def, F2 rename.
// The binder runs in the parse worker; here we only project its spans. Every op
// checks that the bindings were computed for the CURRENT document — during the
// parse round trip after an edit they're stale and everything quietly no-ops.
// ---------------------------------------------------------------------------

/** Poked when fresh bindings arrive so the plugin recomputes without an edit. */
const ideRefresh = StateEffect.define<null>();

type IdeByteSpan = { start: number; end: number };

function ideSymbolAt(ide: IdeInfo, byte: number): number | null {
  const hit = (span: IdeByteSpan) => span.start <= byte && byte <= span.end;
  for (const ref of ide.refs) {
    if (hit(ref)) return ref.symbol;
  }
  for (let i = 0; i < ide.symbols.length; i += 1) {
    if (hit(ide.symbols[i])) return i;
  }
  return null;
}

function ideOccurrences(ide: IdeInfo, symbol: number): IdeByteSpan[] {
  const out: IdeByteSpan[] = [{ start: ide.symbols[symbol].start, end: ide.symbols[symbol].end }];
  for (const ref of ide.refs) {
    if (ref.symbol === symbol) out.push({ start: ref.start, end: ref.end });
  }
  out.sort((a, b) => a.start - b.start);
  return out;
}

function freshIde(state: EditorState, entry: IdeState): IdeInfo | null {
  if (!entry || entry.ide.error !== null) return null;
  return entry.input === state.doc.toString() ? entry.ide : null;
}

function buildIdeDecorations(state: EditorState, entry: IdeState): DecorationSet {
  const ide = freshIde(state, entry);
  if (!ide) return Decoration.none;
  const doc = state.doc.toString();
  const byteMap = byteOffsetMap(doc);
  const toChar = (byte: number) => Math.min(byteMap[byte] ?? doc.length, doc.length);
  const entries: { from: number; to: number; deco: Decoration }[] = [];

  for (const u of ide.unresolved) {
    const [from, to] = [toChar(u.start), toChar(u.end)];
    if (to > from) entries.push({ from, to, deco: Decoration.mark({ class: "cm-unresolved" }) });
  }

  const cursor = state.selection.main.head;
  const symbol = ideSymbolAt(ide, utf8ByteLength(doc.slice(0, cursor)));
  if (symbol !== null) {
    const def = ide.symbols[symbol];
    for (const occ of ideOccurrences(ide, symbol)) {
      const [from, to] = [toChar(occ.start), toChar(occ.end)];
      const isDef = occ.start === def.start && occ.end === def.end;
      if (to > from) {
        entries.push({ from, to, deco: Decoration.mark({ class: isDef ? "cm-occ cm-occ-def" : "cm-occ" }) });
      }
    }
  }

  entries.sort((left, right) => left.from - right.from || left.to - right.to);
  return Decoration.set(
    entries.map((entry) => entry.deco.range(entry.from, entry.to)),
    true,
  );
}

function ideExtensions(ideRef: { current: IdeState }) {
  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet = Decoration.none;

      constructor(view: EditorView) {
        this.decorations = buildIdeDecorations(view.state, ideRef.current);
      }

      update(update: ViewUpdate) {
        const poked = update.transactions.some((tr) => tr.effects.some((e) => e.is(ideRefresh)));
        if (update.docChanged || update.selectionSet || poked) {
          this.decorations = buildIdeDecorations(update.state, ideRef.current);
        }
      }
    },
    { decorations: (value) => value.decorations },
  );

  const goToDefinition = EditorView.domEventHandlers({
    mousedown(event, view) {
      if (!(event.metaKey || event.ctrlKey)) return false;
      const ide = freshIde(view.state, ideRef.current);
      if (!ide) return false;
      const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
      if (pos === null) return false;
      const doc = view.state.doc.toString();
      const symbol = ideSymbolAt(ide, utf8ByteLength(doc.slice(0, pos)));
      if (symbol === null) return false;
      const byteMap = byteOffsetMap(doc);
      const def = ide.symbols[symbol];
      const [from, to] = [byteMap[def.start] ?? 0, byteMap[def.end] ?? 0];
      view.dispatch({
        selection: { anchor: from, head: to },
        effects: EditorView.scrollIntoView(from, { y: "center" }),
      });
      event.preventDefault();
      return true;
    },
  });

  const rename = (view: EditorView): boolean => {
    const ide = freshIde(view.state, ideRef.current);
    if (!ide) return false;
    const doc = view.state.doc.toString();
    const cursor = view.state.selection.main.head;
    const symbol = ideSymbolAt(ide, utf8ByteLength(doc.slice(0, cursor)));
    if (symbol === null) return false;
    const current = ide.symbols[symbol];
    const next = window.prompt(`Rename ${current.kind} \`${current.name}\` to:`, current.name);
    if (!next || next === current.name) return true;
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(next)) {
      window.alert(`\`${next}\` is not a valid vix identifier`);
      return true;
    }
    const byteMap = byteOffsetMap(doc);
    view.dispatch({
      changes: ideOccurrences(ide, symbol).map((occ) => ({
        from: byteMap[occ.start] ?? 0,
        to: byteMap[occ.end] ?? 0,
        insert: next,
      })),
    });
    return true;
  };

  return [plugin, goToDefinition, keymap.of([{ key: "F2", run: rename }])];
}

function sourceEditForUpdate(update: ViewUpdate): SourceEdit | null {
  let edit: SourceEdit | null = null;
  let changeCount = 0;
  const oldInput = update.startState.doc.toString();
  const newInput = update.state.doc.toString();
  update.changes.iterChanges((fromA, toA, _fromB, toB) => {
    changeCount += 1;
    if (changeCount > 1) {
      edit = null;
      return;
    }
    edit = {
      start_byte: utf8ByteLength(oldInput.slice(0, fromA)),
      old_end_byte: utf8ByteLength(oldInput.slice(0, toA)),
      new_end_byte: utf8ByteLength(newInput.slice(0, toB)),
    };
  });
  return changeCount === 1 ? edit : null;
}

function utf8ByteLength(input: string) {
  return new TextEncoder().encode(input).length;
}

const snarkTheme = EditorView.theme({
  "&": {
    height: "100%",
    color: "var(--text)",
    backgroundColor: "var(--code-bg)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": {
    fontFamily: "var(--mono)",
    fontSize: "var(--code-size)",
    lineHeight: "var(--line-h)",
    overflow: "auto",
  },
  ".cm-content": {
    padding: "var(--pad-y) 0",
    caretColor: "var(--accent)",
  },
  ".cm-line": { padding: "0 var(--pad-x)" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
    backgroundColor: "var(--accent-soft)",
  },
  ".cm-content ::selection": { backgroundColor: "var(--accent-soft)" },
  ".cm-gutters": {
    backgroundColor: "var(--code-bg)",
    color: "var(--text-faint)",
    border: "none",
    borderRight: "1px solid var(--line-soft)",
  },
  ".cm-lineNumbers .cm-gutterElement": {
    padding: "0 10px 0 16px",
    fontVariantNumeric: "tabular-nums",
  },
  ".cm-gutterElement.cm-gutter-error": {
    color: "var(--err)",
    fontWeight: "700",
  },
});

export function SourceEditor({ input, captures, diagnostic, ide, jump, onCursorByte, onChange }: SourceEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const applyingExternalInputRef = useRef(false);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onCursorByteRef = useRef(onCursorByte);
  onCursorByteRef.current = onCursorByte;
  const ideRef = useRef<IdeState>(ide);
  ideRef.current = ide;

  // Create the EditorView once; React never re-renders its content.
  useEffect(() => {
    const view = new EditorView({
      doc: input,
      parent: hostRef.current!,
      extensions: [
        lineNumbers(),
        history(),
        drawSelection(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        decorationField,
        errorGutterClass,
        snarkTheme,
        ...ideExtensions(ideRef),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !applyingExternalInputRef.current) {
            onChangeRef.current(update.state.doc.toString(), sourceEditForUpdate(update));
          }
          if (update.docChanged || update.selectionSet) {
            onCursorByteRef.current?.(utf8ByteLength(update.state.doc.sliceString(0, update.state.selection.main.head)));
          }
        }),
      ],
    });
    viewRef.current = view;
    view.dispatch({ effects: setDecorations.of(buildDecorations(view.state, captures, diagnostic)) });
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Reconcile external document changes (sample switch, "Use input", upload).
  useEffect(() => {
    const view = viewRef.current;
    if (!view) {
      return;
    }
    const current = view.state.doc.toString();
    if (current !== input) {
      applyingExternalInputRef.current = true;
      try {
        view.dispatch({ changes: { from: 0, to: current.length, insert: input } });
      } finally {
        applyingExternalInputRef.current = false;
      }
    }
  }, [input]);

  // Rebuild decorations whenever Snark's highlights or diagnostic change.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) {
      return;
    }
    view.dispatch({ effects: setDecorations.of(buildDecorations(view.state, captures, diagnostic)) });
  }, [captures, diagnostic]);

  // Poke the IDE plugin when fresh bindings arrive (the ref already holds them).
  useEffect(() => {
    viewRef.current?.dispatch({ effects: ideRefresh.of(null) });
  }, [ide]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view || !jump) {
      return;
    }
    const doc = view.state.doc.toString();
    const byteMap = byteOffsetMap(doc);
    const from = Math.min(byteMap[jump.start_byte] ?? doc.length, doc.length);
    const to = Math.min(byteMap[jump.end_byte] ?? from, doc.length);
    view.dispatch({
      selection: { anchor: from, head: Math.max(from, to) },
      effects: EditorView.scrollIntoView(from, { y: "center" }),
    });
    view.focus();
  }, [jump]);

  return <div className="editor" ref={hostRef} />;
}
