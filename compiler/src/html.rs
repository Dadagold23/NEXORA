//! NXL and markup: output escaping, and the `.nxlhtml` template format.
//!
//! # Why a separate format
//!
//! `.nxl` stays what it has always been: a NXL source file that prints. That is
//! enough to build a whole page — `public/index.nxl` in the web template does
//! exactly that — and it needs no grammar change at all.
//!
//! Mixed markup-and-code is a different job, so it gets a different extension,
//! the way PHP uses `.phtml` and Ruby uses `.erb`. A template is
//!
//! ```text
//! <h1>Hello</h1>
//! <nxl>
//!     for feature in features() {
//!         print("<li>" + escape_html(feature) + "</li>");
//!     }
//! </nxl>
//! <p><nxl expr="app_tagline()" /></p>
//! ```
//!
//! Two forms, both of which are valid markup so HTML tooling leaves them alone:
//!
//! - `<nxl> … </nxl>` — a statement block. Its NXL runs in place; anything it
//!   prints becomes part of the page.
//! - `<nxl expr="…" />` — one expression. Its value is HTML-escaped before it
//!   is inserted, always.
//!
//! # Quoting
//!
//! An expression is read up to its first matching quote, so a NXL string
//! literal inside `expr=` needs the other quote character:
//!
//! ```text
//! <nxl expr='items.len()' />
//! <nxl expr='"<b>" + name + "</b>"' />
//! ```
//!
//! # How it renders
//!
//! The whole template becomes one NXL program: markup is emitted as `print` of a
//! string literal, expressions become `print(escape_html(…))`, and blocks are
//! inlined. One compile, one run, and bindings declared in one block are visible
//! in the next — which is what makes loops over data worth writing.
//!
//! Nothing here executes text as a system command. The only thing that is ever
//! compiled is NXL source, through the ordinary compiler.

use crate::source::SourceFile;
use crate::vm;

/// Escape text so it can be placed inside HTML without becoming markup.
///
/// This is the security primitive the format is built on: `<nxl expr>` output is
/// escaped by definition, so a value that happens to contain a tag cannot
/// inject one.
pub fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Remove tags, leaving the text.
///
/// Useful for summaries and titles. It is not a sanitiser: it does not decode
/// entities, and it is not a defence against markup that arrives by another
/// route. Escape for output, strip for text.
pub fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for character in text.chars() {
        match character {
            '<' => inside = true,
            '>' if inside => inside = false,
            other if !inside => out.push(other),
            _ => {}
        }
    }
    out
}

/// One piece of a parsed template.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    /// Literal markup, passed through untouched.
    Markup(String),
    /// `<nxl> … </nxl>`: NXL statements run in place.
    Statements(String),
    /// `<nxl expr="…" />`: one expression, escaped on the way out.
    Expression(String),
    /// `<nxl raw="." />`: one binding, inserted as markup, unescaped.
    ///
    /// This exists for layouts, where what is being inserted is a page this
    /// project already rendered and already escaped. It is the one way to put a
    /// value into a page without escaping it, so it is spelled differently on
    /// purpose: `expr` is safe by default, `raw` is the deliberate opt-out.
    Raw(String),
}

/// A parsed template.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub segments: Vec<Segment>,
}

/// Why a template could not be parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateError {
    pub message: String,
    /// 1-based line the problem is on, when it can be worked out.
    pub line: usize,
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

const OPEN: &str = "<nxl";
const CLOSE: &str = "</nxl>";

