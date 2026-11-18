; Parse the body of `rust { ... }` (or any `<lang> { ... }`) as that language.
; The whole brace_block is the content (so nested braces stay intact), trimmed of the outer { }.
((literal_block
   language: (identifier) @injection.language
   (brace_block) @injection.content)
 (#offset! @injection.content 0 1 0 -1)
 (#set! injection.include-children))

; Prose inside [ ] is (mostly) markdown. Only inline markdown is injected: block-level
; markdown would read the indentation as code blocks. Our own highlights (calls, holes,
; labels, ...) are layered on top.
((prose) @injection.content
 (#set! injection.language "markdown_inline")
 (#set! injection.include-children))
