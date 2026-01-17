import { ExternalTokenizer } from "@lezer/lr";
import { heredoc as Heredoc } from "./syntax.grammar.terms";

// Helper: check if char is valid delimiter start [A-Z]
function isDelimiterStart(ch: number): boolean {
  return ch >= 65 && ch <= 90; // A-Z
}

// Helper: check if char is valid delimiter char [A-Z0-9_]
function isDelimiterChar(ch: number): boolean {
  return (ch >= 65 && ch <= 90) || (ch >= 48 && ch <= 57) || ch === 95;
}

// Helper: check if char is valid lang hint char [a-z0-9_.-]
function isLangHintChar(ch: number): boolean {
  return (ch >= 97 && ch <= 122) || (ch >= 48 && ch <= 57) ||
         ch === 95 || ch === 46 || ch === 45;
}

/**
 * External tokenizer that matches an entire heredoc as a single token.
 *
 * Format: <<DELIM[,lang]\n...content...\nDELIM
 *
 * The token includes:
 * - The opening marker (<<DELIM or <<DELIM,lang)
 * - The newline after the marker
 * - All content lines
 * - The closing delimiter
 */
export const heredocTokenizer = new ExternalTokenizer(
  (input, stack) => {
    // Must start with <<
    if (input.next !== 60 /* < */) return;
    input.advance();
    if (input.next !== 60 /* < */) return;
    input.advance();

    // Must have delimiter starting with [A-Z]
    if (!isDelimiterStart(input.next)) return;

    // Read delimiter name
    let delimiter = "";
    while (isDelimiterChar(input.next)) {
      delimiter += String.fromCharCode(input.next);
      input.advance();
    }

    // Optional lang hint after comma
    if (input.next === 44 /* , */) {
      input.advance();
      // Consume lang hint (a-z start, then a-z0-9_.- continuation)
      if (input.next >= 97 && input.next <= 122) {
        while (isLangHintChar(input.next)) {
          input.advance();
        }
      }
    }

    // Must be followed by newline
    if (input.next !== 10 && input.next !== 13) return;

    // Consume newline
    if (input.next === 13) input.advance(); // \r
    if (input.next === 10) input.advance(); // \n

    // Now scan content lines until we find the delimiter at start of line
    while (input.next !== -1) {
      // At start of line - check for delimiter
      // Skip optional leading whitespace (for indented heredocs)
      while (input.next === 32 || input.next === 9) {
        input.advance();
      }

      // Check if this line starts with the delimiter
      let matchPos = 0;
      let isMatch = true;

      // We need to peek ahead without consuming if it's not a match
      // Unfortunately ExternalTokenizer doesn't have peek, so we'll
      // consume and track position
      const lineStart = input.pos;

      for (let i = 0; i < delimiter.length && input.next !== -1; i++) {
        if (input.next !== delimiter.charCodeAt(i)) {
          isMatch = false;
          break;
        }
        input.advance();
        matchPos++;
      }

      if (isMatch && matchPos === delimiter.length) {
        // Check that delimiter is followed by newline or EOF
        if (input.next === 10 || input.next === 13 || input.next === -1) {
          // Found the end! Accept the token
          input.acceptToken(Heredoc);
          return;
        }
      }

      // Not a match - consume rest of line
      while (input.next !== 10 && input.next !== 13 && input.next !== -1) {
        input.advance();
      }

      // Consume newline
      if (input.next === 13) input.advance();
      if (input.next === 10) input.advance();
    }

    // EOF without finding end delimiter - still accept as (unterminated) heredoc
    input.acceptToken(Heredoc);
  },
  { contextual: false }
);
