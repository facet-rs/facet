((normal_element
  (start_tag
    name: (tag_name) @_tag)
  (text) @injection.content)
  (#eq? @_tag "style")
  (#set! injection.language "css")
  (#set! injection.combined))
