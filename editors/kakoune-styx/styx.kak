# Styx syntax highlighting for Kakoune

# Detection
hook global BufCreate .*\.styx %{
    set-option buffer filetype styx
}

# Highlighters
add-highlighter shared/styx regions
add-highlighter shared/styx/code default-region group

# Comments
add-highlighter shared/styx/doc_comment region '///' '$' fill comment
add-highlighter shared/styx/line_comment region '//[^/]' '$' fill comment

# Strings
add-highlighter shared/styx/string region '"' '(?<!\\)"' group
add-highlighter shared/styx/string/ fill string
add-highlighter shared/styx/string/ regex '\\(?:[\\\"nrt]|u[0-9A-Fa-f]{4}|u\{[0-9A-Fa-f]{1,6}\})' 0:keyword

# Raw strings
add-highlighter shared/styx/raw_string region 'r#*"' '"#*' fill string

# Heredocs
add-highlighter shared/styx/heredoc region '<<([A-Za-z_][A-Za-z0-9_]*)' '^\s*\1\s*$' fill string

# Code region
add-highlighter shared/styx/code/ regex '@[A-Za-z_][A-Za-z0-9_-]*' 0:type
add-highlighter shared/styx/code/ regex '@(?![A-Za-z_])' 0:value
add-highlighter shared/styx/code/ regex '([A-Za-z_][A-Za-z0-9_-]*)\s*>' 1:attribute
add-highlighter shared/styx/code/ regex '>' 0:keyword
add-highlighter shared/styx/code/ regex '[{}()]' 0:operator
add-highlighter shared/styx/code/ regex ',' 0:operator

# Indentation
set-option global indentwidth 2

hook global WinSetOption filetype=styx %{
    add-highlighter window/styx ref styx
    set-option window comment_line '//'

    hook -once -always window WinSetOption filetype=.* %{
        remove-highlighter window/styx
    }
}

# Commands
define-command -hidden styx-indent-on-newline %{
    evaluate-commands -draft -itersel %{
        # Preserve previous line indent
        try %{ execute-keys -draft <semicolon>K<a-&> }
        # Indent after opening brace/paren
        try %{ execute-keys -draft k<a-x> <a-k>[{(]\h*$<ret> j<a-gt> }
    }
}

hook global WinSetOption filetype=styx %{
    hook window InsertChar \n styx-indent-on-newline
}
