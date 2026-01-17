; Language injection for heredocs with language hint
; e.g., <<SQL,sql or <<CODE,javascript
((heredoc
  (heredoc_lang) @injection.language
  (heredoc_content) @injection.content))