impl Template {
    /// Parse a template.
    pub fn parse(text: &str) -> Result<Self, TemplateError> {
        let mut segments = Vec::new();
        // `cursor` is where the next `<nxl` search starts; `markup_start` is
        // where the run of literal text before it began. Keeping them apart is
        // what stops text that merely looks like a tag from losing its `<`.
        let mut cursor = 0usize;
        let mut markup_start = 0usize;

        while let Some(offset) = text[cursor..].find(OPEN) {
            let start = cursor + offset;
            // A tag is ours only if what follows `<nxl` is either `expr=` (the
            // expression form) or `>` (a block). Anything else is text that
            // happens to begin with those letters, such as `<nxltools>`.
            let after_open = text[start + OPEN.len()..].trim_start();
            let is_expression = after_open.starts_with("expr=");
            let is_raw = after_open.starts_with("raw=");
            let is_opening = after_open.starts_with('>');

            if is_expression || is_raw {
                let keyword = if is_expression { "expr" } else { "raw" };
                let after_attribute = after_open[keyword.len() + 1..].trim_start();
                let quote = after_attribute
                    .chars()
                    .next()
                    .filter(|c| *c == '"' || *c == '\'')
                    .ok_or_else(|| TemplateError {
                        message: format!(
                            "the `{keyword}=` value must be quoted, as in `<nxl {keyword}=\"name\" />`"
                        ),
                        line: line_of(text, start),
                    })?;
                let value_start = text.len() - after_attribute.len() + 1;
                let value_end = value_start
                    + text[value_start..]
                        .find(quote)
                        .ok_or_else(|| TemplateError {
                            message: format!("the `{keyword}=` value has no closing {quote}"),
                            line: line_of(text, start),
                        })?;
                let value = text[value_start..value_end].to_owned();
                if value.trim().is_empty() {
                    return Err(TemplateError {
                        message: format!("`{keyword}=` is empty"),
                        line: line_of(text, start),
                    });
                }
                let after_value = text[value_end + 1..].trim_start();
                if !after_value.starts_with("/>") {
                    return Err(TemplateError {
                        message: format!("an `<nxl {keyword}=… />` tag must end with `/>`"),
                        line: line_of(text, start),
                    });
                }
                if start > markup_start {
                    segments.push(Segment::Markup(text[markup_start..start].to_owned()));
                }
                segments.push(if is_raw {
                    Segment::Raw(value)
                } else {
                    Segment::Expression(value)
                });
                cursor = text.len() - after_value.len() + 2;
                markup_start = cursor;
                continue;
            }

            if !is_opening {
                // Something like `<nxlfoo`: not ours. Step past this `<` and
                // keep searching, leaving the text itself intact.
                cursor = start + 1;
                continue;
            }

            let body_start =
                start + OPEN.len() + (text.len() - start - OPEN.len() - after_open.len()) + 1;
            let end = text[body_start..]
                .find(CLOSE)
                .ok_or_else(|| TemplateError {
                    message: "unclosed `<nxl>`: expected `</nxl>`".to_owned(),
                    line: line_of(text, start),
                })?;
            let statements = &text[body_start..body_start + end];
            if statements.contains(OPEN) {
                return Err(TemplateError {
                    message: "`<nxl>` blocks cannot be nested".to_owned(),
                    line: line_of(text, start),
                });
            }
            if start > markup_start {
                segments.push(Segment::Markup(text[markup_start..start].to_owned()));
            }
            segments.push(Segment::Statements(statements.to_owned()));
            cursor = body_start + end + CLOSE.len();
            markup_start = cursor;
        }

        if markup_start < text.len() {
            let tail = &text[markup_start..];
            if tail.contains(CLOSE) {
                return Err(TemplateError {
                    message: "`</nxl>` without a matching `<nxl>`".to_owned(),
                    line: line_of(text, markup_start),
                });
            }
            segments.push(Segment::Markup(tail.to_owned()));
        }

        Ok(Self { segments })
    }

