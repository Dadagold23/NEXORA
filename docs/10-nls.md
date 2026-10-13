# NLS — Language Server (P20) and IDE delegation

NLS reuses the real compiler (`parse_source`, `compile_to_nir`); no second parser exists.

```bash
nxl lsp     # stdio JSON-RPC: Content-Length framing, initialize/completion/documentSymbol
```

API (`nxl_compiler::nls`):

| fn | what |
| --- | --- |
| diagnostics(src) | compiler errors on the buffer |
| collect_symbols(src) | top-level symbols with kinds/spans |
| completions(src) | keywords + in-file symbol names |
| hover(src, line, col) | `kind name: type?` for identifier under cursor |
| definition(src, name) | 1-based (line, col) of first declaration |
| rename(src, old, new) | whole-word, same-file, identifier-boundary-safe |
| handle_message(json) | pure JSON-RPC handler; serve_stdio() wires it |

NOT IMPLEMENTED: find references, cross-file rename, workspace indexing, code actions, refactorings.

NIDE (P22) consumes the same symbols/diagnostics through `IdeModel` — see `docs/15-debugger-ide.md`.
