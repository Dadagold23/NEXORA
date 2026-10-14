//! Phase 4 — multi-file NXL projects.
//!
//! Maps dotted module paths to files, loads transitive imports in dependency
//! order, and renames top-level items so independently authored files share
//! one lowering unit without collisions.
//!
//! Conventions (documented in `docs/04-language-reference.md`):
//! - `importPath` segments map to directories under the entry file's parent:
//!   `app.math` next to `src/main.nxl` lives in `src/app/math.nxl`.
//! - Only the named module file loads; there are no package init files.
//! - The entry file keeps the root namespace (`""`); every other module
//!   `p` owns `p::Name` for its top-level functions and types (`#` stays
//!   reserved for methods: `Type#method`). Top-level `let` bindings stay
//!   shared (a collision is NXL2005, like one file).
//! - Missing module files are tolerated: the import binds nothing and the
//!   use site fails with NXL2002, exactly like single-file mode.
//! - `module` declarations are informational only in this phase.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::{Program, Statement};
use crate::diagnostic::Diagnostic;
use crate::source::{SourceFile, Span};

/// One loaded source file: `path` is dotted (`""` for the entry file).
#[derive(Clone, Debug)]
pub struct ProjectModule {
    pub path: String,
    pub program: Program,
    pub source_name: String,
}

/// A parsed import statement, resolved against the importing file.
#[derive(Clone, Debug, PartialEq)]
pub enum ModuleImport {
    /// `import a.b` / `import a.b as c`: namespace access as `c.*`.
    Whole { path: String, alias: String },
    /// `from a.b import x, y`: direct access to `x`, `y`.
    Selected { path: String, names: Vec<String> },
}

/// Parse the import statements of one program.
pub fn parse_imports(program: &Program) -> Vec<ModuleImport> {
    let mut out = Vec::new();
    for statement in &program.statements {
        if let Statement::Import {
            path,
            alias,
            selected,
            ..
        } = statement
        {
            let dotted = path.join(".");
            if selected.is_empty() {
                out.push(ModuleImport::Whole {
                    alias: alias
                        .clone()
                        .unwrap_or_else(|| path.last().cloned().unwrap_or_default()),
                    path: dotted,
                });
            } else {
                out.push(ModuleImport::Selected {
                    path: dotted,
                    names: selected.clone(),
                });
            }
        }
    }
    out
}

/// Dotted path to a file under `base_dir`: `app.math` -> `base/app/math.nxl`.
pub fn module_file(base_dir: &Path, dotted: &str) -> PathBuf {
    let mut path = base_dir.to_owned();
    for segment in dotted.split('.') {
        path.push(segment);
    }
    path.set_extension("nxl");
    path
}

/// Load the entry file plus every transitively imported module that exists on
/// disk, in dependency order (dependencies before dependents). `entry_path`
/// is dotted (`""` for a bare file); `entry_text` is its content.
pub fn load_project(
    base_dir: &Path,
    entry_path: &str,
    entry_text: &str,
    entry_name: &str,
) -> Result<Vec<ProjectModule>, Diagnostic> {
    let mut modules = Vec::new();
    let mut loaded: HashSet<String> = HashSet::new();
    let mut in_stack: Vec<String> = Vec::new();
    load_module(
        base_dir,
        entry_path,
        entry_text,
        entry_name,
        &mut modules,
        &mut loaded,
        &mut in_stack,
    )?;
    Ok(modules)
}

fn load_module(
    base_dir: &Path,
    dotted: &str,
    text: &str,
    source_name: &str,
    modules: &mut Vec<ProjectModule>,
    loaded: &mut HashSet<String>,
    in_stack: &mut Vec<String>,
) -> Result<(), Diagnostic> {
    if loaded.contains(dotted) {
        return Ok(());
    }
    if in_stack.iter().any(|entry| entry == dotted) {
        let mut chain = in_stack.clone();
        chain.push(dotted.to_owned());
        return Err(Diagnostic::error(
            "NXL2002",
            format!("circular import detected: {}", chain.join(" -> ")),
            Span::default(),
        ));
    }
    in_stack.push(dotted.to_owned());
    let source = SourceFile::new(source_name, text);
    let program = crate::parse_source(&source)?;
    for import in parse_imports(&program) {
        let path = match &import {
            ModuleImport::Whole { path, .. } | ModuleImport::Selected { path, .. } => path.clone(),
        };
        if loaded.contains(&path) {
            continue;
        }
        let file = module_file(base_dir, &path);
        // Missing files are tolerated (use sites fail with NXL2002).
        if let Ok(text) = std::fs::read_to_string(&file) {
            let name = file.to_string_lossy().into_owned();
            load_module(base_dir, &path, &text, &name, modules, loaded, in_stack)?;
        }
    }
    in_stack.pop();
    loaded.insert(dotted.to_owned());
    modules.push(ProjectModule {
        path: dotted.to_owned(),
        program,
        source_name: source_name.to_owned(),
    });
    Ok(())
}

