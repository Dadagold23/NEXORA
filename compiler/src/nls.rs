//! P20 â€” NLS Language Server
//!
//! The language service reuses the compiler's real lexer/parser/HIR/NIR â€”
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

// ── LSP server ──────────────────────────────────────────────────────────
//
// The Language Server Protocol is a request/response protocol over a stream:
// the client opens a document with `textDocument/didOpen` and keeps it
// up to date with `textDocument/didChange`, then asks questions about it.
// Requests carry a URI, never the text.
//
// An earlier version of this file took the source inline in every request,
// which no LSP client ever sends, and advertised hover and definition that it
// did not answer. Both are corrected here.

/// A document the client currently has open.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub text: String,
    pub version: i64,
}

/// A stateful language server for one client connection.
#[derive(Debug, Default)]
pub struct Server {
    documents: BTreeMap<String, Document>,
    shutdown_requested: bool,
    exit_requested: bool,
}

impl Server {
    pub fn new() -> Self {
        Self::default()
    }

    /// The text the client last sent for `uri`.
    pub fn document(&self, uri: &str) -> Option<&Document> {
        self.documents.get(uri)
    }

    pub fn open_document_count(&self) -> usize {
        self.documents.len()
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown_requested
    }

    /// Set once the client has asked the server to stop.
    pub fn should_exit(&self) -> bool {
        self.exit_requested
    }

