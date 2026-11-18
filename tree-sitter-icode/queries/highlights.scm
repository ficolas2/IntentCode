; ---- general ----
; comments win over anything injected into the surrounding prose (markdown, html tags, ...)
((comment) @comment (#set! priority 200))
["[" "]" "(" ")" "{" "}"] @punctuation.bracket
"," @punctuation.delimiter
(arrow) @operator
"->" @operator
(word) @spell

; ---- imports ----
"#import" @keyword.import
(import path: (path) @string.special.path)

; ---- intents: keyword in one color, name in another ----
(declaration kind: (kind) @keyword)
(declaration name: (identifier) @type)
((declaration kind: (kind) @_kind name: (identifier) @function)
  (#any-of? @_kind "behaviour" "test"))

; ---- sub-intents: insert(Link) -> RepositoryError [ ... ] ----
(subintent name: (identifier) @function)
(parameter) @variable.parameter
((parameter) @type (#match? @type "^[A-Z]"))
(type_name) @type
(type_arguments ["(" ")"] @punctuation.bracket)

; ---- routes ----
(http_method) @keyword.operator
(route_path) @string.special.url

; ---- named blocks, labels, markers ----
(named_block name: (identifier) @property)
(label (identifier) @property)
((named_block name: (identifier) @keyword) (#eq? @keyword "assumption"))
((label (identifier) @keyword) (#eq? @keyword "usage"))

; ---- intent references: @retry(...)  @Repository.get(slug) ----
(reference "@" @punctuation.special)
(reference name: (identifier) @function.call)
((reference name: (identifier) @type) (#match? @type "^[A-Z]"))
(reference member: (identifier) @function.method.call)
(argument) @variable
(slice) @variable

; ---- holes, literal blocks (backtick spans are left to markdown inline code) ----
(hole) @string.special.symbol
(literal_block language: (identifier) @label)

; ---- markdown-style block markers (line start only) ----
(heading) @markup.heading
((heading) @markup.heading.1 (#lua-match? @markup.heading.1 "^#%s"))
((heading) @markup.heading.2 (#lua-match? @markup.heading.2 "^##%s"))
((heading) @markup.heading.3 (#lua-match? @markup.heading.3 "^###%s"))
((heading) @markup.heading.4 (#lua-match? @markup.heading.4 "^####%s"))
((heading) @markup.heading.5 (#lua-match? @markup.heading.5 "^#####%s"))
((heading) @markup.heading.6 (#lua-match? @markup.heading.6 "^######%s"))
(list_marker) @markup.list

; ---- pseudocode keywords in prose (no meaning in the language, highlighted for readability) ----
; Only plain words in prose (not names, labels or references). Lowercase and Capitalized forms.
((prose (identifier) @keyword.conditional)
  (#any-of? @keyword.conditional "if" "If" "else" "Else" "elif" "Elif" "unless" "Unless" "otherwise" "Otherwise"))
((prose (identifier) @keyword.repeat)
  (#any-of? @keyword.repeat "for" "For" "foreach" "Foreach" "each" "Each" "while" "While" "until" "Until" "loop" "Loop"))
((prose (identifier) @keyword.operator)
  (#any-of? @keyword.operator "not" "Not" "and" "And" "or" "Or" "where" "Where"))
((prose (identifier) @keyword.return)
  (#any-of? @keyword.return "return" "Return" "returns" "Returns"))
((prose (identifier) @constant.builtin)
  (#any-of? @constant.builtin "true" "True" "false" "False" "none" "None" "null" "Null" "nil" "Nil"))

; ---- plain calls without "@": links.contains(target)  (different color from !calls) ----
((prose (word) @function.builtin . (arguments))
  (#match? @function.builtin "[.]"))

; test structure words, capitalized only (to avoid noise in normal prose)
((prose (identifier) @keyword)
  (#any-of? @keyword "Given" "When" "Then" "Expect"))
