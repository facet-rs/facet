; Language injection queries for Styx
;
; Heredocs can specify a language: <<SQL,sql
; The heredoc_lang node captures the language name (e.g., "sql")
; The heredoc_content node contains the actual content to highlight

(heredoc
  (heredoc_lang) @injection.language
  (heredoc_content) @injection.content)
