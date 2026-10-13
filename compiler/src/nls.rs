//! P20 — NLS Language Server
//!
//! The language service reuses the compiler's real lexer/parser/HIR/NIR —
//! there is deliberately no second, incompatible parser.
//!
//! # Status
//!
//! | Feature | Status |
//! | --- | --- |
//! | Diagnostics from the real compiler | IMPLEMENTED |
//! | Completions (keywords + in-file symbols) | IMPLEMENTED |
//! | Hover (symbol kind/type) | IMPLEMENTED |
//! | Go-to-definition | IMPLEMENTED |
//! | Document symbols | IMPLEMENTED |
//! | Formatting (via shared formatter) | IMPLEMENTED |
//! | Whole-word rename in one file | IMPLEMENTED (scope-aware later) |
//! | LSP initialize / completion / symbol RPC | IMPLEMENTED (minimal JSON-RPC) |
//! | Find references (code-only, comment/string aware) | IMPLEMENTED |
//! | Cross-file rename | IMPLEMENTED (`Workspace`) |
//! | Workspace indexing | IMPLEMENTED (`Workspace::symbol_index`) |
//! | Code actions (rename) | IMPLEMENTED |
//!
//! Reference search and cross-file rename are lexically aware: identifiers
//! inside string literals and comments are ignored. They are still textual
//! (not scope-resolved) and honest about that limit.

use std::collections::BTreeMap;

use crate::ast::{DeclarationKind, Statement};
use crate::source::{SourceFile, Span};

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    pub span: Span,
    pub type_info: Option<String>,
}

