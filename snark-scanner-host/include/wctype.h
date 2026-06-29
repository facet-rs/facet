#ifndef SNARK_SCANNER_HOST_WCTYPE_H_
#define SNARK_SCANNER_HOST_WCTYPE_H_

/*
 * Hand-written minimal wctype shim for fixture external scanners when the
 * target has no C library headers, such as wasm32-unknown-unknown.
 */

static inline int iswspace(int ch) {
  return ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' || ch == '\f' ||
         ch == '\v';
}

static inline int iswalnum(int ch) {
  return (ch >= '0' && ch <= '9') || (ch >= 'A' && ch <= 'Z') ||
         (ch >= 'a' && ch <= 'z');
}

#endif
