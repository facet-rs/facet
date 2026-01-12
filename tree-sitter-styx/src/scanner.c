/**
 * External scanner for tree-sitter-styx.
 *
 * Handles context-sensitive constructs:
 * - Heredocs: <<DELIM ... DELIM
 * - Raw strings: r#"..."# with matching # counts
 * - Unit vs Tag: @ alone vs @tagname
 */

#include "tree_sitter/parser.h"
#include <string.h>
#include <wctype.h>

enum TokenType {
  HEREDOC_START,
  HEREDOC_CONTENT,
  HEREDOC_END,
  RAW_STRING_START,
  RAW_STRING_CONTENT,
  RAW_STRING_END,
  UNIT_AT,
  TAG_START,
};

// Maximum delimiter length for heredocs
#define MAX_DELIMITER_LEN 16
// Maximum # count for raw strings
#define MAX_HASH_COUNT 255

typedef struct {
  // For heredocs: the delimiter string
  char heredoc_delimiter[MAX_DELIMITER_LEN + 1];
  uint8_t heredoc_delimiter_len;
  bool in_heredoc;

  // For raw strings: count of # marks
  uint8_t raw_string_hash_count;
  bool in_raw_string;
} Scanner;

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

// Check if character is valid for heredoc delimiter start: [A-Z]
static inline bool is_delimiter_start(int32_t c) {
  return c >= 'A' && c <= 'Z';
}

