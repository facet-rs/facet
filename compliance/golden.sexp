; file: compliance/corpus/00-basic/attributes.styx
(document [-1, -1]
  (entry
    (scalar [82, 88] bare "server")
    (object [89, 103] comma
      (entry
        (scalar [89, 93] bare "host")
        (scalar [94, 103] bare "localhost"))
    ))
  (entry
    (scalar [128, 134] bare "config")
    (object [135, 159] comma
      (entry
        (scalar [135, 139] bare "host")
        (scalar [140, 149] bare "localhost"))
      (entry
        (scalar [150, 154] bare "port")
        (scalar [155, 159] bare "8080"))
    ))
  (entry
    (scalar [202, 207] bare "mixed")
    (object [208, 260] comma
      (entry
        (scalar [208, 212] bare "name")
        (scalar [213, 227] quoted "quoted value"))
      (entry
        (scalar [228, 233] bare "count")
        (scalar [234, 236] bare "42"))
      (entry
        (scalar [237, 241] bare "tags")
        (sequence [242, 249]
          (scalar [243, 244] bare "a")
          (scalar [245, 246] bare "b")
          (scalar [247, 248] bare "c")))
      (entry
        (scalar [250, 254] bare "opts")
        (object [255, 260] comma
          (entry
            (scalar [256, 257] bare "x")
            (scalar [258, 259] bare "1"))
        ))
    ))
  (entry
    (scalar [299, 302] bare "url")
    (scalar [303, 338] bare "https://example.com?foo=bar&baz=qux"))
  (entry
    (scalar [368, 375] bare "contact")
    (scalar [376, 392] bare "user@example.com"))
  (entry
    (scalar [427, 437] bare "dependency")
    (scalar [438, 451] bare "crate:myapp@2"))
)
; file: compliance/corpus/00-basic/empty.styx
(document [-1, -1]
)
; file: compliance/corpus/00-basic/implicit-unit.styx
(document [-1, -1]
  (entry
    (scalar [52, 59] bare "enabled")
    (unit [52, 59]))
  (entry
    (scalar [60, 67] bare "verbose")
    (unit [60, 67]))
  (entry
    (scalar [68, 75] bare "dry_run")
    (unit [68, 75]))
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
; file: compliance/corpus/00-basic/paths.styx
(document [-1, -1]
  (entry
    (scalar [35, 41] bare "server")
    (object [35, 46] newline
      (entry
        (scalar [42, 46] bare "host")
        (scalar [47, 56] bare "localhost"))
    ))
  (entry
    (scalar [57, 65] bare "database")
    (object [57, 70] newline
      (entry
        (scalar [66, 70] bare "port")
        (scalar [71, 75] bare "5432"))
    ))
  (entry
    (scalar [76, 81] bare "cache")
    (object [76, 85] newline
      (entry
        (scalar [82, 85] bare "ttl")
        (scalar [86, 90] bare "3600"))
    ))
)
; file: compliance/corpus/00-basic/schema-declaration.styx
(document [-1, -1]
  (entry
    (tag [49, 56] "schema")
    (scalar [57, 74] bare "myapp.schema.styx"))
  (entry
    (scalar [75, 79] bare "name")
    (scalar [80, 85] bare "myapp"))
  (entry
    (scalar [86, 93] bare "version")
    (scalar [94, 99] bare "1.0.0"))
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
    (tag [0, 7] "schema")
    (scalar [8, 19] bare "schema.styx"))
  (entry
    (scalar [20, 24] bare "name")
    (scalar [25, 30] bare "hello"))
)
; file: compliance/corpus/00-basic/unit-value.styx
(document [-1, -1]
  (entry
    (scalar [0, 7] bare "nothing")
    (unit [8, 9]))
)
; file: compliance/corpus/01-scalars/bare-at-in-middle.styx
(document [-1, -1]
  (entry
    (scalar [66, 71] bare "email")
    (scalar [72, 88] bare "user@example.com"))
  (entry
    (scalar [89, 94] bare "crate")
    (scalar [95, 106] bare "myapp@2.0.0"))
  (entry
    (scalar [107, 114] bare "version")
    (scalar [115, 124] bare "pkg@1.0.0"))
  (entry
    (scalar [125, 130] bare "multi")
    (scalar [131, 142] bare "foo@bar@baz"))
)
; file: compliance/corpus/01-scalars/bare-equals-in-middle.styx
(document [-1, -1]
  (entry
    (scalar [66, 69] bare "url")
    (scalar [70, 97] bare "https://example.com?foo=bar"))
  (entry
    (scalar [98, 103] bare "query")
    (scalar [104, 115] bare "a=1&b=2&c=3"))
  (entry
    (scalar [116, 120] bare "math")
    (scalar [121, 126] bare "x=y+z"))
  (entry
    (scalar [127, 137] bare "assignment")
    (scalar [138, 147] bare "var=value"))
)
; file: compliance/corpus/01-scalars/bare-paths.styx
(document [-1, -1]
  (entry
    (scalar [48, 52] bare "unix")
    (scalar [53, 66] bare "/usr/bin/styx"))
  (entry
    (scalar [67, 71] bare "root")
    (scalar [72, 73] bare "/"))
  (entry
    (scalar [74, 77] bare "etc")
    (scalar [78, 94] bare "/etc/styx/config"))
  (entry
    (scalar [95, 99] bare "home")
    (scalar [100, 108] bare "~/config"))
  (entry
    (scalar [109, 116] bare "windows")
    (scalar [117, 130] bare "C:/Users/name"))
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
; file: compliance/corpus/01-scalars/bare-urls.styx
(document [-1, -1]
  (entry
    (scalar [48, 52] bare "http")
    (scalar [53, 72] bare "https://example.com"))
  (entry
    (scalar [73, 76] bare "ftp")
    (scalar [77, 104] bare "ftp://files.example.com/pub"))
  (entry
    (scalar [105, 109] bare "file")
    (scalar [110, 135] bare "file:///home/user/doc.txt"))
  (entry
    (scalar [136, 143] bare "complex")
    (scalar [144, 198] bare "https://user:pass@example.com:8080/path?q=1&r=2#anchor"))
)
; file: compliance/corpus/01-scalars/heredoc-empty.styx
(document [-1, -1]
  (entry
    (scalar [0, 5] bare "empty")
    (scalar [6, 15] heredoc ""))
)
; file: compliance/corpus/01-scalars/heredoc-indented.styx
(document [-1, -1]
  (entry
    (scalar [28, 34] bare "config")
    (object [35, 87] newline
      (entry
        (scalar [41, 47] bare "script")
        (scalar [48, 85] heredoc "echo \"hello\"\necho \"world\"\n"))
    ))
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
    (scalar [5, 23] raw "C:\\Users\\name"))
  (entry
    (scalar [24, 29] bare "regex")
    (scalar [30, 40] raw "^\\d+$"))
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
; file: compliance/corpus/02-objects/trailing-comma.styx
(document [-1, -1]
  (entry
    (scalar [56, 62] bare "single")
    (object [63, 68] comma
      (entry
        (scalar [64, 65] bare "a")
        (scalar [66, 67] bare "1"))
    ))
  (entry
    (scalar [69, 77] bare "multiple")
    (object [78, 93] comma
      (entry
        (scalar [79, 80] bare "a")
        (scalar [81, 82] bare "1"))
      (entry
        (scalar [84, 85] bare "b")
        (scalar [86, 87] bare "2"))
      (entry
        (scalar [89, 90] bare "c")
        (scalar [91, 92] bare "3"))
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
; file: compliance/corpus/02-objects/with-attributes.styx
(document [-1, -1]
  (entry
    (scalar [29, 35] bare "config")
    (object [36, 92] newline
      (entry
        (scalar [42, 48] bare "server")
        (object [49, 63] comma
          (entry
            (scalar [49, 53] bare "host")
            (scalar [54, 63] bare "localhost"))
        ))
      (entry
        (scalar [68, 76] bare "database")
        (object [77, 90] comma
          (entry
            (scalar [77, 81] bare "host")
            (scalar [82, 90] bare "db.local"))
        ))
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
; file: compliance/corpus/03-sequences/with-tags.styx
(document [-1, -1]
  (entry
    (scalar [38, 44] bare "colors")
    (sequence [45, 64]
      (tag [46, 50] "red")
      (tag [51, 57] "green")
      (tag [58, 63] "blue")))
  (entry
    (scalar [65, 72] bare "results")
    (sequence [73, 119]
      (tag [77, 86] "ok"
        (object [77, 86] comma
          (entry
            (scalar [78, 83] bare "value")
            (scalar [84, 85] bare "1"))
        ))
      (tag [90, 99] "ok"
        (object [90, 99] comma
          (entry
            (scalar [91, 96] bare "value")
            (scalar [97, 98] bare "2"))
        ))
      (tag [104, 118] "err"
        (object [104, 118] comma
          (entry
            (scalar [105, 108] bare "msg")
            (scalar [109, 117] quoted "failed"))
        ))))
  (entry
    (scalar [120, 125] bare "mixed")
    (sequence [126, 150]
      (scalar [127, 132] bare "plain")
      (tag [133, 140] "tagged")
      (scalar [141, 149] quoted "quoted")))
)
; file: compliance/corpus/04-tags/as-keys.styx
(document [-1, -1]
  (entry
    (tag [57, 64] "string")
    (scalar [65, 72] quoted "hello"))
  (entry
    (tag [73, 77] "int")
    (scalar [78, 80] bare "42"))
  (entry
    (tag [81, 88] "schema")
    (scalar [89, 107] bare "nested.schema.styx"))
  (entry
    (tag [108, 113] "root")
    (tag [121, 135] "object"
      (object [121, 135] comma
        (entry
          (scalar [122, 126] bare "name")
          (tag [127, 134] "string"))
      )))
)
; file: compliance/corpus/04-tags/explicit-unit-payload.styx
(document [-1, -1]
  (entry
    (scalar [73, 79] bare "status")
    (tag [83, 84] "ok"))
  (entry
    (scalar [85, 92] bare "nothing")
    (tag [98, 99] "none"))
  (entry
    (scalar [100, 104] bare "flag")
    (tag [113, 114] "enabled"))
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
; file: compliance/corpus/04-tags/names-mixed.styx
(document [-1, -1]
  (entry
    (scalar [60, 67] bare "complex")
    (tag [68, 84] "foo_bar_baz-qux"))
  (entry
    (scalar [85, 92] bare "version")
    (tag [93, 107] "v2_0_0-beta_1"))
  (entry
    (scalar [108, 112] bare "full")
    (tag [113, 136] "com_example_my_type-v1"))
)
; file: compliance/corpus/04-tags/names-with-dashes.styx
(document [-1, -1]
  (entry
    (scalar [32, 37] bare "kebab")
    (tag [38, 46] "my-type"))
  (entry
    (scalar [47, 51] bare "http")
    (tag [52, 65] "content-type"))
  (entry
    (scalar [66, 71] bare "multi")
    (tag [72, 84] "foo-bar-baz"))
)
; file: compliance/corpus/04-tags/names-with-underscores.styx
(document [-1, -1]
  (entry
    (scalar [60, 69] bare "qualified")
    (tag [70, 89] "com_example_MyType"))
  (entry
    (scalar [90, 96] bare "nested")
    (tag [97, 109] "foo_bar_baz"))
  (entry
    (scalar [110, 117] bare "version")
    (tag [118, 125] "v1_0_0"))
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
    (scalar [0, 6] bare "status")
    (tag [7, 10] "ok"))
  (entry
    (scalar [11, 16] bare "level")
    (tag [17, 22] "warn"))
  (entry
    (scalar [23, 30] bare "nothing")
    (tag [31, 36] "none"))
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
; file: compliance/corpus/06-edge-cases/consecutive-tags.styx
(document [-1, -1]
  (entry
    (scalar [31, 36] bare "items")
    (sequence [37, 53]
      (tag [38, 41] "ok")
      (tag [42, 46] "err")
      (tag [47, 52] "none")))
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
; file: compliance/corpus/06-edge-cases/quoted-keys.styx
(document [-1, -1]
  (entry
    (scalar [38, 55] quoted "key with spaces")
    (scalar [56, 61] bare "value"))
  (entry
    (scalar [62, 67] quoted "123")
    (scalar [68, 87] bare "numeric-looking-key"))
  (entry
    (scalar [88, 90] quoted "")
    (scalar [91, 100] bare "empty-key"))
)
; file: compliance/corpus/06-edge-cases/raw-keys.styx
(document [-1, -1]
  (entry
    (scalar [35, 47] raw "raw key")
    (scalar [48, 53] bare "value"))
  (entry
    (scalar [54, 78] raw "key with \"# in it")
    (scalar [79, 86] bare "another"))
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
; file: compliance/corpus/07-invalid/duplicate-keys.styx
(error [45, 49] "parse error at 45-49: duplicate key")
; file: compliance/corpus/07-invalid/heredoc-as-key.styx
(error [35, 49] "parse error at 35-49: invalid key")
; file: compliance/corpus/07-invalid/invalid-escape.styx
(error [13, 15] "parse error at 13-15: invalid escape sequence: \\x")
; file: compliance/corpus/07-invalid/invalid-tag-digit.styx
(error [42, 45] "parse error at 42-45: invalid tag name")
; file: compliance/corpus/07-invalid/invalid-tag-dot.styx
(error [40, 44] "parse error at 40-44: invalid tag name")
; file: compliance/corpus/07-invalid/invalid-tag-hyphen.styx
(error [43, 47] "parse error at 43-47: invalid tag name")
; file: compliance/corpus/07-invalid/mixed-separators.styx
(error [13, 14] "parse error at 13-14: mixed separators (use either commas or newlines)")
; file: compliance/corpus/07-invalid/object-as-key.styx
(document [-1, -1]
  (entry
    (unit [-1, -1])
    (object [34, 39] comma
      (entry
        (scalar [35, 36] bare "a")
        (scalar [37, 38] bare "1"))
    ))
)
; file: compliance/corpus/07-invalid/reopen-path.styx
(error [62, 67] "parse error at 62-67: duplicate key")
; file: compliance/corpus/07-invalid/sequence-as-key.styx
(error [36, 43] "parse error at 36-43: invalid key")
; file: compliance/corpus/07-invalid/too-many-atoms.styx
(document [-1, -1]
  (entry
    (scalar [137, 140] bare "key")
    (tag [145, 147] "tag"
      (object [145, 147] comma)))
)
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
