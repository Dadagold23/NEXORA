# Contributing to NEXORA / NXL

Thanks for considering a contribution. This document is the short version; the
repository's own rules are simple enough to follow directly.

## Ground rules

- **Keep the suite green.** `cargo test` must pass completely and
  `cargo clippy --all-targets -- -D warnings` must be clean. The release
  pipeline refuses to package a build that fails either.
- **Never claim something works without evidence.** If a change affects a
  documented behaviour, update the documentation and add a test that exercises
  it. If something is only planned, say so — "not implemented yet" with an
  explanation is better than a stub that appears to work.
- **Fail loudly, never silently.** Code paths that cannot produce correct
  results should return a clear diagnostic and a non-zero exit code, not emit
  something that looks like success.
- **Do not commit credentials.** No API keys, signing material or personal
  access tokens, in code, tests, examples or release artifacts.

## Getting set up

```bash
git clone https://github.com/Dadagold23/NEXORA.git
cd NEXORA/compiler
cargo build
cargo test
```

Rust stable is the only requirement. Node.js is optional (it enables the
JavaScript backend); everything runs on the VM without it.

## Making a change

1. Open an issue first for anything larger than a bug fix, so the design can be
   agreed before the work.
2. Work on a branch, keep commits focused, and write commit messages that
   explain *why*.
3. Add tests for new behaviour and for every bug you fix.
4. Run the gates before opening a pull request:

```bash
cd compiler
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

5. Update the relevant file under `docs/` and add an entry to `CHANGELOG.md`.

## Language changes

Changes to the language itself are larger than they look: they touch the
parser, HIR, NIR, the VM, both backends, the IDE/LSP surface and the
documentation, and they need a migration story for existing programs. Open an
issue describing the feature and its semantics before writing code.

Every accepted language change has an approval record (`APPROVAL_LANG_P*.txt`)
so the rationale and the evidence behind each phase stays visible.

## Reporting bugs

Open an issue with:

- the NXL version (`nxlc --version`) and platform,
- the smallest program that reproduces the problem,
- what you expected and what happened, with the exact diagnostic.

Security issues go through [SECURITY.md](SECURITY.md) instead.

## Code of conduct

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## License

By contributing you agree that your contributions are licensed under the MIT
License that covers this repository.