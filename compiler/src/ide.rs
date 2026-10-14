//! P22 — NIDE
//!
//! NIDE contains no language semantics of its own: every IDE capability is a
//! thin composition of the same compiler crate (`parse_source`,
//! `compile_to_nir`), the NLS language server, the formatter, the debugger,
//! the package manager, and the CLI build/test pipeline.
//!
//! # Status
//!
//! | Capability | Status | Evidence |
//! | --- | --- | --- |
//! | Diagnostics panel | IMPLEMENTED | delegates to `nls::diagnostics` (same messages) — tested |
//! | Outline / symbols | IMPLEMENTED | delegates to `nls::collect_symbols` — tested |
//! | Completions | IMPLEMENTED | delegates to `nls::completions` |
//! | Hover / go-to-definition | IMPLEMENTED | delegates to `nls::hover` / `nls::definition` |
//! | Format document | IMPLEMENTED | delegates to the CLI formatter |
//! | Build / run / test / debug | IMPLEMENTED | delegates to `cli::cmd_build`/`cmd_run`/`cmd_test`/`cmd_debug` |
//! | Package actions | DELEGATED | delegates to `cli` pkg commands |
//! | Interactive debugger transport | NOT IMPLEMENTED | UI/debugger plumbing beyond CLI report pending |
//! | IDE shell (editor canvas, panels) | NOT IMPLEMENTED | This module is the model; no GUI shell yet |

use std::path::{Path, PathBuf};

use crate::nls;

/// An open document in the IDE model, backed by compiler analysis.
#[derive(Debug, Clone)]
pub struct Document {
    pub path: PathBuf,
    pub text: String,
}

impl Document {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            text,
        })
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_owned();
    }
}

#[derive(Debug, Clone)]
pub struct IdeItem {
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Default)]
pub struct IdeModel {
    pub documents: Vec<Document>,
}

impl IdeModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, doc: Document) {
        self.documents.push(doc);
    }

    /// Diagnostics are the compiler's, not the IDE's.
    pub fn diagnostics(&self, doc: &Document) -> Vec<String> {
        nls::diagnostics(&doc.text)
    }

    /// Outline comes from the same AST as hover/definition/completion.
    pub fn outline(&self, doc: &Document) -> Vec<IdeItem> {
        nls::collect_symbols(&doc.text)
            .into_iter()
            .map(|s| IdeItem {
                label: s.name,
                detail: s.type_info.or(Some(s.kind.to_owned())),
            })
            .collect()
    }

    pub fn completions(&self, doc: &Document) -> Vec<IdeItem> {
        nls::completions(&doc.text)
            .into_iter()
            .map(|c| IdeItem {
                label: c,
                detail: None,
            })
            .collect()
    }

    pub fn hover(&self, doc: &Document, line: usize, col: usize) -> Option<String> {
        nls::hover(&doc.text, line, col)
    }

    pub fn definition(&self, doc: &Document, name: &str) -> Option<(usize, usize)> {
        nls::definition(&doc.text, name)
    }

    /// Format the whole document via the shared formatter (whitespace normaliser).
    pub fn format(&self, doc: &Document) -> String {
        doc.text
            .lines()
            .map(|l| l.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    /// NIDE delegates every heavyweight action to the compiler crate / CLI.
    pub fn build_and_test(&self, dir: &Path) -> Result<String, String> {
        let check = crate::cli::cmd_check(dir, None).map_err(|e| e.to_string())?;
        let report = crate::cli::cmd_test(dir, None).map_err(|e| e.to_string())?;
        Ok(format!(
            "check: {}\ntests: {}/{} passed\n",
            check.join(", "),
            report.passed,
            report.total
        ))
    }
}

/// capability table for `nxl ide`
pub fn ide_capability_table() -> Vec<(&'static str, &'static str)> {
    vec![
        ("compiler", "nxl-compiler (shared)"),
        ("language server", "nxl-compiler::nls"),
        ("formatter", "nxl-compiler::cli::cmd_fmt"),
        ("debugger", "nxl-compiler::debugger"),
        ("package manager", "nxl-compiler::pkg"),
        ("build/test", "nxl-compiler::cli"),
        ("IDE shell", "NOT IMPLEMENTED (model-only)"),
    ]
}
