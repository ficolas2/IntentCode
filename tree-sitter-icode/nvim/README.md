# nvim setup (nvim-treesitter, `master` branch)

Add this in your treesitter config, before `require('nvim-treesitter.configs').setup`:

```lua
local icode = vim.fn.expand('~/projects/intentcode/tree-sitter-icode')
vim.filetype.add({ extension = { icode = 'icode' } })
vim.opt.rtp:append(icode .. '/nvim')   -- makes nvim find queries/icode/*.scm
require('nvim-treesitter.parsers').get_parser_configs().icode = {
  install_info = { url = icode, files = { 'src/parser.c', 'src/scanner.c' } },
  filetype = 'icode',
}
```

Then run `:TSInstall icode` once (and again after changing `grammar.js`).

Regenerate the parser after editing the grammar: `tree-sitter generate` in this folder.

## Heading colors

Colorschemes usually define heading colors only as `@markup.heading.N.markdown`. To get the same
per-level colors in `.icode`, link them (add next to the snippet above):

```lua
local function icode_heading_hl()
  for i = 1, 6 do
    vim.api.nvim_set_hl(0, '@markup.heading.' .. i .. '.icode', { link = '@markup.heading.' .. i .. '.markdown' })
  end
end
icode_heading_hl()
vim.api.nvim_create_autocmd('ColorScheme', { callback = icode_heading_hl })
```