    /// Handle one incoming message.
    ///
    /// Returns every message to send back: usually a single response, nothing
    /// for a notification, and a response plus diagnostics after a change.
    pub fn handle(&mut self, json: &str) -> Vec<String> {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
            return vec![error_response(
                serde_json::Value::Null,
                -32700,
                "parse error",
            )];
        };
        let method = value
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_owned();
        let id = value.get("id").cloned();
        let params = value
            .get("params")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        match method.as_str() {
            "initialize" => vec![response(
                id,
                serde_json::json!({
                    "capabilities": {
                        // 1 = the client sends the whole document on change.
                        "textDocumentSync": 1,
                        "completionProvider": { "triggerCharacters": [".", " "] },
                        "hoverProvider": true,
                        "definitionProvider": true,
                        "documentSymbolProvider": true,
                        "referencesProvider": true,
                    },
                    "serverInfo": { "name": "nxlc", "version": env!("CARGO_PKG_VERSION") },
                }),
            )],
            "initialized" => Vec::new(),
            "shutdown" => {
                self.shutdown_requested = true;
                vec![response(id, serde_json::Value::Null)]
            }
            "exit" => {
                self.exit_requested = true;
                Vec::new()
            }
            "textDocument/didOpen" => {
                let uri = document_uri(&params);
                if let Some(uri) = uri {
                    let text = params
                        .pointer("/textDocument/text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_owned();
                    let version = params
                        .pointer("/textDocument/version")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    self.documents
                        .insert(uri.clone(), Document { text, version });
                    return self.diagnostics_for(&uri);
                }
                Vec::new()
            }
            "textDocument/didChange" => {
                let uri = document_uri(&params);
                if let Some(uri) = uri {
                    // Full synchronisation: the last change carries everything.
                    let text = params
                        .pointer("/contentChanges")
                        .and_then(|changes| changes.as_array())
                        .and_then(|changes| changes.last())
                        .and_then(|change| change.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_owned();
                    let version = params
                        .pointer("/textDocument/version")
                        .and_then(|v| v.as_i64())
                        .unwrap_or_else(|| {
                            self.documents
                                .get(&uri)
                                .map(|document| document.version + 1)
                                .unwrap_or(1)
                        });
                    self.documents
                        .insert(uri.clone(), Document { text, version });
                    return self.diagnostics_for(&uri);
                }
                Vec::new()
            }
            "textDocument/didSave" => match document_uri(&params) {
                Some(uri) => self.diagnostics_for(&uri),
                None => Vec::new(),
            },
            "textDocument/didClose" => match document_uri(&params) {
                Some(uri) => {
                    self.documents.remove(&uri);
                    // Clear the squiggles the client is still showing.
                    vec![notification(
                        "textDocument/publishDiagnostics",
                        serde_json::json!({ "uri": uri, "diagnostics": [] }),
                    )]
                }
                None => Vec::new(),
            },
            "textDocument/completion" => {
                let Some(text) = self.text_for(&params) else {
                    return vec![response(
                        id,
                        serde_json::json!({ "isIncomplete": false, "items": [] }),
                    )];
                };
                let items = completions(&text)
                    .into_iter()
                    .map(|label| serde_json::json!({ "label": label, "kind": 1 }))
                    .collect::<Vec<_>>();
                vec![response(
                    id,
                    serde_json::json!({ "isIncomplete": false, "items": items }),
                )]
            }
            "textDocument/hover" => {
                let Some(text) = self.text_for(&params) else {
                    return vec![response(id, serde_json::Value::Null)];
                };
                let (line, column) = lsp_position(&params);
                // `hover` works in 1-based line/column.
                match hover(&text, line + 1, column + 1) {
                    Some(description) => vec![response(
                        id,
                        serde_json::json!({
                            "contents": { "kind": "markdown", "value": format!("```nxl\n{description}\n```") }
                        }),
                    )],
                    None => vec![response(id, serde_json::Value::Null)],
                }
            }
            "textDocument/definition" => {
                let Some(text) = self.text_for(&params) else {
                    return vec![response(id, serde_json::Value::Null)];
                };
                let uri = document_uri(&params).unwrap_or_default();
                let (line, column) = lsp_position(&params);
                let offset = offset_of(&text, line + 1, column + 1);
                match offset
                    .and_then(|offset| identifier_at(&text, offset))
                    .and_then(|name| definition(&text, &name))
                {
                    Some((target_line, target_column)) => vec![response(
                        id,
                        serde_json::json!([{
                            "uri": uri,
                            "range": {
                                "start": { "line": target_line - 1, "character": target_column - 1 },
                                "end": { "line": target_line - 1, "character": target_column - 1 }
                            }
                        }]),
                    )],
                    None => vec![response(id, serde_json::Value::Null)],
                }
            }
            "textDocument/documentSymbol" => {
                let Some(text) = self.text_for(&params) else {
                    return vec![response(id, serde_json::json!([]))];
                };
                let items = document_symbols(&text);
                vec![response(id, serde_json::json!(items))]
            }
            "textDocument/references" => {
                let Some(text) = self.text_for(&params) else {
                    return vec![response(id, serde_json::json!([]))];
                };
                let uri = document_uri(&params).unwrap_or_default();
                let (line, column) = lsp_position(&params);
                let locations = offset_of(&text, line + 1, column + 1)
                    .and_then(|offset| identifier_at(&text, offset))
                    .map(|name| {
                        references(&text, &name)
                            .into_iter()
                            .map(|(found_line, found_column)| {
                                serde_json::json!({
                                    "uri": uri,
                                    "range": {
                                        "start": { "line": found_line - 1, "character": found_column - 1 },
                                        "end": { "line": found_line - 1, "character": found_column - 1 }
                                    }
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                vec![response(id, serde_json::json!(locations))]
            }
            "$/cancelRequest" | "$/setTrace" => Vec::new(),
            other => match id {
                // A notification we do not implement must not be answered.
                None => Vec::new(),
                Some(id) => vec![error_response(
                    id,
                    -32601,
                    &format!("method not found: {other}"),
                )],
            },
        }
    }

    /// The document text a request refers to, if the client has it open.
    fn text_for(&self, params: &serde_json::Value) -> Option<String> {
        let uri = document_uri(params)?;
        self.documents
            .get(&uri)
            .map(|document| document.text.clone())
    }

    /// Compile the document and publish what the compiler says.
    fn diagnostics_for(&mut self, uri: &str) -> Vec<String> {
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        let items = published_diagnostics(&document.text);
        vec![notification(
            "textDocument/publishDiagnostics",
            serde_json::json!({ "uri": uri, "diagnostics": items }),
        )]
    }
}

fn document_uri(params: &serde_json::Value) -> Option<String> {
    params
        .pointer("/textDocument/uri")
        .and_then(|uri| uri.as_str())
        .map(str::to_owned)
}

/// The 0-based line/character a request points at.
///
/// LSP counts characters in UTF-16 units while the compiler counts characters,
/// which agrees for every code point in the Basic Multilingual Plane and
/// differs only for astral characters such as emoji.
fn lsp_position(params: &serde_json::Value) -> (usize, usize) {
    let line = params
        .pointer("/position/line")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;
    let character = params
        .pointer("/position/character")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;
    (line, character)
}

/// Diagnostics in the shape an editor expects, with the real error position.
pub fn published_diagnostics(source_text: &str) -> Vec<serde_json::Value> {
    let file = SourceFile::new("buffer", source_text);
    match crate::compile_to_nir(&file) {
        Ok(_) => Vec::new(),
        Err(diagnostic) => {
            let (line, column) = file.line_column(diagnostic.span.start);
            vec![serde_json::json!({
                "range": {
                    "start": { "line": line - 1, "character": column - 1 },
                    "end": { "line": line - 1, "character": column - 1 }
                },
                "severity": 1,
                "source": "nxlc",
                "message": diagnostic.message,
            })]
        }
    }
}

/// Document symbols with the ranges an outline view needs.
pub fn document_symbols(source_text: &str) -> Vec<serde_json::Value> {
    collect_symbols(source_text)
        .into_iter()
        .map(|symbol| {
            let start = position_of(source_text, symbol.span.start);
            let end = position_of(source_text, symbol.span.end.max(symbol.span.start));
            serde_json::json!({
                "name": symbol.name,
                // SymbolKind: Function 12, Variable 13, Struct 23, Class 5,
                // Interface 11, Enum 10, Method 6, Module 2.
                "kind": symbol_kind_number(symbol.kind),
                "range": { "start": start, "end": end },
                "selectionRange": { "start": start, "end": end },
            })
        })
        .collect()
}

fn symbol_kind_number(kind: &str) -> u32 {
    match kind {
        "function" => 12,
        "variable" => 13,
        "struct" => 23,
        "class" => 5,
        "actor" => 5,
        "interface" => 11,
        "trait" => 11,
        "enum" => 10,
        "alias" => 13,
        "module" => 2,
        _ => 13,
    }
}

/// A byte offset as a 0-based LSP position.
fn position_of(source_text: &str, offset: usize) -> serde_json::Value {
    let mut offset = offset.min(source_text.len());
    while offset > 0 && !source_text.is_char_boundary(offset) {
        offset -= 1;
    }
    let prefix = &source_text[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let character = prefix
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.chars().count());
    serde_json::json!({ "line": line, "character": character })
}

fn response(id: Option<serde_json::Value>, result: serde_json::Value) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(serde_json::Value::Null),
        "result": result,
    })
    .to_string()
}

fn error_response(id: serde_json::Value, code: i64, message: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
    .to_string()
}

fn notification(method: &str, params: serde_json::Value) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
    .to_string()
}

/// Run a stdio JSON-RPC loop reading Content-Length framed messages.
pub fn serve_stdio() -> std::io::Result<()> {
    use std::io::{BufRead, BufReader, Read, Write};
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout();
    let mut server = Server::new();
    loop {
        let mut content_length = 0usize;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header)? == 0 {
                return Ok(());
            }
            let trimmed = header.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = trimmed.strip_prefix("Content-Length: ") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
        if content_length == 0 {
            continue;
        }
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;
        let Ok(text) = String::from_utf8(body) else {
            continue;
        };
        for message in server.handle(&text) {
            write!(
                stdout,
                "Content-Length: {}\r\n\r\n{}",
                message.len(),
                message
            )?;
        }
        stdout.flush()?;
        if server.should_exit() {
            return Ok(());
        }
    }
}
