pub mod ast;
pub mod backend;
pub mod cli;
pub mod concurrency;
pub mod debugger;
pub mod diagnostic;
pub mod ffi;
pub mod hir;
pub mod ide;
pub mod lexer;
pub mod ndb;
pub mod nir;
pub mod nls;
pub mod nuf;
pub mod nui;
pub mod parser;
pub mod perf;
pub mod pkg;
pub mod platforms;
pub mod project;
pub mod runtime;
pub mod security;
pub mod source;
pub mod stdlib;
pub mod vm;

use diagnostic::Diagnostic;
use nir::Module;
use parser::Parser;
use source::SourceFile;

pub fn parse_source(source: &SourceFile) -> Result<ast::Program, Diagnostic> {
    let tokens = lexer::lex(source)?;
    Parser::new(tokens).parse()
}

pub fn compile_to_nir(source: &SourceFile) -> Result<Module, Diagnostic> {
    let program = parse_source(source)?;
    let hir = hir::analyze(&program, source.name.clone())?;
    nir::lower_hir(&hir)
}
