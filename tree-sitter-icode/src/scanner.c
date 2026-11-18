// External scanner: markdown-style block markers, only at the start of a line.
//   heading      "## text"  (whole line, 1-6 #)
//   list_marker  "-" "*" "+" or "1." followed by a space (marker only)
#include "tree_sitter/parser.h"
#include <stdbool.h>
#include <ctype.h>

enum TokenType { HEADING, LIST_MARKER };

void *tree_sitter_icode_external_scanner_create(void) { return NULL; }
void tree_sitter_icode_external_scanner_destroy(void *p) { (void)p; }
unsigned tree_sitter_icode_external_scanner_serialize(void *p, char *b) { (void)p; (void)b; return 0; }
void tree_sitter_icode_external_scanner_deserialize(void *p, const char *b, unsigned n) { (void)p; (void)b; (void)n; }

bool tree_sitter_icode_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid) {
  (void)payload;
  if (!valid[HEADING] && !valid[LIST_MARKER]) return false;

  // Only at the start of a line: whitespace before us must contain a newline.
  bool newline = false;
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
         lexer->lookahead == '\r' || lexer->lookahead == '\n') {
    if (lexer->lookahead == '\n') newline = true;
    lexer->advance(lexer, true);
  }
  if (!newline) return false;

  int32_t c = lexer->lookahead;

  if (c == '#' && valid[HEADING]) {
    int n = 0;
    while (lexer->lookahead == '#' && n < 7) { lexer->advance(lexer, false); n++; }
    if (n > 6 || (lexer->lookahead != ' ' && lexer->lookahead != '\t')) return false;
    while (lexer->lookahead != '\n' && lexer->lookahead != '\r' && !lexer->eof(lexer))
      lexer->advance(lexer, false);
    lexer->mark_end(lexer);
    lexer->result_symbol = HEADING;
    return true;
  }

  if (valid[LIST_MARKER]) {
    if (c == '-' || c == '*' || c == '+') {
      lexer->advance(lexer, false);
    } else if (isdigit((int)c)) {
      while (isdigit((int)lexer->lookahead)) lexer->advance(lexer, false);
      if (lexer->lookahead != '.') return false;
      lexer->advance(lexer, false);
    } else {
      return false;
    }
    lexer->mark_end(lexer);
    if (lexer->lookahead != ' ' && lexer->lookahead != '\t') return false;
    lexer->result_symbol = LIST_MARKER;
    return true;
  }
  return false;
}
