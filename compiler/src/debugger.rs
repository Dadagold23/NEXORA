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
//! | Stack frames | IMPLEMENTED (named frames with per-frame lines in session snapshots) |
//! | Variables view | IMPLEMENTED (named locals per frame at a paused stop) |
//! | Watch expressions | NOT IMPLEMENTED |
//! | Runtime inspection | IMPLEMENTED (output at the pause point, output/return on completion) |
//! | Step into / over / out | IMPLEMENTED at session level via deterministic replay (see `DebugSession`); resumable ip-level execution is NOT IMPLEMENTED |
//! | `nxl debug <file>` CLI | IMPLEMENTED (trace report; `--inspect`/`--steps` session view) |
//!
//! # Stepping model
//!
//! `DebugSession` re-runs the compiled program from the start on every
//! command, under a `vm::StepPolicy` that names the exact statement to stop
//! at next and marks everything the session has already visited as
//! un-stoppable. Programs are deterministic, so every replay reaches the
//! same stop with exact locals and output; the cost is one re-execution per
//! command. The VM's recursive interpreter is left untouched — no
//! continuation capture, no ip-level suspension.

use std::collections::BTreeSet;

use crate::diagnostic::Diagnostic;
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

// ── Stepping sessions (variables + step) ──────────────────────────────────────

/// Why a session ended without pausing.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEnd {
    /// The program ran to completion from the current position.
    Completed {
        /// Output produced by the run that completed.
        output: Vec<String>,
        /// Final return value (string form).
        return_value: String,
    },
    /// The program raised an uncaught error.
    Failed(String),
}

/// An interactive stepping session over a compiled program.
///
/// Each command replays the program from the start under a policy that
/// stops at the requested position (see the module docs for the model).
/// Sessions track the last stop's statement ordinal and frame depth so the
/// next command can name its target exactly; once the program completes or
/// fails, the session stays ended and repeats that outcome.
pub struct DebugSession {
    /// Compiled once at session construction; every replay runs this.
    program: vm::CompiledProgram,
    source_text: String,
    breakpoints: BTreeSet<usize>,
    /// Statement ordinal and frame depth of the last stop.
    position: Option<(u64, usize)>,
    /// Set once the program has completed or failed.
    ended: Option<SessionEnd>,
    /// Snapshot of the last stop (for a final variables view).
    last: Option<vm::StepStop>,
}

impl DebugSession {
    /// Compile `source_text` and open a session with the given 1-based
    /// breakpoint lines. Compile errors are returned as diagnostics.
    pub fn new(source_text: &str, breakpoints: &[usize]) -> Result<Self, Diagnostic> {
        let sf = SourceFile::new("debug_target.nxl", source_text);
        let module = crate::compile_to_nir(&sf)?;
        let program = vm::compile(&module)?;
        Ok(Self {
            program,
            source_text: source_text.to_owned(),
            breakpoints: breakpoints.iter().copied().collect(),
            position: None,
            ended: None,
            last: None,
        })
    }

    /// Run to the first stop: the first breakpoint hit, or the program's
    /// first statement when no breakpoints are set.
    pub fn start(&mut self) -> Result<vm::StepStop, SessionEnd> {
        if self.breakpoints.is_empty() {
            self.step()
        } else {
            self.continue_run()
        }
    }

    /// Step into: stop before the next statement, wherever it lives
    /// (descending into a called function if that is the next statement).
    pub fn step(&mut self) -> Result<vm::StepStop, SessionEnd> {
        let boundary = self.boundary();
        self.run_to(vm::StepPolicy {
            text: self.source_text.clone(),
            breakpoints: self.breakpoints.clone(),
            skip_before_statement: Some(boundary),
            stop_at_statement: Some(boundary),
            max_frame_depth: None,
        })
    }

    /// Step over: stop before the next statement in this frame or a
    /// shallower one, skipping the bodies of called functions.
    pub fn step_over(&mut self) -> Result<vm::StepStop, SessionEnd> {
        match self.position {
            Some((statement, depth)) => {
                let mut policy = self.base_policy(statement + 1);
                policy.max_frame_depth = Some(depth);
                self.run_to(policy)
            }
            None => self.step(),
        }
    }

