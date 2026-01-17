import { EditorView, basicSetup } from 'codemirror';
import { EditorState, Compartment } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import { json } from '@codemirror/lang-json';
import { vim } from '@replit/codemirror-vim';
import { styx } from '@bearcove/codemirror-lang-styx';

// Export everything the playground needs
export { EditorView, EditorState, Compartment, basicSetup, oneDark, json, vim, styx };
