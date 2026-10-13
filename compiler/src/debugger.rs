//! P21 — NXL Debugger
//!
//! Source-level debugging over the real VM. The VM already streams
//! `Opcode::DebugSpan` entries; this module maps those spans to source lines
//! and checks them against user-set breakpoints.
//!
//! # Status
//!
//! | Feature | Status |
//! | --- | --- |
//! | Breakpoints by source line | IMPLEMENTED |
//! | Statement trace (step trace from DebugSpan stream) | IMPLEMENTED |
//! | Stack frames | PARTIAL (frame names not exposed yet) |
//! | Variables view | NOT IMPLEMENTED (locals lack names post-lowering) |
//! | Watch expressions | NOT IMPLEMENTED |
//! | Runtime inspection | PARTIAL (output/return value available) |
//! | True step over/into/out at VM-ip level | NOT IMPLEMENTED |
//! | `nxl debug <file>` CLI | IMPLEMENTED (breakpoint hit-count + trace lines) |

use std::collections::BTreeSet;

use crate::source::SourceFile;
use crate::vm::{self, VmResult};

/// Map `span` to a 1-based source line.
pub fn line_of(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1
}

#[derive(Debug, Default)]
pub struct DebugReport {
    /// Source lines executed, in order (consecutive duplicates removed).
    pub trace: Vec<usize>,
    /// Lines executed at least once.
    pub lines_hit: BTreeSet<usize>,
    /// Breakpoints that were hit, in execution order.
    pub breakpoint_hits: Vec<usize>,
    /// Program output lines (from `print()` calls).
    pub output: Vec<String>,
    /// Final return value (string form) or compile error.
    pub return_value: String,
}

#[derive(Default)]
pub struct Debugger {
    breakpoints: BTreeSet<usize>,
}

impl Debugger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_breakpoint(&mut self, line: usize) -> &mut Self {
        self.breakpoints.insert(line);
        self
    }

    pub fn remove_breakpoint(&mut self, line: usize) -> &mut Self {
        self.breakpoints.remove(&line);
        self
    }

    /// Execute the program under tracing and produce a debugging report.
    pub fn run(&self, source_text: &str) -> DebugReport {
        let sf = SourceFile::new("debug_target.nxl", source_text);
        match vm::run(&sf) {
            Ok(VmResult {
                output,
                return_value,
                spans,
            }) => {
                let mut trace = Vec::new();
                let mut hits = Vec::new();
                let mut last = 0usize;
                for span in &spans {
                    let line = line_of(source_text, span.start);
                    if line != last {
                        trace.push(line);
                        last = line;
                    }
                    if self.breakpoints.contains(&line) {
                        hits.push(line);
                    }
                }
                let lines_hit = trace.iter().copied().collect();
                DebugReport {
                    trace,
                    lines_hit,
                    breakpoint_hits: hits,
                    output,
                    return_value: return_value.to_string(),
                }
            }
            Err(d) => DebugReport {
                return_value: format!("error: {}", d.message),
                ..Default::default()
            },
        }
    }
}

/// Format a debug report for the CLI.
pub fn format_report(report: &DebugReport) -> String {
    let mut out = String::new();
    out.push_str("nxl debug\n");
    out.push_str(&format!("  return: {}\n", report.return_value));
    out.push_str(&format!("  output lines: {}\n", report.output.len()));
    for line in &report.output {
        out.push_str(&format!("    | {line}\n"));
    }
    out.push_str(&format!(
        "  statement trace lines: {}\n",
        report.trace.len()
    ));
    if !report.breakpoint_hits.is_empty() {
        out.push_str("  breakpoint hits (in execution order):\n");
        for l in &report.breakpoint_hits {
            out.push_str(&format!("    -> line {l}\n"));
        }
    } else {
        out.push_str("  no breakpoints hit\n");
    }
    out.push_str(&format!(
        "  lines executed: {}\n",
        report
            .lines_hit
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out
}