    /// Step out: stop before the next statement in a shallower frame
    /// (running to completion when already in the entry frame).
    pub fn step_out(&mut self) -> Result<vm::StepStop, SessionEnd> {
        match self.position {
            Some((statement, depth)) => {
                let mut policy = self.base_policy(statement + 1);
                policy.max_frame_depth = Some(depth.saturating_sub(1));
                self.run_to(policy)
            }
            None => self.step(),
        }
    }

    /// Continue to the next breakpoint hit after the current position;
    /// runs to completion when no further hit occurs.
    pub fn continue_run(&mut self) -> Result<vm::StepStop, SessionEnd> {
        let boundary = self.boundary();
        self.run_to(self.base_policy(boundary))
    }

    /// The most recent paused snapshot, if the session has stopped.
    pub fn last(&self) -> Option<&vm::StepStop> {
        self.last.as_ref()
    }

    /// First statement ordinal the next command may stop at: everything
    /// below it has already been visited (breakpoint hits included).
    fn boundary(&self) -> u64 {
        self.position
            .map(|(statement, _)| statement + 1)
            .unwrap_or(0)
    }

    fn base_policy(&self, skip_before_statement: u64) -> vm::StepPolicy {
        vm::StepPolicy {
            text: self.source_text.clone(),
            breakpoints: self.breakpoints.clone(),
            skip_before_statement: Some(skip_before_statement),
            stop_at_statement: None,
            max_frame_depth: None,
        }
    }

    fn run_to(&mut self, policy: vm::StepPolicy) -> Result<vm::StepStop, SessionEnd> {
        if let Some(end) = &self.ended {
            return Err(end.clone());
        }
        match vm::execute_program_step(&self.program, policy) {
            vm::StepOutcome::Stopped(stop) => {
                self.position = Some((stop.statement, stop.frames.len()));
                self.last = Some(stop.clone());
                Ok(stop)
            }
            vm::StepOutcome::Completed {
                output,
                return_value,
            } => {
                let end = SessionEnd::Completed {
                    output,
                    return_value: return_value.to_string(),
                };
                self.position = None;
                self.ended = Some(end.clone());
                Err(end)
            }
            vm::StepOutcome::Failed(message) => {
                let end = SessionEnd::Failed(message);
                self.position = None;
                self.ended = Some(end.clone());
                Err(end)
            }
        }
    }
}

/// Format a paused snapshot: stop position, call stack (innermost first)
/// with each frame's named locals, and the output produced so far.
pub fn format_snapshot(stop: &vm::StepStop) -> String {
    let mut out = format!(
        "  paused at line {} (statement {})",
        stop.line, stop.statement
    );
    if stop.breakpoint_hit {
        out.push_str(" [breakpoint]");
    }
    out.push('\n');
    out.push_str("  call stack (innermost first):\n");
    for frame in stop.frames.iter().rev() {
        match frame.line {
            Some(line) => out.push_str(&format!("    {} at line {}\n", frame.function, line)),
            None => out.push_str(&format!("    {}\n", frame.function)),
        }
        for local in &frame.locals {
            out.push_str(&format!("      {} = {}\n", local.name, local.value));
        }
    }
    out.push_str(&format!("  output so far: {}\n", stop.output.len()));
    for line in &stop.output {
        out.push_str(&format!("    | {line}\n"));
    }
    out
}

/// Format one compact per-step line for the CLI.
pub fn format_step(stop: &vm::StepStop) -> String {
    let frame = stop.frames.last();
    let function = frame.map(|f| f.function.as_str()).unwrap_or("?");
    let locals = frame
        .map(|f| {
            f.locals
                .iter()
                .map(|l| format!("{} = {}", l.name, l.value))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    if locals.is_empty() {
        format!("  -> line {}, {function}\n", stop.line)
    } else {
        format!("  -> line {}, {function}: {locals}\n", stop.line)
    }
}

/// Format why a session ended without pausing.
pub fn format_end(end: &SessionEnd) -> String {
    match end {
        SessionEnd::Completed {
            output,
            return_value,
        } => {
            let mut out = format!("  program completed, return: {return_value}\n");
            out.push_str(&format!("  output lines: {}\n", output.len()));
            for line in output {
                out.push_str(&format!("    | {line}\n"));
            }
            out
        }
        SessionEnd::Failed(message) => format!("  program failed: {message}\n"),
    }
}