    /// The NXL program this template is.
    ///
    /// Markup becomes a printed string literal, an expression becomes a printed
    /// escaped value, and a block's statements are inlined between braces so
    /// its bindings stay its own.
    pub fn to_program(&self) -> String {
        let mut program = String::from("// generated from a .nxlhtml template\nfn main() {\n");
        for segment in &self.segments {
            match segment {
                Segment::Markup(markup) => {
                    program.push_str(&format!("    print({});\n", nxl_string_literal(markup)));
                }
                Segment::Expression(expression) => {
                    program.push_str(&format!("    print(escape_html({expression}));\n"));
                }
                Segment::Raw(binding) => {
                    // Deliberately not escaped: this is how a layout inserts a
                    // page it already rendered.
                    program.push_str(&format!("    print({binding});\n"));
                }
                Segment::Statements(statements) => {
                    program.push_str("    {\n");
                    for line in statements.lines() {
                        program.push_str("        ");
                        program.push_str(line);
                        program.push('\n');
                    }
                    program.push_str("    }\n");
                }
            }
        }
        program.push_str("}\n");
        program
    }

    /// Compile and run the template, returning the page.
    pub fn render(&self, name: &str) -> Result<String, TemplateError> {
        self.render_with(name, &[])
    }

    /// Compile and run the template with values already bound in scope.
    ///
    /// Each binding becomes a `let` before the template body, so a page can use
    /// `<nxl expr="id" />` for a path parameter or a submitted field without
    /// any of them having to travel through the template's text. Bindings are
    /// the *raw* values: escaping happens when the template prints them, so a
    /// binding is never a hole in the escaping.
    pub fn render_with(
        &self,
        name: &str,
        bindings: &[(&str, &str)],
    ) -> Result<String, TemplateError> {
        let mut program = String::new();
        for (variable, value) in bindings {
            if !is_binding_name(variable) {
                return Err(TemplateError {
                    message: format!("'{variable}' cannot be used as a template binding"),
                    line: 1,
                });
            }
            program.push_str(&format!(
                "let {variable} = {};\n",
                nxl_string_literal(value)
            ));
        }
        let body = self.to_program();
        // The bindings go *inside* the generated function, after its opening
        // line. Placing them before it would put them out of scope.
        let marker = "fn main() {\n";
        let source = match body.find(marker) {
            Some(at) => format!(
                "{}{}{}",
                &body[..at + marker.len()],
                program,
                &body[at + marker.len()..]
            ),
            None => format!("{program}\n{body}"),
        };
        let source = SourceFile::new(name, source);
        match crate::compile_to_nir(&source) {
            Ok(module) => match vm::execute(&module) {
                Ok(result) => Ok(result.output.join("\n")),
                Err(diagnostic) => Err(TemplateError {
                    message: format!("{} [{}]", diagnostic.message, diagnostic.code),
                    line: 1,
                }),
            },
            Err(diagnostic) => Err(TemplateError {
                message: format!("{} [{}]", diagnostic.message, diagnostic.code),
                line: 1,
            }),
        }
    }
}

/// Render a template from its text in one step.
pub fn render(text: &str, name: &str) -> Result<String, TemplateError> {
    Template::parse(text)?.render(name)
}

