; Inject each fenced block into the language named by its fence label.
(fence
  language: (language) @injection.language
  body: (body) @injection.content)
