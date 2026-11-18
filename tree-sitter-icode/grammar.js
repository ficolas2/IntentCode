// Tree-sitter grammar for IntentCode (.icode).
//
// Bracket roles (see project design notes):
//   [ ]  LLM-interpreted, free-form  -> parsed leniently (prose words)
//   ( )  compiler-interpreted        -> parameters / arguments
//   < >  holes the LLM fills in
//   lang { }  verbatim target code   -> `code` node, injectable
//
// The grammar is deliberately lenient: prose inside [ ] is never a syntax error.

const KINDS = ['data', 'component', 'behaviour', 'test'];
const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'];

module.exports = grammar({
  name: 'icode',

  word: $ => $.identifier,
  externals: $ => [$.heading, $.list_marker],
  extras: $ => [/\s/, $.comment],

  conflicts: $ => [
    [$.named_block, $._prose_item],
    [$.subintent, $._prose_item],
    [$.literal_block, $._prose_item],
    [$.label, $._prose_item],
    [$._member, $._prose_item],
    [$.declaration, $._prose_item],
    [$.prose],
    [$.reference],
  ],

  rules: {
    source_file: $ => repeat($._top),

    _top: $ => choice($.import, $.declaration),

    comment: _ => token(seq('//', /[^\n]*/)),

    // #import utils.icode
    import: $ => seq('#import', field('path', $.path)),
    path: _ => /[^\s]+/,

    // data Link [ ... ] | component Repository [ ... ] | behaviour retry [ ... ] | test name [ ... ]
    declaration: $ => seq(
      field('kind', $.kind),
      field('name', $.identifier),
      field('body', $.block),
    ),
    kind: _ => choice(...KINDS),

    block: $ => seq('[', repeat($._member), ']'),

    _member: $ => choice(
      $.declaration,
      $.route,
      $.subintent,
      $.named_block,
      $.literal_block,
      $.block,
      $.prose,
    ),

    // insert(Link) -> RepositoryError [ ... ]
    subintent: $ => prec.dynamic(5, seq(
      field('name', $.identifier),
      field('parameters', $.parameters),
      optional(field('returns', $.returns)),
      field('body', $.block),
    )),
    parameters: $ => seq('(', optional(commaSep($._parameter)), ')'),
    _parameter: $ => choice($.reference, $.parameter),
    parameter: _ => /[^\s,()\[\]{}@]+/,
    returns: $ => seq('->', commaSep($.type)),
    // Link   time?   list(Link)   map(text, Link)
    type: $ => seq(field('name', $.type_name), optional($.type_arguments)),
    type_name: _ => /[A-Za-z_][A-Za-z0-9_.]*\??/,
    type_arguments: $ => seq('(', commaSep($.type), ')'),

    // GET /url?slug -> LinkResponse [ ... ]
    route: $ => seq(
      field('method', $.http_method),
      field('path', $.route_path),
      optional($.brace_block),
      optional(field('returns', $.returns)),
      field('body', $.block),
    ),
    http_method: _ => choice(...METHODS),
    route_path: _ => /\/[^\s\[\]]*/,

    // backoff [ ... ]   attempts []   assumption [ ... ]
    named_block: $ => prec.dynamic(5, seq(field('name', $.identifier), field('body', $.block))),

    // rust { ... }
    literal_block: $ => prec.dynamic(5, seq(field('language', $.identifier), $.brace_block)),
    brace_block: $ => seq('{', repeat(choice($.brace_block, $.code)), '}'),
    code: _ => /[^{}]+/,

    // free-form text (LLM interpreted): a run of consecutive items, injected as markdown
    prose: $ => prec.dynamic(-1, repeat1($._prose_item)),

    _prose_item: $ => choice(
      $.heading,
      $.list_marker,
      $.label,
      $.reference,
      $.hole,
      $.constraint,
      $.arguments,
      $.arrow,
      $.word,
      $.identifier,
      alias($.kind, $.word),
      alias($.http_method, $.word),
      $.brace_block,
    ),

    label: $ => prec.dynamic(1, seq($.identifier, ':')),
    arrow: _ => '->',

    // @retry(...)   @Repository.get(slug)   @Link(...)
    // The name sticks to the sigil (no whitespace allowed), so "@ x" is not a call.
    reference: $ => seq(
      '@',
      field('name', alias($._imm_identifier, $.identifier)),
      repeat(seq(token.immediate('.'), field('member', alias($._imm_identifier, $.identifier)))),
      optional(field('arguments', $.arguments)),
    ),
    _imm_identifier: _ => token.immediate(/[A-Za-z_][A-Za-z0-9_]*/),
    arguments: $ => seq('(', repeat(choice($.reference, $.arguments, $.slice, $.argument)), ')'),
    // hash[0:4]  (an index or slice directly inside call arguments)
    slice: _ => /\[[^\[\]\n]*\]/,
    argument: _ => /[^\s()\[\]{}@]+/,

    // <slug_generation>  or  <hashing algorithm>  (a letter first, no nesting, one line)
    hole: _ => /<[A-Za-z_][^<>\n`\[\]()]*>/,
    // `4 < len < 12`
    constraint: _ => /`[^`\n]*`/,

    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,
    // '@' may appear inside a word (a@b.com) but not start one (that is a reference)
    word: _ => token(prec(-1, /[^\s\[\]()\{\}@<`][^\s\[\]()\{\}<`]*/)),
  },
});

function commaSep(rule) {
  return seq(rule, repeat(seq(',', rule)));
}