/// A binding name has to be something NXL will accept as an identifier, so a
/// hostile path segment cannot turn into source code.
fn is_binding_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Quote `text` as an NXL string literal.
///
/// Markup is trusted here — it is the template author's own text — so this only
/// has to produce a valid NXL literal, not safe output. Values that reach a
/// page through `<nxl expr>` go through `escape_html` instead.
fn nxl_string_literal(text: &str) -> String {
    let mut literal = String::from("\"");
    for character in text.chars() {
        match character {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\t' => literal.push_str("\\t"),
            other if (other as u32) < 0x20 => literal.push_str(&format!("\\u{:04x}", other as u32)),
            other => literal.push(other),
        }
    }
    literal.push('"');
    literal
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_ok(text: &str) -> String {
        render(text, "test.nxlhtml").expect("template should render")
    }

    #[test]
    fn escape_covers_every_dangerous_character() {
        assert_eq!(
            escape_html("<a href=\"x\">&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn escape_leaves_ordinary_text_alone() {
        assert_eq!(escape_html("Tom & Jerry"), "Tom &amp; Jerry");
        assert_eq!(escape_html("plain"), "plain");
    }

    #[test]
    fn strip_tags_leaves_text() {
        assert_eq!(strip_tags("<p>hi <b>there</b></p>"), "hi there");
        assert_eq!(strip_tags("no markup"), "no markup");
    }

    #[test]
    fn plain_markup_is_one_segment() {
        let template = Template::parse("<h1>Hi</h1>").unwrap();
        assert_eq!(
            template.segments,
            vec![Segment::Markup("<h1>Hi</h1>".into())]
        );
    }

    #[test]
    fn expression_form_is_recognised() {
        let template = Template::parse(r#"<p><nxl expr="name" /></p>"#).unwrap();
        assert_eq!(
            template.segments,
            vec![
                Segment::Markup("<p>".into()),
                Segment::Expression("name".into()),
                Segment::Markup("</p>".into()),
            ]
        );
    }

    #[test]
    fn block_form_is_recognised() {
        let template = Template::parse("<nxl>\n    let x = 1;\n</nxl>").unwrap();
        assert_eq!(template.segments.len(), 1);
        assert!(matches!(template.segments[0], Segment::Statements(_)));
    }

    #[test]
    fn markup_that_merely_starts_like_a_tag_is_left_alone() {
        let template = Template::parse("<nxlfoo>x</nxlfoo>").unwrap();
        assert_eq!(template.segments.len(), 1);
        assert!(matches!(template.segments[0], Segment::Markup(_)));
    }

    #[test]
    fn unclosed_block_is_an_error() {
        let error = Template::parse("<nxl>let x = 1;").unwrap_err();
        assert!(error.message.contains("unclosed"), "{}", error.message);
    }

    #[test]
    fn stray_close_is_an_error() {
        let error = Template::parse("hello</nxl>").unwrap_err();
        assert!(
            error.message.contains("without a matching"),
            "{}",
            error.message
        );
    }

    #[test]
    fn nested_block_is_an_error() {
        let error = Template::parse("<nxl><nxl>x</nxl></nxl>").unwrap_err();
        assert!(
            error.message.contains("cannot be nested"),
            "{}",
            error.message
        );
    }

    #[test]
    fn empty_expression_is_an_error() {
        let error = Template::parse(r#"<nxl expr="  " />"#).unwrap_err();
        assert!(error.message.contains("empty"), "{}", error.message);
    }

    #[test]
    fn unterminated_self_closing_tag_is_an_error() {
        let error = Template::parse(r#"<nxl expr="name">"#).unwrap_err();
        assert!(error.message.contains("/>"), "{}", error.message);
    }

    #[test]
    fn unquoted_expression_is_an_error() {
        let error = Template::parse("<nxl expr=name />").unwrap_err();
        assert!(error.message.contains("quoted"), "{}", error.message);
    }

    #[test]
    fn markup_renders_unchanged() {
        assert_eq!(render_ok("<h1>Hi</h1>"), "<h1>Hi</h1>");
    }

    #[test]
    fn expression_output_is_escaped() {
        // The attribute is single-quoted so the NXL string can hold its own
        // double quotes.
        let page = render_ok(r#"<nxl expr='"<script>alert(1)</script>"' />"#);
        assert!(!page.contains("<script"), "script tag survived: {page}");
        assert!(page.contains("&lt;script&gt;"), "{page}");
    }

    #[test]
    fn a_value_that_looks_like_markup_cannot_inject_it() {
        let page = render_ok(
            r#"<nxl>
            let comment = "<img src=x onerror=alert(1)>";
            </nxl><div><nxl expr="comment" /></div>"#,
        );
        assert!(!page.contains("<img"), "injection survived: {page}");
        assert!(page.contains("&lt;img"), "{page}");
    }

    #[test]
    fn blocks_can_build_repeated_markup() {
        let page = render_ok(
            "<nxl>\nlet items = [\"a\", \"b\", \"c\"];\nfor item in items {\n    print(\"<li>\" + escape_html(item) + \"</li>\");\n}\n</nxl>",
        );
        assert_eq!(page.matches("<li>").count(), 3, "{page}");
        assert!(page.contains("<li>a</li>"), "{page}");
    }

    #[test]
    fn markup_quotes_survive_the_trip_into_nxl() {
        let page = render_ok("<a href=\"/a?b=1&c=2\">go</a>");
        assert_eq!(page, "<a href=\"/a?b=1&c=2\">go</a>");
    }

    #[test]
    fn a_nxl_syntax_error_inside_a_block_surfaces() {
        let error = render("<nxl>let = ;</nxl>", "bad.nxlhtml").unwrap_err();
        assert!(!error.message.is_empty());
    }
}

#[cfg(test)]
mod parity_tests {
    use crate::backend::{compile_js, compile_native};
    use crate::source::SourceFile;
    use std::process::Command;

    const PROBE: &str = "print(escape_html(\"<b>a & b</b>\"));\nprint(strip_tags(\"<i>x</i>y\"));";

    fn vm_output() -> String {
        let source = SourceFile::new("probe.nxl", PROBE);
        crate::vm::run(&source)
            .expect("VM should run")
            .output
            .join("\n")
    }

    /// Run generated code with `node` or `rustc`.
    ///
    /// Returns `None` only when the tool is not installed at all. If the tool *is*
    /// there but refuses the generated code, that is a real failure and panics with
    /// the compiler's own message — a backend that emits broken code must never
    /// look like a skipped test.
    fn run_with(program: &str, tool: &str) -> Option<String> {
        let path = std::env::temp_dir().join(format!("nxl_escape_parity_{tool}"));
        let source_path = match tool {
            "js" => path.with_extension("js"),
            _ => path.with_extension("rs"),
        };
        std::fs::write(&source_path, program).expect("should write the generated program");

        let run = |binary: Option<std::path::PathBuf>| -> Option<std::process::Output> {
            let output = match binary {
                Some(binary) => Command::new(binary).output(),
                None => Command::new(tool).arg(&source_path).output(),
            };
            output.ok() // None means the tool is not installed
        };

        let binary = if tool == "js" {
            None
        } else {
            let exe = path.with_extension("exe");
            let compiled = Command::new("rustc")
                .arg("-A")
                .arg("warnings")
                .arg(&source_path)
                .arg("-o")
                .arg(&exe)
                .output();
            match compiled {
                Ok(result) => assert!(
                    result.status.success(),
                    "the generated Rust does not compile:\n{}",
                    String::from_utf8_lossy(&result.stderr)
                ),
                Err(_) => return None,
            }
            Some(exe)
        };

        let output = match run(binary) {
            Some(output) => output,
            None => {
                let _ = std::fs::remove_file(&source_path);
                return None;
            }
        };
        let _ = std::fs::remove_file(&source_path);
        assert!(
            output.status.success(),
            "the generated {} program exited with {:?}:\n{}",
            tool,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        Some(
            String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_owned(),
        )
    }

    #[test]
    fn the_vm_escapes() {
        assert_eq!(vm_output(), "&lt;b&gt;a &amp; b&lt;/b&gt;\nxy");
    }

    #[test]
    fn the_js_backend_escapes_identically() {
        let source = SourceFile::new("probe.nxl", PROBE);
        let artifact = compile_js(&source).expect("should compile to JS");
        match run_with(&artifact.source, "js") {
            Some(js_output) => assert_eq!(js_output, vm_output(), "JS output differs from the VM"),
            None => eprintln!("skipped: node is not available to run the generated JS"),
        }
    }

    #[test]
    fn the_rust_backend_escapes_identically() {
        let source = SourceFile::new("probe.nxl", PROBE);
        let artifact = compile_native(&source).expect("should compile to Rust");
        match run_with(&artifact.source, "rust") {
            Some(rust_output) => {
                assert_eq!(rust_output, vm_output(), "Rust output differs from the VM")
            }
            None => eprintln!("skipped: rustc is not available to build the generated Rust"),
        }
    }
}
