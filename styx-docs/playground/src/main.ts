import { EditorView, basicSetup } from 'codemirror';
import { EditorState, Compartment, Prec } from '@codemirror/state';
import { keymap } from '@codemirror/view';
import { oneDark } from '@codemirror/theme-one-dark';
import { json } from '@codemirror/lang-json';
import { sql } from '@codemirror/lang-sql';
import { vim } from '@replit/codemirror-vim';
import { styx } from '@bearcove/codemirror-lang-styx';

// Export everything the playground needs
export { EditorView, EditorState, Compartment, Prec, basicSetup, oneDark, json, sql, vim, styx, keymap };
