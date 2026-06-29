#ifndef SNARK_SCANNER_HOST_TREE_SITTER_PARSER_H_
#define SNARK_SCANNER_HOST_TREE_SITTER_PARSER_H_

/*
 * Hand-written test ABI shim for compiling fixture external scanners.
 *
 * This is not generated parser material and is not copied from a generated
 * grammar package. It declares only the Tree-sitter scanner host surface that
 * the reduced CSS fixture's src/scanner.c needs.
 */

#ifdef __cplusplus
extern "C" {
#endif

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define TREE_SITTER_SERIALIZATION_BUFFER_SIZE 1024

typedef uint16_t TSSymbol;
typedef struct TSLexer TSLexer;

struct TSLexer {
  int32_t lookahead;
  TSSymbol result_symbol;
  void (*advance)(TSLexer *, bool);
  void (*mark_end)(TSLexer *);
  uint32_t (*get_column)(TSLexer *);
  bool (*is_at_included_range_start)(const TSLexer *);
  bool (*eof)(const TSLexer *);
  void (*log)(const TSLexer *, const char *, ...);
};

#ifdef __cplusplus
}
#endif

#endif
