//! NXL on Android: the real compiler and the real VM, as a JNI library.
//!
//! # What this is
//!
//! An Android app cannot run `nxlc` — there is no shell, no filesystem layout
//! and no terminal. What it can load is a `.so`. So the language's own
//! front end and interpreter are compiled here into one:
//!
//! ```text
//!    NXL source  ──▶  lexer ─▶ parser ─▶ hir ─▶ nir ─▶ vm  ──▶  output
//! ```
//!
//! Those are the same source files the desktop compiler uses, reached through
//! `#[path]` rather than a dependency, so there is one implementation of NXL and
//! no second dialect to keep in step. They are reached this way because
//! `nxl-compiler` itself pulls in axum, tokio, SQLite, PostgreSQL and MySQL for
//! the server and database subsystems; none of that belongs in an APK, and none
//! of it is needed to run a program. Splitting the crate properly is the right
//! long-term fix and is noted as such in the docs.
//!
//! Every module below the JNI surface is std-only, which is what makes a
//! `cdylib` for `aarch64-linux-android` possible at all.
//!
//! # The JNI surface
//!
//! Two functions, declared here rather than generated:
//!
//! - `run(source) -> output` — compile and run a program, return what it printed.
//! - `compile_only(source) -> null|"ok"` — type-check without running.
//!
//! Errors come back as a string beginning `error[NXL…]:` so a caller can tell a
//! diagnostic from a result.

#![allow(non_snake_case)]

// ── The NXL language, from the desktop compiler ──────────────────────────────
//
// These are the compiler's own files. Do not edit them here; edit them in
// `compiler/src/` and they change here too.

#[path = "../../../compiler/src/ast.rs"]
pub mod ast;
#[path = "../../../compiler/src/backend.rs"]
pub mod backend;
#[path = "../../../compiler/src/diagnostic.rs"]
pub mod diagnostic;
#[path = "../../../compiler/src/hir.rs"]
pub mod hir;
#[path = "../../../compiler/src/html.rs"]
pub mod html;
#[path = "../../../compiler/src/lexer.rs"]
pub mod lexer;
#[path = "../../../compiler/src/nir.rs"]
pub mod nir;
#[path = "../../../compiler/src/parser.rs"]
pub mod parser;
#[path = "../../../compiler/src/project.rs"]
pub mod project;
#[path = "../../../compiler/src/source.rs"]
pub mod source;
#[path = "../../../compiler/src/stdlib.rs"]
pub mod stdlib;
#[path = "../../../compiler/src/vm.rs"]
pub mod vm;

use diagnostic::Diagnostic;
use nir::Module;
use parser::Parser;
use source::SourceFile;

/// Parse NXL source to its AST. `project.rs` calls this by crate path, exactly
/// as the desktop compiler's own `lib.rs` does.
pub fn parse_source(source: &SourceFile) -> Result<ast::Program, Diagnostic> {
    let tokens = lexer::lex(source)?;
    Parser::new(tokens).parse()
}

/// Compile NXL source to the VM's module form.
pub fn compile_to_nir(source: &SourceFile) -> Result<Module, Diagnostic> {
    let program = parse_source(source)?;
    let hir = hir::analyze(&program, source.name.clone())?;
    nir::lower_hir(&hir)
}

/// Compile and run, returning what the program printed.
pub fn run_source(name: &str, source: &str) -> Result<String, String> {
    let file = SourceFile::new(name, source.to_owned());
    let module = compile_to_nir(&file).map_err(describe)?;
    vm::execute(&module)
        .map(|result| result.output.join("\n"))
        .map_err(describe)
}

/// Type-check without running.
pub fn check_source(name: &str, source: &str) -> Result<(), String> {
    let file = SourceFile::new(name, source.to_owned());
    compile_to_nir(&file).map(|_| ()).map_err(describe)
}

fn describe(diagnostic: Diagnostic) -> String {
    format!("error[{}]: {}", diagnostic.code, diagnostic.message)
}

// ── JNI ──────────────────────────────────────────────────────────────────────
//
// No `jni` crate: it would pull a dependency into an APK for four functions, and
// the ABI here is small enough to declare by hand. The signatures must match
// the NDK's `jni.h`, which is fixed by the platform rather than by us.

/// Opaque handle to a Java object, as the NDK defines it.
#[repr(C)]
pub struct JObject {
    _private: [u8; 0],
}

/// `jstring` is a Java string reference.
pub type JString = *mut JObject;

/// Opaque `JNIEnv*`. Declared opaque because only the function-pointer
/// signatures below are ever used through it.
#[repr(C)]
pub struct JNIEnv {
    _private: [u8; 0],
}

type GetStringUTFChars = unsafe extern "system" fn(
    *mut JNIEnv,
    JString,
    *mut i8,
) -> *const std::os::raw::c_char;

type ReleaseStringUTFChars =
    unsafe extern "system" fn(*mut JNIEnv, JString, *const std::os::raw::c_char);

type NewStringUTF =
    unsafe extern "system" fn(*mut JNIEnv, *const std::os::raw::c_char) -> JString;

