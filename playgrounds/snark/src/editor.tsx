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

export interface SourceEditorProps {
  input: string;
  captures: CaptureRange[];
  diagnostic: EditorDiagnostic | null;
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

export function SourceEditor({ input, captures, diagnostic, onChange }: SourceEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const applyingExternalInputRef = useRef(false);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

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
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !applyingExternalInputRef.current) {
            onChangeRef.current(update.state.doc.toString(), sourceEditForUpdate(update));
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

  return <div className="editor" ref={hostRef} />;
}
