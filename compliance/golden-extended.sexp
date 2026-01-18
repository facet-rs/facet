; file: compliance/corpus-extended/01-scalars/heredoc-dedent-in-object.styx
(document [-1, -1]
  (entry
    (scalar [60, 66] bare "config")
    (object [67, 159] newline
      (entry
        (scalar [73, 79] bare "script")
        (scalar [80, 141] heredoc "echo \"hello\"\necho \"world\"\n"))
      (entry
        (scalar [146, 151] bare "other")
        (scalar [152, 157] bare "value"))
    ))
)
; file: compliance/corpus-extended/01-scalars/heredoc-dedent.styx
(document [-1, -1]
  (entry
    (scalar [80, 83] bare "key")
    (scalar [84, 117] heredoc "line one\nline two\n"))
)