impl JNIEnv {
    // The JNINativeInterface table begins with these slots, in this order.
    // Reaching them by offset is the documented way to call JNI without a
    // binding library.
    unsafe fn table(&mut self) -> *const *const std::os::raw::c_void {
        self as *mut JNIEnv as *const *const std::os::raw::c_void
    }

    unsafe fn get_string_utf_chars(
        &mut self,
        string: JString,
    ) -> Option<(*mut i8, *const std::os::raw::c_char)> {
        let get = std::mem::transmute::<
            *const std::os::raw::c_void,
            GetStringUTFChars,
        >(*self.table().offset(15));
        let copy = get(self, string, std::ptr::null_mut());
        if copy.is_null() {
            None
        } else {
            Some((copy as *mut i8, copy))
        }
    }

    unsafe fn release_string_utf_chars(
        &mut self,
        string: JString,
        chars: *const std::os::raw::c_char,
    ) {
        let release = std::mem::transmute::<
            *const std::os::raw::c_void,
            ReleaseStringUTFChars,
        >(*self.table().offset(16));
        release(self, string, chars);
    }

    unsafe fn new_string_utf(&mut self, text: &str) -> JString {
        let new_string = std::mem::transmute::<
            *const std::os::raw::c_void,
            NewStringUTF,
        >(*self.table().offset(167));
        let mut buffer: Vec<std::os::raw::c_char> =
            text.as_bytes().iter().map(|b| *b as std::os::raw::c_char).collect();
        buffer.push(0);
        new_string(self, buffer.as_ptr())
    }
}

/// Run a NXL program and return everything it printed.
///
/// # Safety
/// Called by the JVM with valid `JNIEnv` and `jstring` arguments.
#[no_mangle]
pub unsafe extern "system" fn Java_com_nexora_nxl_NxlRuntime_run<'local>(
    env: *mut JNIEnv,
    _class: JString,
    name: JString,
    source: JString,
) -> JString {
    let env = &mut *env;
    let name = match env.get_string_utf_chars(name) {
        Some((_, c)) => std::ffi::CStr::from_ptr(c).to_string_lossy().into_owned(),
        None => return std::ptr::null_mut(),
    };
    let text = match env.get_string_utf_chars(source) {
        Some((_, c)) => {
            let owned = std::ffi::CStr::from_ptr(c).to_string_lossy().into_owned();
            env.release_string_utf_chars(source, c);
            owned
        }
        None => return std::ptr::null_mut(),
    };

    let outcome = match run_source(&name, &text) {
        Ok(output) => output,
        Err(message) => message,
    };
    env.new_string_utf(&outcome)
}

/// Type-check a NXL program without running it. Returns `"ok"` or a diagnostic.
///
/// # Safety
/// Called by the JVM with valid `JNIEnv` and `jstring` arguments.
#[no_mangle]
pub unsafe extern "system" fn Java_com_nexora_nxl_NxlRuntime_check<'local>(
    env: *mut JNIEnv,
    _class: JString,
    name: JString,
    source: JString,
) -> JString {
    let env = &mut *env;
    let name = match env.get_string_utf_chars(name) {
        Some((_, c)) => std::ffi::CStr::from_ptr(c).to_string_lossy().into_owned(),
        None => return std::ptr::null_mut(),
    };
    let text = match env.get_string_utf_chars(source) {
        Some((_, c)) => {
            let owned = std::ffi::CStr::from_ptr(c).to_string_lossy().into_owned();
            env.release_string_utf_chars(source, c);
            owned
        }
        None => return std::ptr::null_mut(),
    };

    let outcome = match check_source(&name, &text) {
        Ok(()) => "ok".to_owned(),
        Err(message) => message,
    };
    env.new_string_utf(&outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Android app depends on this crate being the real NXL engine, so the
    /// same language the desktop VM runs is exercised here directly.
    #[test]
    fn it_runs_a_straight_line_program() {
        let out = run_source("t.nxl", r#"print("hello from nxl");"#).unwrap();
        assert_eq!(out, "hello from nxl");
    }

    #[test]
    fn it_runs_a_loop_and_a_branch() {
        // Neither of these is reachable through the Rust backend; they run here
        // because this crate embeds the VM rather than generating native code.
        let program = r#"
let names = ["ada", "grace"];
for name in names {
    if name == "ada" {
        print("first: " + name);
    } else {
        print("other: " + name);
    }
}
"#;
        let out = run_source("t.nxl", program).unwrap();
        assert!(out.contains("first: ada"), "{out}");
        assert!(out.contains("other: grace"), "{out}");
    }

    #[test]
    fn it_escapes_html_the_same_way_the_desktop_vm_does() {
        let out = run_source("t.nxl", r#"print(escape_html("<b>x</b>"));"#).unwrap();
        assert_eq!(out, "&lt;b&gt;x&lt;/b&gt;");
    }

    #[test]
    fn it_reports_a_diagnostic_rather_than_throwing() {
        let error = run_source("bad.nxl", "let = ;").unwrap_err();
        assert!(error.starts_with("error[NXL"), "{error}");
    }

    #[test]
    fn it_type_checks_without_running() {
        assert_eq!(check_source("t.nxl", r#"print("x");"#).unwrap(), ());
        assert!(check_source("bad.nxl", "let = ;").is_err());
    }
}