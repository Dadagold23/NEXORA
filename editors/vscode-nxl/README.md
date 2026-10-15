# NXL for Visual Studio Code

Syntax highlighting, a file icon for `.nxl`, a language server, and commands
that run the real compiler.

## What you get

- **File icon**: `.nxl` files get the NEXORA logo in the explorer and tabs,
  the same way `.rs`, `.py`, `.php` and `.sql` files do. The icon comes from
  the repository's `favicon.ico` (see below).
- **Highlighting** for keywords, types, declarations, string interpolation,
  comments and operators. The keyword list is taken from the compiler's own
  lexer (`compiler/src/lexer.rs`), so it cannot drift silently.
- **Language server** (`nxlc lsp`): completion, hover, go-to-definition,
  find-references, the symbol outline and diagnostics. Diagnostics come from the
  real compiler, so they are the same errors `nxlc check` reports.
- **Commands**: `NXL: Run File`, `NXL: Check File`, `NXL: Build Project`. The
  run command also appears as a play button in the editor title bar for `.nxl`
  files. Output goes to an "NXL" output channel, including the compiler's exit
  code.

## Requirements

The `nxlc` compiler on your PATH. Point the extension elsewhere with:

```jsonc
"nxl.compilerPath": "/full/path/to/nxlc"
```

If it cannot be started, the commands and highlighting still work and the
language-server features are simply absent — they are never faked.

## Not included yet

No cross-file rename or refactoring, and no completions that use types rather
than names: the completion list is the language's keywords plus the symbols in
the open document. The NLS module documents exactly what it answers and what it
does not (`compiler/src/nls.rs`).

## How the language client works

`language-client.js` is a dependency-free JSON-RPC client: it spawns
`nxlc lsp` and speaks Content-Length framed messages over stdio, so the
extension needs no `npm install` and loads from source. It is deliberately
small — it implements only what the NXL server answers, and requests time out
rather than hanging the editor if the server stops.

## Installing from source

```bash
cd editors/vscode-nxl
npm install            # no runtime dependencies, this is a formality
code --extension-extensionDevelopmentPath="$PWD"
```

Then open any `.nxl` file. Package it with `vsce package` to publish.

## Icons

`icons/nxl-32.png`, `nxl-64.png`, `nxl-128.png`, `icons/nxl.svg` and
`media/gallery-banner.png` are generated from `favicon.ico` in the repository
root by one script, so the file icons and the marketplace banner can never
drift apart:

```powershell
# from the repository root
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\make-brand-assets.ps1
```

Two notes about the source asset:

1. VS Code does **not** accept `.ico` for language icons — only SVG and PNG —
   so the ICO is decoded and re-rendered at each size. The SVG wraps the 64px
   PNG so it scales cleanly with no build step; replace it with a hand-authored
   vector if you prefer.
2. The checked-in `favicon.ico` has a non-standard header: it declares a
   769x771 image while decoding as 769x751, and its background is opaque white
   rather than transparency. The generator repairs both — it flood fills the
   border-connected background into alpha before scaling, which is what keeps
   white halos out of the corners. Replacing the ICO with a properly exported,
   multi-resolution file would remove that repair step.

## Verifying

```bash
node scripts/verify-extension.js
```

Checks that the manifest is complete, that `.nxl` is claimed, that the icon and
banner files exist at the sizes the marketplace expects, that the generated PNG
and SVG assets are present, that `extension.js` loads and uses the language
client, and that both entry points parse — all without needing the VS Code
toolchain.

From the repository root, two more checks exercise the language server against
the real binary:

```bash
node scripts/lsp-smoke.js compiler/target/release/nxlc.exe
node scripts/lsp-client-check.js compiler/target/release/nxlc.exe
```

The second one drives this extension's own `language-client.js`, so a client
that cannot hold a conversation with the server fails rather than quietly
offering no completion. Both run as part of `cargo test`.