; file: compliance/corpus/00-basic/empty.styx
(document [-1, -1]
)
; file: compliance/corpus/00-basic/multiple-entries.styx
(document [-1, -1]
  (entry
    (scalar [0, 4] bare "name")
    (scalar [5, 10] bare "hello"))
  (entry
    (scalar [11, 15] bare "port")
    (scalar [16, 20] bare "8080"))
  (entry
    (scalar [21, 28] bare "enabled")
    (scalar [29, 33] bare "true"))
)
; file: compliance/corpus/00-basic/single-entry.styx
(document [-1, -1]
  (entry
    (scalar [0, 4] bare "name")
    (scalar [5, 10] bare "hello"))
)
; file: compliance/corpus/00-basic/unit-key.styx
(document [-1, -1]
  (entry
    (unit [0, 1])
    (scalar [2, 13] bare "schema.styx"))
  (entry
    (scalar [15, 19] bare "name")
    (scalar [20, 25] bare "hello"))
)
; file: compliance/corpus/00-basic/unit-value.styx
(document [-1, -1]
  (entry
    (scalar [0, 7] bare "nothing")
    (unit [8, 9]))
)
; file: compliance/corpus/01-scalars/bare-simple.styx
(document [-1, -1]
  (entry
    (scalar [0, 4] bare "name")
    (scalar [5, 10] bare "hello"))
  (entry
    (scalar [11, 15] bare "port")
    (scalar [16, 20] bare "8080"))
  (entry
    (scalar [21, 25] bare "path")
    (scalar [26, 37] bare "/etc/config"))
  (entry
    (scalar [38, 41] bare "url")
    (scalar [42, 61] bare "https://example.com"))
)
; file: compliance/corpus/01-scalars/bare-special-chars.styx
(document [-1, -1]
  (entry
    (scalar [0, 4] bare "dash")
    (scalar [5, 12] bare "foo-bar"))
  (entry
    (scalar [13, 23] bare "underscore")
    (scalar [24, 31] bare "foo_bar"))
  (entry
    (scalar [32, 35] bare "dot")
    (scalar [36, 43] bare "foo.bar"))
  (entry
    (scalar [44, 49] bare "colon")
    (scalar [50, 57] bare "foo:bar"))
  (entry
    (scalar [58, 63] bare "slash")
    (scalar [64, 71] bare "foo/bar"))
  (entry
    (scalar [72, 76] bare "plus")
    (scalar [77, 80] bare "1+2"))
  (entry
    (scalar [81, 86] bare "minus")
    (scalar [87, 90] bare "1-2"))
)
; file: compliance/corpus/01-scalars/heredoc-empty.styx
(document [-1, -1]
  (entry
    (scalar [0, 5] bare "empty")
    (scalar [6, 15] heredoc ""))
)
; file: compliance/corpus/01-scalars/heredoc-lang-hint.styx
(document [-1, -1]
  (entry
    (scalar [0, 4] bare "code")
    (scalar [5, 59] heredoc ",rust\nfn main() {\n    println!(\"Hello!\");\n}\n"))
)
; file: compliance/corpus/01-scalars/heredoc-simple.styx
(document [-1, -1]
  (entry
    (scalar [0, 4] bare "text")
    (scalar [5, 57] heredoc "Hello, world!\nThis is a multi-line string.\n"))
)
; file: compliance/corpus/01-scalars/quoted-escapes.styx
(document [-1, -1]
  (entry
    (scalar [0, 7] bare "newline")
    (scalar [8, 22] quoted "line1\nline2"))
  (entry
    (scalar [23, 26] bare "tab")
    (scalar [27, 39] quoted "col1\tcol2"))
  (entry
    (scalar [40, 48] bare "carriage")
    (scalar [49, 65] quoted "line1\r\nline2"))
  (entry
    (scalar [66, 71] bare "quote")
    (scalar [72, 87] quoted "say \"hello\""))
  (entry
    (scalar [88, 97] bare "backslash")
    (scalar [98, 114] quoted "path\\to\\file"))
  (entry
    (scalar [115, 119] bare "null")
    (scalar [120, 132] quoted "null\u0000char"))
)
; file: compliance/corpus/01-scalars/quoted-simple.styx
(document [-1, -1]
  (entry
    (scalar [0, 8] bare "greeting")
    (scalar [9, 22] quoted "hello world"))
  (entry
    (scalar [23, 28] bare "empty")
    (scalar [29, 31] quoted ""))
  (entry
    (scalar [32, 38] bare "spaces")
    (scalar [39, 51] quoted "  spaces  "))
)
; file: compliance/corpus/01-scalars/quoted-unicode.styx
(document [-1, -1]
  (entry
    (scalar [0, 5] bare "emoji")
    (scalar [6, 23] quoted "Hello 😀"))
  (entry
    (scalar [24, 29] bare "latin")
    (scalar [30, 50] quoted "ABC"))
  (entry
    (scalar [51, 56] bare "short")
    (scalar [57, 65] quoted "A"))
)
; file: compliance/corpus/01-scalars/raw-hashes.styx
(document [-1, -1]
  (entry
    (scalar [0, 3] bare "one")
    (scalar [4, 21] raw "has \"quotes\""))
  (entry
    (scalar [22, 25] bare "two")
    (scalar [26, 45] raw "has \"# in it"))
  (entry
    (scalar [46, 51] bare "three")
    (scalar [52, 74] raw "has \"## in it"))
)
; file: compliance/corpus/01-scalars/raw-simple.styx
(document [-1, -1]
  (entry
    (scalar [0, 4] bare "path")
    (scalar [5, 21] raw "C:\\Users\\name"))
  (entry
    (scalar [22, 27] bare "regex")
    (scalar [28, 36] raw "^\\d+$"))
)
; file: compliance/corpus/02-objects/comma-sep.styx
(document [-1, -1]
  (entry
    (scalar [0, 6] bare "server")
    (object [7, 34] comma
      (entry
        (scalar [8, 12] bare "host")
        (scalar [13, 22] bare "localhost"))
      (entry
        (scalar [24, 28] bare "port")
        (scalar [29, 33] bare "8080"))
    ))
)
; file: compliance/corpus/02-objects/empty.styx
(document [-1, -1]
  (entry
    (scalar [0, 3] bare "obj")
    (object [4, 6] comma))
)
; file: compliance/corpus/02-objects/nested.styx
(document [-1, -1]
  (entry
    (scalar [0, 6] bare "config")
    (object [7, 136] newline
      (entry
        (scalar [13, 19] bare "server")
        (object [20, 134] newline
          (entry
            (scalar [30, 34] bare "host")
            (scalar [35, 44] bare "localhost"))
          (entry
            (scalar [53, 56] bare "tls")
            (object [57, 128] newline
              (entry
                (scalar [71, 78] bare "enabled")
                (scalar [79, 83] bare "true"))
              (entry
                (scalar [96, 100] bare "cert")
                (scalar [101, 118] bare "/etc/ssl/cert.pem"))
            ))
        ))
    ))
)
; file: compliance/corpus/02-objects/newline-sep.styx
(document [-1, -1]
  (entry
    (scalar [0, 6] bare "server")
    (object [7, 43] newline
      (entry
        (scalar [13, 17] bare "host")
        (scalar [18, 27] bare "localhost"))
      (entry
        (scalar [32, 36] bare "port")
        (scalar [37, 41] bare "8080"))
    ))
)
; file: compliance/corpus/02-objects/unit-key-in-object.styx
(document [-1, -1]
  (entry
    (scalar [0, 6] bare "schema")
    (object [7, 64] newline
      (entry
        (unit [13, 14])
        (tag [22, 36] "object"
          (object [22, 36] comma
            (entry
              (scalar [23, 27] bare "name")
              (tag [28, 35] "string"))
          )))
      (entry
        (scalar [41, 45] bare "User")
        (tag [53, 62] "object"
          (object [53, 62] comma
            (entry
              (scalar [54, 56] bare "id")
              (tag [57, 61] "int"))
          )))
    ))
)
; file: compliance/corpus/03-sequences/empty.styx
(document [-1, -1]
  (entry
    (scalar [0, 5] bare "items")
    (sequence [6, 8]))
)
; file: compliance/corpus/03-sequences/multiline.styx
(document [-1, -1]
  (entry
    (scalar [0, 5] bare "hosts")
    (sequence [6, 50]
      (scalar [7, 16] bare "localhost")
      (scalar [17, 30] quoted "192.168.1.1")
      (scalar [31, 49] bare "server.example.com")))
)
; file: compliance/corpus/03-sequences/nested.styx
(document [-1, -1]
  (entry
    (scalar [0, 6] bare "matrix")
    (sequence [7, 32]
      (sequence [8, 15]
        (scalar [9, 10] bare "1")
        (scalar [11, 12] bare "2")
        (scalar [13, 14] bare "3"))
      (sequence [16, 23]
        (scalar [17, 18] bare "4")
        (scalar [19, 20] bare "5")
        (scalar [21, 22] bare "6"))
      (sequence [24, 31]
        (scalar [25, 26] bare "7")
        (scalar [27, 28] bare "8")
        (scalar [29, 30] bare "9"))))
)
; file: compliance/corpus/03-sequences/scalars.styx
(document [-1, -1]
  (entry
    (scalar [0, 7] bare "numbers")
    (sequence [8, 19]
      (scalar [9, 10] bare "1")
      (scalar [11, 12] bare "2")
      (scalar [13, 14] bare "3")
      (scalar [15, 16] bare "4")
      (scalar [17, 18] bare "5")))
  (entry
    (scalar [20, 27] bare "strings")
    (sequence [28, 41]
      (scalar [29, 32] bare "foo")
      (scalar [33, 36] bare "bar")
      (scalar [37, 40] bare "baz")))
  (entry
    (scalar [42, 48] bare "quoted")
    (sequence [49, 74]
      (scalar [50, 63] quoted "hello world")
      (scalar [64, 73] quoted "foo bar")))
)
; file: compliance/corpus/03-sequences/with-objects.styx
(document [-1, -1]
  (entry
    (scalar [0, 6] bare "routes")
    (sequence [7, 29]
      (object [8, 16] comma
        (entry
          (scalar [9, 13] bare "path")
          (scalar [14, 15] bare "/"))
      )
      (object [17, 28] comma
        (entry
          (scalar [18, 22] bare "path")
          (scalar [23, 27] bare "/api"))
      )))
)
; file: compliance/corpus/04-tags/explicit-unit.styx
(document [-1, -1]
  (entry
    (scalar [0, 8] bare "explicit")
    (tag [9, 14] "none"))
)
; file: compliance/corpus/04-tags/in-sequence.styx
(document [-1, -1]
  (entry
    (scalar [0, 6] bare "routes")
    (sequence [7, 62]
      (tag [14, 22] "route"
        (object [14, 22] comma
          (entry
            (scalar [15, 19] bare "path")
            (scalar [20, 21] bare "/"))
        ))
      (tag [29, 40] "route"
        (object [29, 40] comma
          (entry
            (scalar [30, 34] bare "path")
            (scalar [35, 39] bare "/api"))
        ))
      (tag [47, 61] "route"
        (object [47, 61] comma
          (entry
            (scalar [48, 52] bare "path")
            (scalar [53, 60] bare "/health"))
        ))))
)
; file: compliance/corpus/04-tags/nested-tags.styx
(document [-1, -1]
  (entry
    (scalar [0, 6] bare "result")
    (tag [10, 26] "ok"
      (object [10, 26] comma
        (entry
          (scalar [11, 15] bare "data")
          (tag [21, 25] "some"
            (sequence [21, 25]
              (scalar [22, 24] bare "42"))))
      )))
  (entry
    (scalar [27, 32] bare "field")
    (tag [42, 60] "optional"
      (sequence [42, 60]
        (tag [51, 59] "default"
          (sequence [51, 59]
            (scalar [52, 53] bare "0")
            (tag [54, 58] "int"))))))
)
; file: compliance/corpus/04-tags/object-payload.styx
(document [-1, -1]
  (entry
    (scalar [0, 5] bare "error")
    (tag [12, 38] "error"
      (object [12, 38] comma
        (entry
          (scalar [13, 17] bare "code")
          (scalar [18, 21] bare "500"))
        (entry
          (scalar [23, 30] bare "message")
          (scalar [31, 37] quoted "fail"))
      )))
  (entry
    (scalar [39, 43] bare "user")
    (tag [49, 69] "user"
      (object [49, 69] comma
        (entry
          (scalar [50, 54] bare "name")
          (scalar [55, 60] bare "alice"))
        (entry
          (scalar [62, 65] bare "age")
          (scalar [66, 68] bare "30"))
      )))
)
; file: compliance/corpus/04-tags/sequence-payload.styx
(document [-1, -1]
  (entry
    (scalar [0, 5] bare "point")
    (tag [10, 21] "rgb"
      (sequence [10, 21]
        (scalar [11, 14] bare "255")
        (scalar [15, 18] bare "128")
        (scalar [19, 20] bare "0"))))
  (entry
    (scalar [22, 27] bare "maybe")
    (tag [33, 37] "some"
      (sequence [33, 37]
        (scalar [34, 36] bare "42"))))
  (entry
    (scalar [38, 43] bare "union")
    (tag [50, 64] "union"
      (sequence [50, 64]
        (tag [51, 55] "int")
        (tag [56, 63] "string"))))
)
; file: compliance/corpus/04-tags/string-payload.styx
(document [-1, -1]
  (entry
    (scalar [0, 7] bare "warning")
    (tag [13, 33] "warn"
      (scalar [13, 33] quoted "deprecated feature")))
  (entry
    (scalar [34, 37] bare "env")
    (tag [42, 48] "env"
      (scalar [42, 48] quoted "HOME")))
)
; file: compliance/corpus/04-tags/unit-payload.styx
(document [-1, -1]
  (entry
    (scalar [0, 7] bare "enabled")
    (tag [8, 13] "true"))
  (entry
    (scalar [14, 22] bare "disabled")
    (tag [23, 29] "false"))
  (entry
    (scalar [30, 37] bare "nothing")
    (tag [38, 43] "none"))
)
; file: compliance/corpus/05-comments/doc-comment.styx
(document [-1, -1]
  (entry
    (scalar [20, 24] bare "name")
    (scalar [25, 30] bare "hello"))
  (entry
    (scalar [84, 88] bare "port")
    (scalar [89, 93] bare "8080"))
)
; file: compliance/corpus/05-comments/inline-comment.styx
(document [-1, -1]
  (entry
    (scalar [0, 4] bare "name")
    (scalar [5, 10] bare "hello"))
  (entry
    (scalar [29, 33] bare "port")
    (scalar [34, 38] bare "8080"))
)
; file: compliance/corpus/05-comments/line-comment.styx
(document [-1, -1]
  (entry
    (scalar [21, 25] bare "name")
    (scalar [26, 31] bare "hello"))
  (entry
    (scalar [51, 55] bare "port")
    (scalar [56, 60] bare "8080"))
)
; file: compliance/corpus/06-edge-cases/at-in-strings.styx
(document [-1, -1]
  (entry
    (scalar [0, 5] bare "email")
    (scalar [6, 24] quoted "user@example.com"))
  (entry
    (scalar [25, 32] bare "mention")
    (scalar [33, 44] quoted "@username"))
)
; file: compliance/corpus/06-edge-cases/deeply-nested.styx
(document [-1, -1]
  (entry
    (scalar [0, 1] bare "a")
    (object [2, 31] comma
      (entry
        (scalar [3, 4] bare "b")
        (object [5, 30] comma
          (entry
            (scalar [6, 7] bare "c")
            (object [8, 29] comma
              (entry
                (scalar [9, 10] bare "d")
                (object [11, 28] comma
                  (entry
                    (scalar [12, 13] bare "e")
                    (object [14, 27] comma
                      (entry
                        (scalar [15, 16] bare "f")
                        (object [17, 26] comma
                          (entry
                            (scalar [18, 19] bare "g")
                            (scalar [20, 25] bare "value"))
                        ))
                    ))
                ))
            ))
        ))
    ))
)
; file: compliance/corpus/06-edge-cases/empty-string.styx
(document [-1, -1]
  (entry
    (scalar [0, 5] bare "empty")
    (scalar [6, 8] quoted ""))
  (entry
    (scalar [9, 12] bare "raw")
    (scalar [13, 16] raw ""))
)
; file: compliance/corpus/06-edge-cases/numeric-looking.styx
(document [-1, -1]
  (entry
    (scalar [0, 3] bare "int")
    (scalar [4, 6] bare "42"))
  (entry
    (scalar [7, 15] bare "negative")
    (scalar [16, 19] bare "-17"))
  (entry
    (scalar [20, 25] bare "float")
    (scalar [26, 30] bare "3.14"))
  (entry
    (scalar [31, 41] bare "scientific")
    (scalar [42, 46] bare "1e10"))
  (entry
    (scalar [47, 50] bare "hex")
    (scalar [51, 55] bare "0xff"))
)
; file: compliance/corpus/06-edge-cases/single-char.styx
(document [-1, -1]
  (entry
    (scalar [0, 1] bare "a")
    (scalar [2, 3] bare "b"))
  (entry
    (scalar [4, 5] bare "x")
    (scalar [6, 7] bare "y"))
)
; file: compliance/corpus/06-edge-cases/trailing-comma.styx
(document [-1, -1]
  (entry
    (scalar [0, 3] bare "obj")
    (object [4, 14] comma
      (entry
        (scalar [5, 6] bare "a")
        (scalar [7, 8] bare "1"))
      (entry
        (scalar [10, 11] bare "b")
        (scalar [12, 13] bare "2"))
    ))
)
; file: compliance/corpus/06-edge-cases/unicode-keys.styx
(document [-1, -1]
  (entry
    (scalar [0, 9] bare "日本語")
    (scalar [10, 20] quoted "Japanese"))
  (entry
    (scalar [21, 27] bare "émoji")
    (scalar [28, 36] quoted "French"))
  (entry
    (scalar [37, 43] bare "中文")
    (scalar [44, 53] quoted "Chinese"))
)
; file: compliance/corpus/06-edge-cases/unicode-values.styx
(document [-1, -1]
  (entry
    (scalar [0, 5] bare "emoji")
    (scalar [6, 20] quoted "🎉🚀💯"))
  (entry
    (scalar [21, 29] bare "japanese")
    (scalar [30, 47] quoted "こんにちは"))
  (entry
    (scalar [48, 53] bare "mixed")
    (scalar [54, 68] quoted "Hello 世界"))
)
; file: compliance/corpus/06-edge-cases/whitespace-variations.styx
(document [-1, -1]
  (entry
    (scalar [0, 1] bare "a")
    (scalar [2, 3] bare "b"))
  (entry
    (scalar [4, 5] bare "c")
    (scalar [6, 7] bare "d"))
  (entry
    (scalar [8, 9] bare "e")
    (object [10, 15] comma
      (entry
        (scalar [11, 12] bare "f")
        (scalar [13, 14] bare "g"))
    ))
  (entry
    (scalar [16, 17] bare "h")
    (sequence [18, 23]
      (scalar [19, 20] bare "i")
      (scalar [21, 22] bare "j")))
)
; file: compliance/corpus/07-invalid/invalid-escape.styx
(document [-1, -1]
  (entry
    (scalar [0, 3] bare "bad")
    (scalar [4, 23] quoted "invalid \\x escape"))
)
; file: compliance/corpus/07-invalid/mixed-separators.styx
(error [13, 14] "parse error at 13-14: mixed separators (use either commas or newlines)")
; file: compliance/corpus/07-invalid/unclosed-brace.styx
(error [4, 5] "parse error at 4-5: unclosed object (missing `}`)")
; file: compliance/corpus/07-invalid/unclosed-heredoc.styx
(document [-1, -1]
  (entry
    (scalar [0, 4] bare "text")
    (scalar [5, 11] heredoc "hello world\n"))
)
; file: compliance/corpus/07-invalid/unclosed-paren.styx
(error [4, 5] "parse error at 4-5: unclosed sequence (missing `)`)")
; file: compliance/corpus/07-invalid/unclosed-quote.styx
(document [-1, -1]
  (entry
    (scalar [0, 4] bare "name")
    (scalar [5, 12] quoted "\"hello\n"))
)