/// Normalise a dotted module path for qualified names
/// (`app.math` becomes `app::math`).
pub fn normalize_path(dotted: &str) -> String {
    dotted.replace('.', "::")
}

/// Qualify a top-level item name: entry items stay bare, others get
/// `p::Name` (`app.math` + `add` becomes `app::math::add`).
/// (`#` is reserved for method mangling.)
pub fn qualify(module: &str, name: &str) -> String {
    if module.is_empty() {
        name.to_owned()
    } else {
        format!("{}::{name}", normalize_path(module))
    }
}

/// Rename top-level functions and type declarations of non-entry modules so
/// independently authored files cannot collide after merging.
pub fn qualify_module_items(module: &mut ProjectModule) {
    if module.path.is_empty() {
        return;
    }
    fn rename(statement: &mut Statement, prefix: &str) {
        match statement {
            Statement::Function(function) => {
                if let Some(name) = function.name.clone() {
                    function.name = Some(qualify(prefix, &name));
                }
            }
            Statement::TypeDeclaration(declaration) => {
                declaration.name = qualify(prefix, &declaration.name.clone());
            }
            Statement::TypeAlias(alias) => {
                alias.name = qualify(prefix, &alias.name.clone());
            }
            Statement::Export(inner) => rename(inner, prefix),
            _ => {}
        }
    }
    let path = module.path.clone();
    for statement in &mut module.program.statements {
        rename(statement, &path);
    }
}

/// Compile an already-loaded module set to NIR.
pub fn compile_modules(modules: &mut [ProjectModule]) -> Result<crate::nir::Module, Diagnostic> {
    for module in modules.iter_mut() {
        qualify_module_items(module);
    }
    let mut files = Vec::with_capacity(modules.len());
    for module in modules {
        let context = module_context(module);
        let hir =
            crate::hir::analyze_module(&module.program, module.source_name.clone(), &context)?;
        let imports = parse_imports(&module.program);
        let aliases = file_aliases(&module.path, &module.program, &imports);
        files.push(crate::nir::ProjectFile {
            path: module.path.clone(),
            hir,
            imports,
            aliases,
        });
    }
    crate::nir::lower_project_files(&files)
}

/// Bare-to-qualified type aliases for one file: own-module types plus
/// selected imports (which may name types or functions; harmless either way).
pub(crate) fn file_aliases(
    path: &str,
    program: &Program,
    imports: &[ModuleImport],
) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    for statement in &program.statements {
        let name = match statement {
            Statement::TypeDeclaration(declaration) => Some(&declaration.name),
            Statement::TypeAlias(alias) => Some(&alias.name),
            Statement::Export(inner) => match &**inner {
                Statement::TypeDeclaration(declaration) => Some(&declaration.name),
                Statement::TypeAlias(alias) => Some(&alias.name),
                _ => None,
            },
            _ => None,
        };
        if let Some(qualified) = name {
            let bare = if path.is_empty() {
                qualified.clone()
            } else {
                let prefix = format!("{}::", normalize_path(path));
                qualified
                    .strip_prefix(&prefix)
                    .unwrap_or(qualified)
                    .to_owned()
            };
            aliases.insert(bare, qualified.clone());
        }
    }
    for import in imports {
        if let ModuleImport::Selected { path, names } = import {
            for name in names {
                aliases
                    .entry(name.clone())
                    .or_insert_with(|| qualify(path, name));
            }
        }
    }
    aliases
}

/// Name-resolution context for one file: own-module types plus selected
/// imports (which may name types or functions; harmless either way).
fn module_context(module: &ProjectModule) -> crate::hir::ModuleContext {
    let imports = parse_imports(&module.program);
    crate::hir::ModuleContext {
        module: module.path.clone(),
        aliases: file_aliases(&module.path, &module.program, &imports),
    }
}

/// Read the entry file and compile the whole project to NIR.
/// `entry_file` is the entry source path; its parent directory is the
/// module base and its dotted path is always the root namespace.
pub fn compile_project(entry_file: &Path) -> Result<crate::nir::Module, Diagnostic> {
    let text = std::fs::read_to_string(entry_file).map_err(|e| {
        Diagnostic::error(
            "NXL12006",
            format!("cannot read '{}': {e}", entry_file.to_string_lossy()),
            Span::default(),
        )
    })?;
    let base = entry_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let entry_name = entry_file.to_string_lossy().into_owned();
    let mut modules = load_project(&base, "", &text, &entry_name)?;
    compile_modules(&mut modules)
}

/// Map every loaded module path to its source for diagnostics and tests.
pub fn module_index(modules: &[ProjectModule]) -> HashMap<String, String> {
    modules
        .iter()
        .map(|module| (module.path.clone(), module.source_name.clone()))
        .collect()
}
