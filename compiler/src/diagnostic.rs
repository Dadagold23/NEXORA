use crate::source::{SourceFile, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Hint,
}

impl Severity {
    /// The lower-case name used in rendered and machine-readable output.
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
            Severity::Hint => "hint",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            span,
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn render(&self, source: &SourceFile) -> String {
        let (line, column) = source.line_column(self.span.start);
        let source_line = source
            .text
            .lines()
            .nth(line.saturating_sub(1))
            .unwrap_or("");
        let caret_width = source
            .text
            .get(self.span.start..self.span.end)
            .map_or(1, |text| text.chars().count().max(1));
        let label = self.severity.as_str();
        let mut rendered = format!(
            "{label}[{}]: {}\n  --> {}:{line}:{column}\n   |\n{line:>3} | {source_line}\n   | {}{}",
            self.code,
            self.message,
            source.name,
            " ".repeat(column.saturating_sub(1)),
            "^".repeat(caret_width)
        );
        if let Some(help) = &self.help {
            rendered.push_str(&format!("\n   = help: {help}"));
        }
        rendered
    }
}
