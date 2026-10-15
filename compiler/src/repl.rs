//! Interactive REPL for NXL.
//!
//! Reads expressions and statements from an input stream, evaluates each one,
//! and prints the result. Input is accumulated until it parses, so multi-line
//! blocks work naturally. Errors are reported and the session continues.

use crate::diagnostic::Diagnostic;
use crate::source::SourceFile;
use std::io::{BufRead, Write};

/// Outcome of feeding one chunk of input to the REPL.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplOutcome {
    /// Lines written to the output stream.
    pub output: Vec<String>,
    /// The session ended (`.exit`, `.quit`, or end of input).
    pub finished: bool,
}

/// Settings that are not read from the program text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplConfig {
    pub echo_prompts: bool,
}

const HELP: &str = "\
NXL REPL. Type an expression or statements and press enter.
  .help            show this text
  .exit, .quit     leave the REPL
  .clear           forget the accumulated buffer
  Statements and blocks may span several lines; input runs when it parses.";

/// Run a REPL session over `input`, writing prompts and results to `output`.
pub fn run(
    input: &mut impl BufRead,
    output: &mut impl Write,
    config: &ReplConfig,
) -> std::io::Result<ReplOutcome> {
    let mut buffer = String::new();
    let mut session: Vec<String> = Vec::new();
    let mut finished = false;

    loop {
        if config.echo_prompts {
            write!(
                output,
                "{}",
                if buffer.is_empty() { ">>> " } else { "... " }
            )?;
            output.flush()?;
        }
        let mut line = String::new();
        let read = input.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let trimmed = line.trim();

        match trimmed {
            "" => {
                // A blank line runs whatever is buffered.
                if !buffer.trim().is_empty() {
                    lines::eval(&buffer, &mut session, output)?;
                }
                buffer.clear();
            }
            ".exit" | ".quit" => {
                if !buffer.trim().is_empty() {
                    lines::eval(&buffer, &mut session, output)?;
                }
                buffer.clear();
                finished = true;
                break;
            }
            ".clear" => {
                buffer.clear();
                session.clear();
                writeln!(output, "session state cleared")?;
            }
            ".help" => {
                writeln!(output, "{HELP}")?;
            }
            other => {
                buffer.push_str(other);
                buffer.push('\n');
                // Run as soon as the input forms a complete program, so an
                // interactive session shows results as they are typed.
                if !is_incomplete(&buffer) {
                    lines::eval(&buffer, &mut session, output)?;
                    buffer.clear();
                }
            }
        }
    }

    // Anything left when the input ends (EOF) still runs.
    if !buffer.trim().is_empty() {
        lines::eval(&buffer, &mut session, output)?;
    }

    Ok(ReplOutcome {
        output: Vec::new(),
        finished,
    })
}

mod lines {
    use super::*;

    /// Evaluate one input against the session, printing output, the resulting
    /// value, or a diagnostic.
    ///
    /// The session is replayed on every evaluation: declarations and
    /// side-effecting statements are re-run so that later inputs observe their
    /// effects. Purely printing inputs are not replayed, so their output is
    /// not repeated.
    pub fn eval(
        source: &str,
        session: &mut Vec<String>,
        output: &mut impl Write,
    ) -> std::io::Result<()> {
        let source = source.trim();
        if source.is_empty() {
            return Ok(());
        }

        let kind = classify(source);
        let program = match kind {
            Input::Declaration | Input::Statement => {
                let mut all = replayable(session);
                all.push(source.to_owned());
                let text = program_text(&all, None);
                session.push(source.to_owned());
                text
            }
            Input::Expression => program_text(&replayable(session), Some(source)),
        };

        let file = SourceFile::new("repl.nxl", program.clone());
        let module = match crate::compile_to_nir(&file) {
            Ok(module) => module,
            Err(diagnostic) => {
                // Report against the user's own text, not the replayed program.
                let user_file = SourceFile::new("repl.nxl", source);
                writeln!(output, "error: {}", render(&diagnostic, &user_file))?;
                return Ok(());
            }
        };

        let result = match crate::vm::execute(&module) {
            Ok(result) => result,
            Err(diagnostic) => {
                writeln!(output, "error: {}", render(&diagnostic, &file))?;
                return Ok(());
            }
        };
        for line in &result.output {
            writeln!(output, "{line}")?;
        }
        if result.output.is_empty() {
            let value = result.return_value;
            if !matches!(value, crate::vm::Value::Unit) {
                writeln!(output, "{value}")?;
            }
        }
        Ok(())
    }

    /// What kind of input this is.
    enum Input {
        Declaration,
        Statement,
        Expression,
    }

    fn classify(source: &str) -> Input {
        const DECLARATIONS: [&str; 10] = [
            "let ",
            "fn ",
            "struct ",
            "class ",
            "enum ",
            "impl ",
            "interface ",
            "actor ",
            "module ",
            "import ",
        ];
        const STATEMENTS: [&str; 9] = [
            "print(", "for ", "while ", "if ", "match ", "else ", "return ", "throw ", "try ",
        ];
        let trimmed = source.trim_start();
        if DECLARATIONS.iter().any(|k| trimmed.starts_with(k)) {
            return Input::Declaration;
        }
        // Control flow and printing run for their effect; a call like
        // `twice(21)` is an expression whose value is echoed.
        if STATEMENTS.iter().any(|k| trimmed.starts_with(k)) {
            return Input::Statement;
        }
        Input::Expression
    }

    /// Session entries that should be replayed: everything except inputs that
    /// only printed.
    fn replayable(session: &[String]) -> Vec<String> {
        session
            .iter()
            .filter(|entry| !entry.trim_start().starts_with("print("))
            .cloned()
            .collect()
    }

    fn program_text(entries: &[String], expression: Option<&str>) -> String {
        let mut program = entries.join("\n");
        // Everything runs at the top level so that bindings from earlier
        // inputs stay in scope; wrapping in `main` would hide them.
        if let Some(expression) = expression {
            program.push_str(&format!(
                "\nlet __nxl_repl_value = ({expression}); print(__nxl_repl_value);"
            ));
        }
        program
    }

    fn render(diagnostic: &Diagnostic, file: &SourceFile) -> String {
        let (line, column) = file.line_column(diagnostic.span.start);
        format!(
            "{}[{:?}] at line {line}, column {column}: {}",
            diagnostic.code, diagnostic.severity, diagnostic.message
        )
    }
}

/// True while the buffer is still incomplete, so the REPL keeps reading.
///
/// Determined by brace balance rather than by guessing at parser messages:
/// NXL blocks are brace-delimited, and a guess misfires on complete
/// single-line blocks like `for i in 4 { ... }`.
fn is_incomplete(source: &str) -> bool {
    let mut depth: i32 = 0;
    let mut chars = source.chars().peekable();
    let mut in_string = false;
    let mut in_line_comment = false;

    while let Some(c) = chars.next() {
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_string {
            match c {
                '\\' => {
                    chars.next();
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '/' if chars.peek() == Some(&'/') => in_line_comment = true,
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth > 0 || in_string
}

/// Entry point used by the CLI: reads stdin and writes stdout.
pub fn cmd_repl() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let config = ReplConfig { echo_prompts: true };
    run(&mut input, &mut output, &config)?;
    Ok(())
}