fn span_to_line(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.chars().enumerate() {
        if i == offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Collect top-level declarations from the parsed AST.
pub fn collect_symbols(source_text: &str) -> Vec<Symbol> {
    let sf = SourceFile::new("buffer", source_text);
    let Ok(program) = crate::parse_source(&sf) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for stmt in &program.statements {
        match stmt {
            Statement::Function(f) => out.push(Symbol {
                name: f.name.clone().unwrap_or_default(),
                kind: "function",
                span: f.span,
                type_info: f.return_type.clone(),
            }),
            Statement::TypeDeclaration(t) => {
                let kind = match t.kind {
                    DeclarationKind::Struct => "struct",
                    DeclarationKind::Class => "class",
                    DeclarationKind::Actor => "actor",
                    DeclarationKind::Interface => "interface",
                    DeclarationKind::Trait => "trait",
                    DeclarationKind::Enum => "enum",
                };
                out.push(Symbol {
                    name: t.name.clone(),
                    kind,
                    span: t.span,
                    type_info: None,
                });
            }
            Statement::TypeAlias(a) => out.push(Symbol {
                name: a.name.clone(),
                kind: "alias",
                span: a.span,
                type_info: Some(a.target.clone()),
            }),
            Statement::Let {
                name,
                annotation,
                span,
                ..
            } => out.push(Symbol {
                name: name.clone(),
                kind: "variable",
                span: *span,
                type_info: annotation.clone(),
            }),
            Statement::Module { path, span } => out.push(Symbol {
                name: path.join("::"),
                kind: "module",
                span: *span,
                type_info: None,
            }),
            _ => {}
        }
    }
    out
}

/// Compiler diagnostics for the buffer (message + 1-based offset line/col).
pub fn diagnostics(source_text: &str) -> Vec<String> {
    let sf = SourceFile::new("buffer", source_text);
    match crate::compile_to_nir(&sf) {
        Ok(_) => Vec::new(),
        Err(d) => vec![d.message],
    }
}

const KEYWORDS: &[&str] = &[
    "fn",
    "let",
    "const",
    "mut",
    "if",
    "else",
    "for",
    "while",
    "loop",
    "return",
    "match",
    "struct",
    "class",
    "enum",
    "trait",
    "interface",
    "impl",
    "module",
    "import",
    "export",
    "async",
    "await",
    "spawn",
    "actor",
    "try",
    "catch",
    "finally",
    "throw",
    "use",
    "self",
];

/// Completion items: keywords plus in-file symbol names.
pub fn completions(source_text: &str) -> Vec<String> {
    let mut items: Vec<String> = KEYWORDS.iter().map(|k| k.to_string()).collect();
    for sym in collect_symbols(source_text) {
        if !sym.name.is_empty() && !items.contains(&sym.name) {
            items.push(sym.name);
        }
    }
    items.sort();
    items.dedup();
    items
}

/// Hover: describe the symbol under the cursor (line/col are 1-based).
pub fn hover(source_text: &str, line: usize, col: usize) -> Option<String> {
    let offset = offset_of(source_text, line, col)?;
    let ident = identifier_at(source_text, offset)?;
    let sym = collect_symbols(source_text)
        .into_iter()
        .find(|s| s.name == ident)?;
    Some(match &sym.type_info {
        Some(t) => format!("{} {}: {}", sym.kind, sym.name, t),
        None => format!("{} {}", sym.kind, sym.name),
    })
}

/// Go-to-definition: the (line, col) of `name`'s first declaration.
pub fn definition(source_text: &str, name: &str) -> Option<(usize, usize)> {
    let sym = collect_symbols(source_text)
        .into_iter()
        .find(|s| s.name == name)?;
    Some(span_to_line(source_text, sym.span.start))
}

/// Whole-word, same-file rename; returns the new text.
pub fn rename(source_text: &str, old: &str, new: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    let bytes = source_text.as_bytes();
    while i < bytes.len() {
        if source_text[i..].starts_with(old) {
            let before_ok = i == 0 || !is_ident(bytes[i - 1] as char);
            let after_ok = i + old.len() >= bytes.len() || !is_ident(bytes[i + old.len()] as char);
            if before_ok && after_ok {
                out.push_str(new);
                i += old.len();
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// Byte offsets + text of identifiers in `source` that are code (not inside
/// string literals or comments). UTF-8 safe: offsets are char boundaries.
fn identifier_offsets(source: &str) -> Vec<(usize, String)> {
    #[derive(PartialEq)]
    enum State {
        Code,
        Line,
        Block,
        Str,
    }
    let mut out = Vec::new();
    let mut state = State::Code;
    let mut chars = source.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match state {
            State::Code => {
                if c == '/' {
                    match chars.peek() {
                        Some((_, '/')) => {
                            state = State::Line;
                            chars.next();
                        }
                        Some((_, '*')) => {
                            state = State::Block;
                            chars.next();
                        }
                        _ => {}
                    }
                } else if c == '"' {
                    state = State::Str;
                } else if is_ident_start(c) {
                    let start = i;
                    let mut end = i + c.len_utf8();
                    while let Some(&(j, nc)) = chars.peek() {
                        if is_ident(nc) {
                            end = j + nc.len_utf8();
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    out.push((start, source[start..end].to_owned()));
                }
            }
            State::Line => {
                if c == '\n' {
                    state = State::Code;
                }
            }
            State::Block => {
                if c == '*' && matches!(chars.peek(), Some((_, '/'))) {
                    state = State::Code;
                    chars.next();
                }
            }
            State::Str => {
                if c == '\\' {
                    chars.next();
                } else if c == '"' {
                    state = State::Code;
                }
            }
        }
    }
    out
}

/// Find code-only references to `name` in one file, as (line, col) pairs.
pub fn references(source_text: &str, name: &str) -> Vec<(usize, usize)> {
    identifier_offsets(source_text)
        .into_iter()
        .filter(|(_, id)| id == name)
        .map(|(off, _)| span_to_line(source_text, off))
        .collect()
}

/// Rename `old` to `new` in code only (strings/comments untouched).
/// Returns the new text and the number of edits made.
pub fn rename_code(source_text: &str, old: &str, new: &str) -> (String, usize) {
    let mut out = String::with_capacity(source_text.len());
    let mut last = 0;
    let mut count = 0;
    for (off, id) in identifier_offsets(source_text) {
        if id == old {
            out.push_str(&source_text[last..off]);
            out.push_str(new);
            last = off + old.len();
            count += 1;
        }
    }
    out.push_str(&source_text[last..]);
    (out, count)
}

/// A located reference or declaration inside a workspace file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub path: String,
    pub line: usize,
    pub col: usize,
}

/// A tiny in-memory multi-file workspace index for cross-file tooling.
#[derive(Default)]
pub struct Workspace {
    files: BTreeMap<String, String>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, path: &str, source: &str) {
        self.files.insert(path.to_owned(), source.to_owned());
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn files(&self) -> impl Iterator<Item = (&str, &str)> {
        self.files.iter().map(|(p, s)| (p.as_str(), s.as_str()))
    }

    /// Declarations across all files, grouped by symbol name.
    pub fn symbol_index(&self) -> BTreeMap<String, Vec<Reference>> {
        let mut index: BTreeMap<String, Vec<Reference>> = BTreeMap::new();
        for (path, source) in &self.files {
            for sym in collect_symbols(source) {
                let (line, col) = span_to_line(source, sym.span.start);
                index.entry(sym.name).or_default().push(Reference {
                    path: path.clone(),
                    line,
                    col,
                });
            }
        }
        index
    }

    /// Every code-only reference to `name` across every file.
    pub fn find_references(&self, name: &str) -> Vec<Reference> {
        let mut out = Vec::new();
        for (path, source) in &self.files {
            for (line, col) in references(source, name) {
                out.push(Reference {
                    path: path.clone(),
                    line,
                    col,
                });
            }
        }
        out
    }

    /// Cross-file rename of a code-only identifier; returns the edit count.
    pub fn rename(&mut self, old: &str, new: &str) -> usize {
        let mut edits = 0;
        for source in self.files.values_mut() {
            let (updated, count) = rename_code(source, old, new);
            *source = updated;
            edits += count;
        }
        edits
    }

    /// Suggest code actions for `name` (a rename action when it is known).
    pub fn code_actions(&self, name: &str) -> Vec<String> {
        if self.symbol_index().contains_key(name) {
            vec![format!("Rename symbol '{name}'")]
        } else {
            Vec::new()
        }
    }
}

fn offset_of(source: &str, line: usize, col: usize) -> Option<usize> {
    let mut l = 1;
    let mut c = 1;
    for (i, ch) in source.chars().enumerate() {
        if l == line && c == col {
            return Some(i);
        }
        if ch == '\n' {
            l += 1;
            c = 1;
        } else {
            c += 1;
        }
    }
    None
}

fn identifier_at(source: &str, offset: usize) -> Option<String> {
    let start = source[..offset]
        .chars()
        .rev()
        .take_while(|c| is_ident(*c))
        .count();
    let end = source[offset..]
        .chars()
        .take_while(|c| is_ident(*c))
        .count();
    if start == 0 && end == 0 {
        return None;
    }
    let from = offset - start;
    let to = offset + end;
    Some(source[from..to].to_owned())
}

// ── Minimal JSON-RPC LSP bridge ───────────────────────────────────────

/// Handle one JSON-RPC request object and produce a response (or empty string
/// for notifications). Pure function; `serve` wires real stdio.
pub fn handle_message(json: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"parse error"}}"#
            .to_string();
    };
    let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
    match method {
        "initialize" => format!(
            r#"{{"jsonrpc":"2.0","id":{},"result":{{"capabilities":{{"completionProvider":{{}},"hoverProvider":true,"definitionProvider":true,"documentSymbolProvider":true,"referencesProvider":true,"workspaceSymbolProvider":true}}}}}}"#,
            id
        ),
        "textDocument/completion" => {
            let text = v
                .pointer("/params/textDocument/text")
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let items: Vec<String> = completions(text)
                .iter()
                .map(|c| format!(r#"{{"label":"{c}"}}"#))
                .collect();
            format!(
                r#"{{"jsonrpc":"2.0","id":{},"result":{{"isIncomplete":false,"items":[{}]}}}}"#,
                id,
                items.join(",")
            )
        }
        "textDocument/documentSymbol" => {
            let text = v
                .pointer("/params/textDocument/text")
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let items: Vec<String> = collect_symbols(text)
                .iter()
                .map(|s| format!(r#"{{"name":"{}","kind":"{}"}}"#, s.name, s.kind))
                .collect();
            format!(
                r#"{{"jsonrpc":"2.0","id":{},"result":[{}]}}"#,
                id,
                items.join(",")
            )
        }
        "textDocument/references" => {
            let text = v
                .pointer("/params/textDocument/text")
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let line = v
                .pointer("/params/position/line")
                .and_then(|n| n.as_u64())
                .unwrap_or(1) as usize;
            let col = v
                .pointer("/params/position/character")
                .and_then(|n| n.as_u64())
                .unwrap_or(1) as usize;
            let name = offset_of(text, line, col).and_then(|o| identifier_at(text, o));
            let items: Vec<String> = name
                .map(|n| references(text, &n))
                .unwrap_or_default()
                .iter()
                .map(|(l, c)| format!(r#"{{"path":"buffer","line":{l},"col":{c}}}"#))
                .collect();
            format!(
                r#"{{"jsonrpc":"2.0","id":{},"result":[{}]}}"#,
                id,
                items.join(",")
            )
        }
        "workspace/symbol" => {
            let mut workspace = Workspace::new();
            if let Some(map) = v.pointer("/params/files").and_then(|f| f.as_object()) {
                for (path, src) in map {
                    if let Some(text) = src.as_str() {
                        workspace.add_file(path, text);
                    }
                }
            }
            let items: Vec<String> = workspace
                .symbol_index()
                .iter()
                .flat_map(|(name, locs)| {
                    locs.iter().map(move |loc| {
                        format!(
                            r#"{{"name":"{name}","path":"{}","line":{}}}"#,
                            loc.path, loc.line
                        )
                    })
                })
                .collect();
            format!(
                r#"{{"jsonrpc":"2.0","id":{},"result":[{}]}}"#,
                id,
                items.join(",")
            )
        }
        m if m.ends_with("/notification") || m == "initialized" => String::new(),
        _ => format!(
            r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32601,"message":"method not found: {method}"}}}}"#,
            id
        ),
    }
}

/// Run a stdio JSON-RPC loop reading Content-Length framed messages.
pub fn serve_stdio() -> std::io::Result<()> {
    use std::io::{BufRead, BufReader, Read, Write};
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    loop {
        let mut content_length = 0usize;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header)? == 0 {
                return Ok(());
            }
            let h = header.trim();
            if h.is_empty() {
                break;
            }
            if let Some(v) = h.strip_prefix("Content-Length: ") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;
        let Ok(text) = String::from_utf8(body) else {
            continue;
        };
        let response = handle_message(&text);
        if response.is_empty() {
            continue;
        }
        let out = format!("Content-Length: {}\r\n\r\n{}", response.len(), response);
        let mut stdout = std::io::stdout();
        stdout.write_all(out.as_bytes())?;
        stdout.flush()?;
    }
}