// Check if character is valid for heredoc delimiter continuation: [A-Z0-9_]
static inline bool is_delimiter_char(int32_t c) {
  return (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_';
}

// Check if character is valid for tag name start: [A-Za-z_]
static inline bool is_tag_name_start(int32_t c) {
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || c == '_';
}

// Check if character is valid for tag name continuation: [A-Za-z0-9_.-]
static inline bool is_tag_name_char(int32_t c) {
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
         (c >= '0' && c <= '9') || c == '_' || c == '.' || c == '-';
}

// Serialize scanner state
unsigned tree_sitter_styx_external_scanner_serialize(void *payload,
                                                      char *buffer) {
  Scanner *scanner = (Scanner *)payload;
  unsigned i = 0;

  buffer[i++] = scanner->in_heredoc;
  buffer[i++] = scanner->heredoc_delimiter_len;
  for (uint8_t j = 0; j < scanner->heredoc_delimiter_len; j++) {
    buffer[i++] = scanner->heredoc_delimiter[j];
  }
  buffer[i++] = scanner->in_raw_string;
  buffer[i++] = scanner->raw_string_hash_count;

  return i;
}

// Deserialize scanner state
void tree_sitter_styx_external_scanner_deserialize(void *payload,
                                                    const char *buffer,
                                                    unsigned length) {
  Scanner *scanner = (Scanner *)payload;
  scanner->in_heredoc = false;
  scanner->heredoc_delimiter_len = 0;
  scanner->heredoc_delimiter[0] = '\0';
  scanner->in_raw_string = false;
  scanner->raw_string_hash_count = 0;

  if (length > 0) {
    unsigned i = 0;
    scanner->in_heredoc = buffer[i++];
    if (i < length) {
      scanner->heredoc_delimiter_len = buffer[i++];
      for (uint8_t j = 0; j < scanner->heredoc_delimiter_len && i < length;
           j++) {
        scanner->heredoc_delimiter[j] = buffer[i++];
      }
      scanner->heredoc_delimiter[scanner->heredoc_delimiter_len] = '\0';
    }
    if (i < length) {
      scanner->in_raw_string = buffer[i++];
    }
    if (i < length) {
      scanner->raw_string_hash_count = buffer[i++];
    }
  }
}

// Create scanner
void *tree_sitter_styx_external_scanner_create() {
  Scanner *scanner = calloc(1, sizeof(Scanner));
  return scanner;
}

// Destroy scanner
void tree_sitter_styx_external_scanner_destroy(void *payload) {
  free(payload);
}

// Try to scan heredoc start: <<DELIM
static bool scan_heredoc_start(Scanner *scanner, TSLexer *lexer) {
  // We expect to be positioned at '<'
  if (lexer->lookahead != '<')
    return false;
  advance(lexer);

  if (lexer->lookahead != '<')
    return false;
  advance(lexer);

  // Now we need a delimiter starting with [A-Z]
  if (!is_delimiter_start(lexer->lookahead))
    return false;

  scanner->heredoc_delimiter_len = 0;
  while (is_delimiter_char(lexer->lookahead) &&
         scanner->heredoc_delimiter_len < MAX_DELIMITER_LEN) {
    scanner->heredoc_delimiter[scanner->heredoc_delimiter_len++] =
        (char)lexer->lookahead;
    advance(lexer);
  }
  scanner->heredoc_delimiter[scanner->heredoc_delimiter_len] = '\0';

  // Must be followed by newline
  if (lexer->lookahead != '\n' && lexer->lookahead != '\r')
    return false;

  // Consume the newline
  if (lexer->lookahead == '\r')
    advance(lexer);
  if (lexer->lookahead == '\n')
    advance(lexer);

  scanner->in_heredoc = true;
  lexer->result_symbol = HEREDOC_START;
  return true;
}

// Try to scan heredoc content and end
static bool scan_heredoc_content_or_end(Scanner *scanner, TSLexer *lexer) {
  if (!scanner->in_heredoc)
    return false;

  bool has_content = false;
  lexer->result_symbol = HEREDOC_CONTENT;

  while (true) {
    // Check if current line matches delimiter
    // First, skip any leading whitespace (for indented heredocs)
    lexer->mark_end(lexer);

    // Check if we're at the delimiter
    bool at_delimiter = true;
    for (uint8_t i = 0; i < scanner->heredoc_delimiter_len; i++) {
      if (lexer->lookahead != scanner->heredoc_delimiter[i]) {
        at_delimiter = false;
        break;
      }
      advance(lexer);
    }

    if (at_delimiter) {
      // Check that delimiter is followed by newline or EOF
      if (lexer->lookahead == '\n' || lexer->lookahead == '\r' ||
          lexer->eof(lexer)) {
        // This is the end delimiter
        if (has_content) {
          // Return the content first, we'll get the end on next call
          return true;
        } else {
          // No content, return the end delimiter
          scanner->in_heredoc = false;
          lexer->result_symbol = HEREDOC_END;
          lexer->mark_end(lexer);
          return true;
        }
      }
    }

    // Not at delimiter, consume this line as content
    has_content = true;
    while (lexer->lookahead != '\n' && lexer->lookahead != '\r' &&
           !lexer->eof(lexer)) {
      advance(lexer);
    }

    // Consume newline
    if (lexer->lookahead == '\r')
      advance(lexer);
    if (lexer->lookahead == '\n')
      advance(lexer);

    if (lexer->eof(lexer)) {
      // Unterminated heredoc - return what we have
      lexer->mark_end(lexer);
      return has_content;
    }

    lexer->mark_end(lexer);
  }
}

// Try to scan raw string start: r#*"
static bool scan_raw_string_start(Scanner *scanner, TSLexer *lexer) {
  if (lexer->lookahead != 'r')
    return false;
  advance(lexer);

  // Count # marks
  scanner->raw_string_hash_count = 0;
  while (lexer->lookahead == '#' &&
         scanner->raw_string_hash_count < MAX_HASH_COUNT) {
    scanner->raw_string_hash_count++;
    advance(lexer);
  }

  // Must have opening "
  if (lexer->lookahead != '"')
    return false;
  advance(lexer);

  scanner->in_raw_string = true;
  lexer->result_symbol = RAW_STRING_START;
  return true;
}

// Try to scan raw string content and end
static bool scan_raw_string_content_or_end(Scanner *scanner, TSLexer *lexer) {
  if (!scanner->in_raw_string)
    return false;

  bool has_content = false;
  lexer->result_symbol = RAW_STRING_CONTENT;

  while (true) {
    if (lexer->eof(lexer)) {
      // Unterminated raw string
      if (has_content) {
        lexer->mark_end(lexer);
        return true;
      }
      return false;
    }

    if (lexer->lookahead == '"') {
      // Potential end - check for matching # count
      lexer->mark_end(lexer);
      advance(lexer);

      uint8_t hash_count = 0;
      while (lexer->lookahead == '#' &&
             hash_count < scanner->raw_string_hash_count) {
        hash_count++;
        advance(lexer);
      }

      if (hash_count == scanner->raw_string_hash_count) {
        // This is the end
        if (has_content) {
          // Return content first
          return true;
        } else {
          // No content, return end
          scanner->in_raw_string = false;
          lexer->result_symbol = RAW_STRING_END;
          lexer->mark_end(lexer);
          return true;
        }
      }

      // Not the end, the " and any # are part of content
      has_content = true;
      // Continue from current position (after the #s we consumed)
    } else {
      has_content = true;
      advance(lexer);
    }
  }
}

// Try to scan @ (unit) or @tagname (tag start)
static bool scan_at_token(TSLexer *lexer, const bool *valid_symbols) {
  if (lexer->lookahead != '@')
    return false;

  advance(lexer);

  // Check what follows the @
  if (is_tag_name_start(lexer->lookahead)) {
    // This is a tag - consume the tag name
    if (!valid_symbols[TAG_START])
      return false;

    while (is_tag_name_char(lexer->lookahead)) {
      advance(lexer);
    }

    lexer->result_symbol = TAG_START;
    return true;
  } else {
    // This is a unit (bare @)
    if (!valid_symbols[UNIT_AT])
      return false;

    lexer->result_symbol = UNIT_AT;
    return true;
  }
}

// Main scan function
bool tree_sitter_styx_external_scanner_scan(void *payload, TSLexer *lexer,
                                             const bool *valid_symbols) {
  Scanner *scanner = (Scanner *)payload;

  // Skip whitespace if not in heredoc/raw string
  if (!scanner->in_heredoc && !scanner->in_raw_string) {
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
      skip(lexer);
    }
  }

  // Handle heredoc content/end first if we're in one
  if (scanner->in_heredoc) {
    if (valid_symbols[HEREDOC_CONTENT] || valid_symbols[HEREDOC_END]) {
      return scan_heredoc_content_or_end(scanner, lexer);
    }
  }

  // Handle raw string content/end if we're in one
  if (scanner->in_raw_string) {
    if (valid_symbols[RAW_STRING_CONTENT] || valid_symbols[RAW_STRING_END]) {
      return scan_raw_string_content_or_end(scanner, lexer);
    }
  }

  // Try to scan @ tokens (unit or tag)
  if ((valid_symbols[UNIT_AT] || valid_symbols[TAG_START]) &&
      lexer->lookahead == '@') {
    return scan_at_token(lexer, valid_symbols);
  }

  // Try to start a heredoc
  if (valid_symbols[HEREDOC_START] && lexer->lookahead == '<') {
    return scan_heredoc_start(scanner, lexer);
  }

  // Try to start a raw string
  if (valid_symbols[RAW_STRING_START] && lexer->lookahead == 'r') {
    return scan_raw_string_start(scanner, lexer);
  }

  return false;
}
