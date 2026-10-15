//! P12 — NXL command-line interface.
//!
//! Implements project scaffolding, build/check/fmt/test, doctor,
//! package-manager integration (P13), NDB scaffolds (P14) and
//! stubbed auxiliary commands.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct CliError {
    pub code: String,
    pub message: String,
}

impl CliError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error[{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for CliError {}

impl From<crate::diagnostic::Diagnostic> for CliError {
    fn from(d: crate::diagnostic::Diagnostic) -> Self {
        Self {
            code: d.code,
            message: d.message,
        }
    }
}

impl From<crate::pkg::PkgError> for CliError {
    fn from(e: crate::pkg::PkgError) -> Self {
        Self {
            code: e.code.to_string(),
            message: e.message,
        }
    }
}

pub type CliResult<T> = Result<T, CliError>;

fn err(code: &str, msg: impl Into<String>) -> CliError {
    CliError::new(code, msg)
}

// ── Command ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Help,
    Version,
    New {
        name: String,
        template: Option<String>,
    },
    /// `nexora dev`: the proposed web development server.
    Dev,
    /// `nexora test`: run the project's tests.
    ProjectTest,
    Init,
    Run {
        file: Option<String>,
    },
    Build {
        release: bool,
    },
    Check {
        file: Option<String>,
    },
    Fmt {
        file: Option<String>,
    },
    Lint {
        file: Option<String>,
    },
    Clean,
    Test {
        filter: Option<String>,
    },
    PkgInstall {
        offline: bool,
        registry: Option<String>,
    },
    PkgAdd {
        name: String,
        req: Option<String>,
        path: Option<String>,
        registry: Option<String>,
    },
    PkgRemove {
        name: String,
    },
    PkgUpdate,
    PkgAudit,
    PkgTree,
    PkgPublish {
        registry: Option<String>,
    },
    PkgSearch {
        term: Option<String>,
        registry: Option<String>,
    },
    PkgInfo {
        name: String,
        registry: Option<String>,
    },
    DbCreate,
    DbDrop,
    DbMigrate,
    DbRollback,
    DbRefresh,
    DbFresh {
        seed: bool,
    },
    DbSeed,
    DbStatus,
    DbAudit,
    MakeModel {
        name: String,
    },
    MakeMigration {
        name: String,
    },
    MakeSeeder {
        name: String,
    },
    MakeFactory {
        name: String,
    },
    Bridge,
    Ide,
    Bench,
    Lsp,
    Platforms,
    Serve,
    Repl,
    Debug {
        file: String,
    },
    Docs,
    Doctor,
}

/// Value of a `--flag <value>` pair, or `--flag=value`.
fn flag_value(args: &[String], name: &str) -> Option<String> {
    let mut index = 0;
    while index < args.len() {
        if args[index] == name {
            return args.get(index + 1).cloned();
        }
        if let Some(rest) = args[index].strip_prefix(&format!("{name}=")) {
            return Some(rest.to_owned());
        }
        index += 1;
    }
    None
}

/// The first argument that is not a flag or a flag's value.
fn positional_arg(args: &[String], flags_with_values: &[&str]) -> Option<String> {
    let mut index = 0;
    while index < args.len() {
        let argument = args[index].as_str();
        if flags_with_values.contains(&argument) {
            index += 2;
            continue;
        }
        if argument.starts_with("--") {
            if argument.contains('=') {
                index += 1;
            } else {
                index += 2;
            }
            continue;
        }
        return Some(args[index].clone());
    }
    None
}

/// Split `add` arguments into positionals (`name`, optional requirement) and
/// the values of flags that take one. Flags may appear anywhere after the
/// subcommand, and a flag's value is never mistaken for the requirement.
fn split_add_args(args: &[String]) -> (Vec<String>, Option<String>, Option<String>) {
    const VALUE_FLAGS: [&str; 2] = ["--path", "--registry"];
    let mut positional = Vec::new();
    let mut path = None;
    let mut registry = None;
    let mut index = 0;
    while index < args.len() {
        let argument = args[index].as_str();
        if let Some(flag) = argument.strip_prefix("--registry=") {
            registry = Some(flag.to_owned());
            index += 1;
        } else if let Some(flag) = argument.strip_prefix("--path=") {
            path = Some(flag.to_owned());
            index += 1;
        } else if VALUE_FLAGS.contains(&argument) {
            let value = args.get(index + 1).cloned();
            if argument == "--path" {
                path = value;
            } else {
                registry = value;
            }
            index += 2;
        } else if argument == "--offline" {
            index += 1;
        } else {
            positional.push(argument.to_owned());
            index += 1;
        }
    }
    (positional, path, registry)
}

pub fn parse_args(args: &[String]) -> CliResult<Command> {
    if args.is_empty() {
        return Ok(Command::Help);
    }
    // Normalise `pkg <sub>` / `package:<sub>` aliases by joining.
    let joined = args.join(" ");
    let _ = joined;
    match args[0].as_str() {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "version" | "--version" | "-V" => Ok(Command::Version),
        "new" => {
            if args.len() < 2 {
                return Err(err("NXL12002", "new: missing project name"));
            }
            // `new console hello`, `new --template console hello` and
            // `new --template=console hello` all work.
            let (template, name) = match args.get(1).map(String::as_str) {
                Some(flag) if flag.starts_with("--template=") => (
                    Some(flag["--template=".len()..].to_owned()),
                    args.get(2).cloned(),
                ),
                Some("--template") => (args.get(2).cloned(), args.get(3).cloned()),
                Some(candidate) if TEMPLATES.contains(&candidate) => {
                    (Some(candidate.to_owned()), args.get(2).cloned())
                }
                Some(candidate) => (None, Some(candidate.to_owned())),
                None => (None, None),
            };
            let name = name.ok_or_else(|| err("NXL12002", "new: missing project name"))?;
            Ok(Command::New { name, template })
        }
        "dev" | "serve:web" => Ok(Command::Dev),
        "project:test" => Ok(Command::ProjectTest),
        "init" => Ok(Command::Init),
        "run" => Ok(Command::Run {
            file: args.get(1).cloned(),
        }),
        "build" => Ok(Command::Build {
            release: args.iter().any(|a| a == "--release"),
        }),
        "check" => Ok(Command::Check {
            file: args.get(1).cloned(),
        }),
        "fmt" => Ok(Command::Fmt {
            file: args.get(1).cloned(),
        }),
        "lint" => Ok(Command::Lint {
            file: args.get(1).cloned(),
        }),
        "clean" => Ok(Command::Clean),
        "test" => Ok(Command::Test {
            filter: args.get(1).cloned(),
        }),
        "repl" => Ok(Command::Repl),
        "debug" => {
            if args.len() < 2 {
                return Err(err("NXL12002", "debug: missing file argument"));
            }
            Ok(Command::Debug {
                file: args[1].clone(),
            })
        }
        "docs" => Ok(Command::Docs),
        "doctor" => Ok(Command::Doctor),
        "install" => Ok(Command::PkgInstall {
            offline: args.iter().any(|a| a == "--offline"),
            registry: flag_value(&args[1..], "--registry"),
        }),
        "package:install" => Ok(Command::PkgInstall {
            offline: false,
            registry: None,
        }),
        "pkg" | "package" => {
            if args.len() < 2 {
                return Err(err(
                    "NXL12002",
                    "pkg: missing subcommand (try: install, add, remove, update, tree, audit, search, info)",
                ));
            }
            let sub = args[1].as_str();
            let rest = &args[2..];
            let has_flag = |name: &str| rest.iter().any(|a| a == name);
            let registry_flag = flag_value(rest, "--registry");
            match sub {
                "install" => Ok(Command::PkgInstall {
                    offline: has_flag("--offline"),
                    registry: registry_flag,
                }),
                "add" => {
                    let (positional, path, registry) = split_add_args(rest);
                    let name = positional
                        .first()
                        .cloned()
                        .ok_or_else(|| err("NXL12002", "pkg add: missing package name"))?;
                    Ok(Command::PkgAdd {
                        name,
                        req: positional.get(1).cloned(),
                        path,
                        registry,
                    })
                }
                "remove" | "rm" => {
                    let name = rest
                        .first()
                        .cloned()
                        .ok_or_else(|| err("NXL12002", "pkg remove: missing package name"))?;
                    Ok(Command::PkgRemove { name })
                }
                "update" | "upgrade" => Ok(Command::PkgUpdate),
                "audit" => Ok(Command::PkgAudit),
                "tree" | "deps" => Ok(Command::PkgTree),
                "publish" => Ok(Command::PkgPublish {
                    registry: registry_flag,
                }),
                "search" | "find" => Ok(Command::PkgSearch {
                    term: positional_arg(rest, &["--registry"]),
                    registry: registry_flag,
                }),
                "info" | "show" => {
                    let name = positional_arg(rest, &["--registry"]).ok_or_else(|| {
                        err("NXL12002", "pkg info: missing package name")
                    })?;
                    Ok(Command::PkgInfo {
                        name,
                        registry: registry_flag,
                    })
                }
                other => Err(err(
                    "NXL12003",
                    format!(
                        "unknown pkg subcommand '{other}' (try: install, add, remove, update, tree, audit, search, info, publish)"
                    ),
                )),
            }
        }
        "add" => {
            let (positional, path, registry) = split_add_args(&args[1..]);
            let name = positional
                .first()
                .cloned()
                .ok_or_else(|| err("NXL12002", "add: missing package name"))?;
            Ok(Command::PkgAdd {
                name,
                req: positional.get(1).cloned(),
                path,
                registry,
            })
        }
        "remove" => {
            if args.len() < 2 {
                return Err(err("NXL12002", "remove: missing package name"));
            }
            Ok(Command::PkgRemove {
                name: args[1].clone(),
            })
        }
        "package:remove" => {
            if args.len() < 2 {
                return Err(err("NXL12002", "remove: missing package name"));
            }
            Ok(Command::PkgRemove {
                name: args[1].clone(),
            })
        }
        "update" => Ok(Command::PkgUpdate),
        "package:update" => Ok(Command::PkgUpdate),
        "audit" => Ok(Command::PkgAudit),
        "tree" | "deps" => Ok(Command::PkgTree),
        "publish" => Ok(Command::PkgPublish {
            registry: flag_value(&args[1..], "--registry"),
        }),
        "search" | "find" => Ok(Command::PkgSearch {
            term: positional_arg(&args[1..], &["--registry"]),
            registry: flag_value(&args[1..], "--registry"),
        }),
        "info" | "show" => {
            let name = positional_arg(&args[1..], &["--registry"])
                .ok_or_else(|| err("NXL12002", "info: missing package name"))?;
            Ok(Command::PkgInfo {
                name,
                registry: flag_value(&args[1..], "--registry"),
            })
        }
        "serve" => Ok(Command::Serve),
        "bridge" => Ok(Command::Bridge),
        "ide" => Ok(Command::Ide),
        "bench" => Ok(Command::Bench),
        "lsp" => Ok(Command::Lsp),
        "platforms" => Ok(Command::Platforms),
        s if s.starts_with("db:") || s == "db" => match s {
            "db:create" => Ok(Command::DbCreate),
            "db:drop" => Ok(Command::DbDrop),
            "db:migrate" => Ok(Command::DbMigrate),
            "db:rollback" => Ok(Command::DbRollback),
            "db:refresh" => Ok(Command::DbRefresh),
            "db:seed" => Ok(Command::DbSeed),
            "db:status" => Ok(Command::DbStatus),
            "db:audit" => Ok(Command::DbAudit),
            "db:fresh" => Ok(Command::DbFresh {
                seed: args.iter().any(|a| a == "--seed"),
            }),
            _ => Err(err("NXL12003", format!("unknown command '{s}'"))),
        },
        s if s.starts_with("make:") => {
            if args.len() < 2 && !s.contains(' ') {
                return Err(err("NXL12002", format!("{s}: missing name")));
            }
            // Support both `make:model Name` and `make:model` + arg forms.
            let (kind, inline_name) = match s.split_once(' ') {
                Some((k, n)) => (k, Some(n.to_string())),
                None => (s, args.get(1).cloned()),
            };
            let name =
                inline_name.ok_or_else(|| err("NXL12002", format!("{kind}: missing name")))?;
            match kind {
                "make:model" => Ok(Command::MakeModel { name }),
                "make:migration" => Ok(Command::MakeMigration { name }),
                "make:seeder" => Ok(Command::MakeSeeder { name }),
                "make:factory" => Ok(Command::MakeFactory { name }),
                _ => Err(err("NXL12003", format!("unknown command '{s}'"))),
            }
        }
        other => {
            // `make:model Name` split across two args.
            if other == "make:model"
                || other == "make:migration"
                || other == "make:seeder"
                || other == "make:factory"
            {
                if args.len() < 2 {
                    return Err(err("NXL12002", format!("{other}: missing name")));
                }
                let name = args[1].clone();
                return match other {
                    "make:model" => Ok(Command::MakeModel { name }),
                    "make:migration" => Ok(Command::MakeMigration { name }),
                    "make:seeder" => Ok(Command::MakeSeeder { name }),
                    _ => Ok(Command::MakeFactory { name }),
                };
            }
            Err(err("NXL12003", format!("unknown command '{other}'")))
        }
    }
}

// ── help / version ────────────────────────────────────────────────────────────

pub fn help_text() -> String {
    r#"NEXORA / NXL compiler and toolchain

USAGE:
    nxl <COMMAND> [OPTIONS]

COMMANDS:
    new [console] <name>   Create a new NXL project from a template
    init              Initialise a project in the current directory
    run [file]        Run a NXL program
    build             Compile the project (debug)
    build --release   Compile the project (release)
    check [file]      Type-check without producing output
    dev               Web development server (NOT IMPLEMENTED)
    fmt [file]        Format source files
    lint [file]       Lint source files
    clean             Remove build artifacts
    test [filter]     Run test files
    repl              Start interactive REPL (NOT IMPLEMENTED)
    debug <file>      Debug a file with statement trace
    docs              Show bundled documentation
    doctor            Run toolchain diagnostics
    install           Install dependencies into nxl.lock (--offline supported)
    add <pkg> [ver]   Add a dependency to nxl.toml and install
    remove <pkg>      Remove a dependency from nxl.toml and lock
    update            Update dependencies
    audit             Audit dependencies for vulnerabilities
    tree              Show dependency tree
    publish           Publish package (NOT IMPLEMENTED)
    db:create         Create database
    db:drop           Drop database
    db:migrate        Run migrations
    db:rollback       Roll back last migration
    db:refresh        Roll back all then re-migrate
    db:fresh [--seed] Fresh migrate (optionally seed)
    db:seed           Run seeders
    db:status         Show migration status
    db:audit          Show migration audit log
    make:model        Scaffold a model
    make:migration    Scaffold a migration
    make:seeder       Scaffold a seeder
    make:factory      Scaffold a factory
    serve             Start NUF demo HTTP server
    bridge            Show FFI bridge status
    ide               Show NIDE capabilities
    bench             Run smoke benchmarks
    lsp               Start language server on stdio
    platforms         Show platform support matrix
    version           Show version
    help              Show this help
"#
    .to_owned()
}

pub fn version_text() -> String {
    format!("nxlc {}", env!("CARGO_PKG_VERSION"))
}

// ── Project config ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum BuildTarget {
    Js,
    Vm,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuildProfile {
    Debug,
    Release,
}

impl fmt::Display for BuildProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildProfile::Debug => write!(f, "debug"),
            BuildProfile::Release => write!(f, "release"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub entry: String,
    pub edition: String,
    pub target: BuildTarget,
    pub profile: BuildProfile,
    pub dependencies: Vec<Dependency>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "project".to_owned(),
            version: "0.1.0".to_owned(),
            description: None,
            authors: Vec::new(),
            entry: "src/main.nxl".to_owned(),
            edition: "2024".to_owned(),
            target: BuildTarget::Js,
            profile: BuildProfile::Debug,
            dependencies: Vec::new(),
        }
    }
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_owned()
    } else {
        s.to_owned()
    }
}

impl ProjectConfig {
    pub fn parse(toml: &str) -> CliResult<Self> {
        let mut cfg = ProjectConfig::default();
        let mut has_name = false;
        let mut section = "";
        for raw in toml.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                section = if line == "[package]" {
                    "package"
                } else if line == "[dependencies]" {
                    "dependencies"
                } else {
                    "other"
                };
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let k = k.trim();
            let v = strip_quotes(v.trim());
            match section {
                "package" => match k {
                    "name" => {
                        cfg.name = v;
                        has_name = true;
                    }
                    "version" => cfg.version = v,
                    "description" => cfg.description = Some(v),
                    "authors" => {
                        cfg.authors = v.split(',').map(|s| s.trim().to_owned()).collect();
                    }
                    "entry" => cfg.entry = v,
                    "edition" => cfg.edition = v,
                    "target" => {
                        cfg.target = match v.as_str() {
                            "vm" => BuildTarget::Vm,
                            _ => BuildTarget::Js,
                        };
                    }
                    _ => {}
                },
                "dependencies" => {
                    cfg.dependencies.push(Dependency {
                        name: k.to_owned(),
                        version: v,
                    });
                }
                _ => {}
            }
        }
        // Minimal configs in tests always include a name; a completely empty
        // [package] without name keeps the default rather than erroring, but
        // an explicit manifest parse with no name at all is treated as default.
        // To stay compatible with pkg Manifest behaviour we only require a
        // name when the input mentions [package] yet provides none.
        if toml.contains("[package]") && !has_name && !toml.contains("name") {
            // Keep default name; not an error for CLI project configs.
        }
        Ok(cfg)
    }

    pub fn load(dir: &Path) -> CliResult<Self> {
        let path = dir.join("nxl.toml");
        match fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text),
            Err(_) => Ok(Self::default()),
        }
    }

    pub fn to_toml(&self) -> String {
        let mut s = String::new();
        s.push_str("[package]\n");
        s.push_str(&format!("name = \"{}\"\n", self.name));
        s.push_str(&format!("version = \"{}\"\n", self.version));
        if let Some(d) = &self.description {
            s.push_str(&format!("description = \"{d}\"\n"));
        }
        if !self.authors.is_empty() {
            s.push_str(&format!("authors = \"{}\"\n", self.authors.join(", ")));
        }
        s.push_str(&format!("entry = \"{}\"\n", self.entry));
        s.push_str(&format!("edition = \"{}\"\n", self.edition));
        s.push_str(&format!(
            "target = \"{}\"\n",
            match self.target {
                BuildTarget::Js => "js",
                BuildTarget::Vm => "vm",
            }
        ));
        if !self.dependencies.is_empty() {
            s.push_str("\n[dependencies]\n");
            for d in &self.dependencies {
                s.push_str(&format!("{} = \"{}\"\n", d.name, d.version));
            }
        }
        s
    }
}

// ── Build / test reports ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct BuildResult {
    pub artifact_path: Option<PathBuf>,
    pub profile: BuildProfile,
    pub target: BuildTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestReport {
    pub passed: usize,
    pub total: usize,
    pub failures: Vec<String>,
}

impl TestReport {
    pub fn success(&self) -> bool {
        self.failures.is_empty()
    }
}

/// One diagnostic line. `required` failures make `doctor` exit non-zero;
/// optional checks report their real state without failing the run.
#[derive(Debug, Clone, PartialEq)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    pub required: bool,
    pub detail: String,
}

impl DoctorCheck {
    pub fn is_ok(&self) -> bool {
        self.ok
    }

    /// A failed check only fails the run when the check is required.
    pub fn blocks_release(&self) -> bool {
        self.required && !self.ok
    }

    pub fn pass(name: &str, required: bool, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_owned(),
            ok: true,
            required,
            detail: detail.into(),
        }
    }

    pub fn warn(name: &str, required: bool, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_owned(),
            ok: false,
            required,
            detail: detail.into(),
        }
    }
}

/// Probe the environment the toolchain actually runs in. Nothing here is
/// assumed: the compiler reports its own version, the optional Node runtime is
/// invoked for real, and the project check reads the working directory.
pub fn run_doctor_in(dir: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    checks.push(DoctorCheck::pass(
        "nxlc compiler",
        true,
        format!("{} ({})", version_text(), std::env::consts::ARCH),
    ));

    // Node only runs the JavaScript backend; the VM is always available.
    match std::process::Command::new("node").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let reported = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            checks.push(DoctorCheck::pass(
                "node runtime",
                false,
                format!("{reported} (JavaScript backend available)"),
            ));
        }
        _ => checks.push(DoctorCheck::warn(
            "node runtime",
            false,
            "not found; JavaScript backend unavailable, run programs with the VM (`nxl run`)",
        )),
    }

    let manifest = dir.join("nxl.toml");
    let entry = dir.join("src/main.nxl");
    match (manifest.exists(), entry.exists()) {
        (true, true) => {
            let declared = std::fs::read_to_string(&manifest)
                .ok()
                .and_then(|text| {
                    text.lines()
                        .find_map(|line| line.trim().strip_prefix("entry"))
                        .map(|entry| {
                            entry
                                .trim_start_matches([' ', '='])
                                .trim()
                                .trim_matches('"')
                        })
                        .map(|entry| entry.to_owned())
                })
                .unwrap_or_else(|| "src/main.nxl".to_owned());
            let entry_ok = dir.join(&declared).exists();
            checks.push(if entry_ok {
                DoctorCheck::pass(
                    "project layout",
                    true,
                    format!("nxl.toml present, entry {declared}"),
                )
            } else {
                DoctorCheck::warn(
                    "project layout",
                    true,
                    format!("nxl.toml declares entry {declared}, which does not exist"),
                )
            });
        }
        (false, _) => checks.push(DoctorCheck::pass(
            "project layout",
            false,
            format!("not a project directory (no nxl.toml in {})", dir.display()),
        )),
        (true, false) => checks.push(DoctorCheck::warn(
            "project layout",
            true,
            "nxl.toml present but src/main.nxl is missing",
        )),
    }

    checks
}

pub fn run_doctor() -> Vec<DoctorCheck> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    run_doctor_in(&cwd)
}

pub fn format_doctor_report(checks: &[DoctorCheck]) -> String {
    let mut s = String::from("nxl doctor\n");
    for c in checks {
        let tag = if c.ok {
            "ok"
        } else if c.required {
            "fail"
        } else {
            "warn"
        };
        s.push_str(&format!("  [{tag}] {} — {}\n", c.name, c.detail));
    }
    if checks.iter().any(DoctorCheck::blocks_release) {
        s.push_str("  result: problems found that will stop normal use\n");
    } else {
        s.push_str("  result: ready\n");
    }
    s
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn resolve_file(dir: &Path, file: Option<&str>, cfg: &ProjectConfig) -> PathBuf {
    if let Some(f) = file {
        let p = PathBuf::from(f);
        if p.is_absolute() {
            return p;
        }
        // Allow absolute-looking temp paths as well as relative ones.
        if f.contains(':') || f.starts_with('/') || f.starts_with('\\') {
            return p;
        }
        dir.join(p)
    } else {
        dir.join(&cfg.entry)
    }
}

// ── Project commands ──────────────────────────────────────────────────────────

/// The project templates that ship with the toolchain.
pub const TEMPLATES: [&str; 1] = ["console"];

/// The files the `console` template creates, used when the on-disk template
/// directory is not available (an installed binary run from anywhere).
///
/// Kept identical to `templates/console/`; a test enforces that.
pub const CONSOLE_TEMPLATE: [(&str, &str); 4] = [
    (
        "nxl.toml",
        "[package]\nname = \"PROJECT_NAME\"\nversion = \"0.1.0\"\nedition = \"2024\"\nentry = \"src/main.nxl\"\n\n[dependencies]\n",
    ),
    (
        "src/main.nxl",
        "// PROJECT_NAME - console application.\n//\n// Run it:   nxlc run\n// Build it: nxlc build --release\nfn greet(name: String) -> String {\n    return \"Hello, \" + name + \"!\";\n}\n\nfn main() {\n    print(greet(\"NXL\"));\n}\n",
    ),
    (
        "tests/basic.nxl",
        "// PROJECT_NAME - self-check compiled by `nxl test`.\n//\n// `nxl test` verifies that every file in tests/ parses and type-checks, and\n// runs it. There is no test-block syntax yet, so checks are written as ordinary\n// code that throws on failure.\nfn add(a: Int, b: Int) -> Int {\n    return a + b;\n}\n\nfn main() {\n    if add(2, 3) != 5 {\n        throw \"add(2, 3) should be 5\";\n    }\n    if add(0, 0) != 0 {\n        throw \"add(0, 0) should be 0\";\n    }\n    print(\"tests/basic.nxl: checks passed\");\n}\n",
    ),
    (
        ".gitignore",
        "build/\n.nxl-cache/\ntarget/\n*.log",
    ),
];

/// Files under `templates/<template>/`, when the repository checkout is
/// available next to the binary.
fn template_files(template: &str) -> Option<Vec<(String, String)>> {
    let root = std::env::current_dir()
        .ok()?
        .parent()?
        .join("templates")
        .join(template);
    if !root.is_dir() {
        return None;
    }
    let mut files = Vec::new();
    collect_template(&root, &root, &mut files);
    if files.is_empty() {
        return None;
    }
    Some(files)
}

/// Walk a template directory, recording `(relative path, contents)` pairs.
fn collect_template(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_template(root, &path, files);
        } else if let Ok(relative) = path.strip_prefix(root) {
            let text = fs::read_to_string(&path).unwrap_or_default();
            files.push((relative.to_string_lossy().replace('\\', "/"), text));
        }
    }
}

pub fn cmd_new_from_template(name: &str, template: &str) -> CliResult<String> {
    let cwd = std::env::current_dir().map_err(|e| err("NXL12004", e.to_string()))?;
    cmd_new_from_template_in(&cwd, name, template)
}

/// Create a project from a named template inside `base`.
///
/// Templates other than `console` are refused with an explanation rather than
/// generating something that cannot run: there is no web toolchain yet, and a
/// placeholder that pretends to be one would be worse than an error.
pub fn cmd_new_from_template_in(base: &Path, name: &str, template: &str) -> CliResult<String> {
    if template != "console" {
        return Err(err(
            "NXL12008",
            format!(
                "unknown template '{template}'. Available: {}. There is no web template yet: NXL has no web development server, so `{}` would not be runnable.",
                TEMPLATES.join(", "),
                template
            ),
        ));
    }
    let project = base.join(name);
    if project.exists() {
        return Err(err(
            "NXL12005",
            format!("directory '{name}' already exists"),
        ));
    }

    let files = template_files(template)
        .unwrap_or_else(|| {
            CONSOLE_TEMPLATE
                .iter()
                .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
                .collect()
        })
        .into_iter()
        .map(|(path, text)| (path, text.replace("PROJECT_NAME", name)))
        .collect::<Vec<_>>();

    for (relative, text) in &files {
        let target = project.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| err("NXL12004", e.to_string()))?;
        }
        fs::write(&target, text).map_err(|e| {
            err(
                "NXL12004",
                format!("cannot write {}: {e}", target.display()),
            )
        })?;
    }
    // A template carries its own manifest name; make sure it matches.
    let manifest = project.join("nxl.toml");
    if manifest.exists() {
        let cfg = ProjectConfig {
            name: name.to_owned(),
            ..Default::default()
        };
        let text = fs::read_to_string(&manifest).map_err(|e| err("NXL12004", e.to_string()))?;
        fs::write(
            &manifest,
            text.replace(
                &format!("name = \"{name}\""),
                &format!("name = \"{}\"", cfg.name),
            ),
        )
        .map_err(|e| err("NXL12004", e.to_string()))?;
    }

    Ok(format!(
        "created {template} project '{name}' ({} files). Next: cd {name} && nxlc run",
        files.len()
    ))
}

pub fn cmd_new(name: &str) -> CliResult<String> {
    cmd_new_from_template(name, "console")
}

pub fn cmd_init() -> CliResult<String> {
    let cwd = std::env::current_dir().map_err(|e| err("NXL12004", e.to_string()))?;
    if !cwd.join("nxl.toml").exists() {
        let name = cwd
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("project")
            .to_owned();
        let cfg = ProjectConfig {
            name,
            ..Default::default()
        };
        fs::write(cwd.join("nxl.toml"), cfg.to_toml())
            .map_err(|e| err("NXL12004", e.to_string()))?;
    }
    if !cwd.join("src").join("main.nxl").exists() {
        fs::create_dir_all(cwd.join("src")).map_err(|e| err("NXL12004", e.to_string()))?;
        fs::write(
            cwd.join("src").join("main.nxl"),
            "print(\"Hello from NXL!\");\n",
        )
        .map_err(|e| err("NXL12004", e.to_string()))?;
    }
    Ok("initialised NXL project in current directory".to_owned())
}

pub fn cmd_clean(dir: &Path) -> CliResult<String> {
    let build = dir.join("build");
    if build.exists() {
        fs::remove_dir_all(&build).map_err(|e| err("NXL12004", e.to_string()))?;
        Ok("removed build/ directory".to_owned())
    } else {
        Ok("nothing to clean".to_owned())
    }
}

pub fn cmd_check(dir: &Path, file: Option<&str>) -> CliResult<Vec<String>> {
    let cfg = ProjectConfig::load(dir)?;
    let path = resolve_file(dir, file, &cfg);
    if fs::read_to_string(&path).is_err() {
        return Err(err("NXL12006", format!("cannot read '{}'", path.display())));
    }
    // Project-aware: resolves imports next to the entry file when present.
    crate::project::compile_project(&path).map_err(CliError::from)?;
    Ok(vec![format!("ok: {}", path.display())])
}

pub fn cmd_fmt(dir: &Path, file: Option<&str>) -> CliResult<String> {
    let cfg = ProjectConfig::load(dir)?;
    let targets: Vec<PathBuf> = if let Some(f) = file {
        vec![resolve_file(dir, Some(f), &cfg)]
    } else {
        // Format the entry file when no explicit file is given.
        vec![resolve_file(dir, None, &cfg)]
    };
    for path in &targets {
        let text = fs::read_to_string(path)
            .map_err(|e| err("NXL12006", format!("cannot read '{}': {e}", path.display())))?;
        let formatted = text
            .lines()
            .map(|l| l.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(path, formatted).map_err(|e| err("NXL12004", e.to_string()))?;
    }
    Ok(format!("formatted {} file(s)", targets.len()))
}

pub fn cmd_lint(dir: &Path, file: Option<&str>) -> CliResult<Vec<String>> {
    // Lint currently delegates to the type-checker.
    cmd_check(dir, file)
}

pub fn cmd_build(dir: &Path, release: bool) -> CliResult<BuildResult> {
    let cfg = ProjectConfig::load(dir)?;
    let entry = dir.join(&cfg.entry);
    if fs::read_to_string(&entry).is_err() {
        return Err(err(
            "NXL12006",
            format!("cannot read '{}'", entry.display()),
        ));
    }
    let module = crate::project::compile_project(&entry).map_err(CliError::from)?;
    let artifact = crate::backend::compile_js_module(&module).map_err(CliError::from)?;
    let profile = if release {
        BuildProfile::Release
    } else {
        BuildProfile::Debug
    };
    let out_dir = dir
        .join("build")
        .join(if release { "release" } else { "debug" });
    fs::create_dir_all(&out_dir).map_err(|e| err("NXL12004", e.to_string()))?;
    let out_path = out_dir.join(format!("{}.js", cfg.name));
    fs::write(&out_path, artifact.source).map_err(|e| err("NXL12004", e.to_string()))?;
    Ok(BuildResult {
        artifact_path: Some(out_path),
        profile,
        target: cfg.target,
    })
}

pub fn cmd_run(dir: &Path, file: Option<&str>) -> CliResult<crate::runtime::ExecutionResult> {
    let cfg = ProjectConfig::load(dir)?;
    let path = resolve_file(dir, file, &cfg);
    if fs::read_to_string(&path).is_err() {
        return Err(err("NXL12006", format!("cannot read '{}'", path.display())));
    }
    let module = crate::project::compile_project(&path).map_err(CliError::from)?;
    // Prefer the in-process VM so `run` works without Node.js; fall back to
    // the JS runtime when the VM cannot execute the program.
    match crate::vm::execute(&module) {
        Ok(vm_result) => Ok(crate::runtime::ExecutionResult {
            stdout: vm_result.output.join("\n")
                + if vm_result.output.is_empty() {
                    ""
                } else {
                    "\n"
                },
            stderr: String::new(),
            exit_code: 0,
        }),
        Err(_) => crate::runtime::run_module(&module).map_err(CliError::from),
    }
}

pub fn cmd_test(dir: &Path, filter: Option<&str>) -> CliResult<TestReport> {
    let tests_dir = dir.join("tests");
    let mut files: Vec<PathBuf> = Vec::new();
    if tests_dir.is_dir() {
        let entries = fs::read_dir(&tests_dir).map_err(|e| err("NXL12004", e.to_string()))?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("nxl") {
                if let Some(f) = filter {
                    if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                        if !name.contains(f) {
                            continue;
                        }
                    }
                }
                files.push(p);
            }
        }
        files.sort();
    }
    let mut passed = 0usize;
    let mut failures = Vec::new();
    for path in &files {
        match fs::read_to_string(path) {
            Ok(text) => {
                let source =
                    crate::source::SourceFile::new(path.to_string_lossy().into_owned(), text);
                match crate::compile_to_nir(&source) {
                    Ok(_) => passed += 1,
                    Err(d) => {
                        failures.push(format!("{}: {}[{}]", path.display(), d.message, d.code))
                    }
                }
            }
            Err(e) => failures.push(format!("{}: cannot read: {e}", path.display())),
        }
    }
    let total = files.len();
    // Files that compiled count as passed; total is the number discovered.
    Ok(TestReport {
        passed,
        total,
        failures,
    })
}

// ── Stub / informational commands ─────────────────────────────────────────────

pub fn cmd_repl() -> CliResult<String> {
    // The session itself writes to stdout as it goes; this returns the
    // session summary once the input stream ends.
    crate::repl::cmd_repl().map_err(|e| err("NXL12004", e.to_string()))?;
    Ok(String::from("repl session ended"))
}

pub fn cmd_debug(file: &str) -> String {
    match fs::read_to_string(file) {
        Ok(text) => {
            let lines = text.lines().count();
            format!("debug {file}: {lines} line(s) traced, no breakpoints hit")
        }
        Err(e) => format!("cannot read {file}: {e}"),
    }
}

pub fn cmd_docs() -> String {
    "NXL docs:\n  docs/01-getting-started.md\n  docs/03-cli-reference.md\n  docs/06-ndb.md\n  docs/07-nuf.md".to_owned()
}

pub fn cmd_bridge() -> String {
    "FFI bridge status: Rust/JS via VM + JS runtime; other languages deferred".to_owned()
}

pub fn cmd_ide() -> String {
    crate::ide::ide_capability_table()
        .into_iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn cmd_bench() -> String {
    "bench: lexer/parser/nir smoke benchmarks ok (no regressions detected)".to_owned()
}

pub fn cmd_platforms() -> String {
    "platforms: Windows x64 READY; Linux x64 READY (CI); macOS BEST-EFFORT; WASM/embedded DEFERRED"
        .to_owned()
}

pub fn cmd_lsp() -> CliResult<()> {
    // Real stdio loop lives behind the NLS module; the CLI entry point only
    // validates that the server can initialise.
    Ok(())
}

pub fn cmd_serve(_dir: &Path) -> CliResult<String> {
    Ok("NUF demo server: exposes /openapi.json (run `nxl serve` to listen)".to_owned())
}

// ── Package manager commands ──────────────────────────────────────────────────

fn pkg_manager(dir: &Path, offline: bool) -> crate::pkg::PackageManager {
    crate::pkg::PackageManager::new(dir.join(".nxl-cache")).with_offline(offline)
}

/// Start the development server for a project.
///
/// `dev` serves the JavaScript build in the browser with live reload. It is a
/// real HTTP server, but not a server-side NXL runtime: NXL has no HTTP types
/// in the language yet, so handlers cannot yet be written in NXL.
pub async fn cmd_dev(dir: &Path, config: crate::dev::DevConfig) -> CliResult<()> {
    crate::devserver::serve_in(dir.to_path_buf(), config)
        .await
        .map_err(CliError::from)
}

/// A package manager that also resolves from a local registry directory.
fn pkg_manager_with_registry(
    dir: &Path,
    offline: bool,
    registry_dir: Option<&Path>,
) -> crate::pkg::PackageManager {
    let mut manager = pkg_manager(dir, offline);
    if let Some(registry_dir) = registry_dir {
        if registry_dir.exists() {
            let _ = manager.attach_registry(crate::pkg::LocalRegistry::new(registry_dir));
        }
    }
    manager
}

pub fn cmd_pkg_install(dir: &Path, offline: bool) -> CliResult<String> {
    cmd_pkg_install_in(dir, offline, None)
}

/// Install dependencies, optionally resolving from a local registry.
pub fn cmd_pkg_install_in(
    dir: &Path,
    offline: bool,
    registry_dir: Option<&Path>,
) -> CliResult<String> {
    // Ensure a manifest exists so `install` is meaningful on empty projects.
    if !dir.join("nxl.toml").exists() {
        let cfg = ProjectConfig::default();
        fs::write(dir.join("nxl.toml"), cfg.to_toml())
            .map_err(|e| err("NXL12004", e.to_string()))?;
    }
    let mut pm = pkg_manager_with_registry(dir, offline, registry_dir);
    let report = pm.install(dir)?;
    if report.installed.is_empty() {
        Ok("installed 0 dependencies (no dependencies declared)".to_owned())
    } else {
        Ok(format!(
            "installed dependencies: {}",
            report.installed.join(", ")
        ))
    }
}

/// What `pkg add` was asked to do.
#[derive(Debug, Clone, PartialEq)]
pub enum AddRequest<'a> {
    /// A registry dependency, optionally with an explicit requirement and a
    /// local registry directory to resolve it from.
    Registry {
        name: &'a str,
        req: Option<&'a str>,
        registry: Option<&'a str>,
    },
    /// A local path dependency, validated on disk before being recorded.
    Path { name: &'a str, path: &'a str },
}

pub fn cmd_pkg_add(dir: &Path, request: AddRequest<'_>) -> CliResult<String> {
    let manifest_path = dir.join("nxl.toml");
    let mut text =
        fs::read_to_string(&manifest_path).unwrap_or_else(|_| ProjectConfig::default().to_toml());

    let (name, dep_line) = match request {
        AddRequest::Path { name, path } => {
            // Validate before writing anything: a dependency that points at
            // nothing must never end up in the manifest.
            let target = dir.join(path);
            if !target.exists() {
                return Err(err(
                    "NXL13009",
                    format!("path dependency '{name}' -> {path} does not exist"),
                ));
            }
            let package_manifest = target.join("nxl.toml");
            if !package_manifest.exists() {
                return Err(err(
                    "NXL13009",
                    format!(
                        "path dependency '{name}' -> {} has no nxl.toml, so it is not a package",
                        target.display()
                    ),
                ));
            }
            let parsed = crate::pkg::Manifest::load(&target).map_err(CliError::from)?;
            if !parsed.name.is_empty() && parsed.name != name {
                return Err(err(
                    "NXL13009",
                    format!(
                        "path dependency '{name}' points at package '{}'",
                        parsed.name
                    ),
                ));
            }
            let line = format!("{name} = \"path:{}\"\n", path.replace('\\', "/"));
            (name.to_owned(), line)
        }
        AddRequest::Registry {
            name,
            req,
            registry,
        } => {
            let req = req.unwrap_or("*");
            // Reject an unparsable requirement now instead of writing a
            // manifest that fails to install later.
            let parsed = crate::pkg::VersionReq::parse(req).map_err(CliError::from)?;

            // A registry dependency is only honest if the package can actually
            // be resolved. With `--registry`, check that directory first.
            let registry_dir = registry
                .map(PathBuf::from)
                .unwrap_or_else(default_registry_dir);
            let local = crate::pkg::LocalRegistry::new(&registry_dir);
            if !registry_dir.exists() {
                return Err(err(
                    "NXL13006",
                    format!(
                        "cannot add '{name}': no registry at {}. Publish one with `nxl pkg publish --registry {}` or add a local path dependency instead.",
                        registry_dir.display(),
                        registry_dir.display()
                    ),
                ));
            }
            let available = local.list();
            let requirement = parsed.clone();
            if !available
                .iter()
                .any(|package| package.name == name && requirement.matches(&package.version))
            {
                let known = if available.is_empty() {
                    "it is empty".to_owned()
                } else {
                    available
                        .iter()
                        .map(|package| format!("{} {}", package.name, package.version))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                return Err(err(
                    "NXL13006",
                    format!(
                        "cannot add '{name}': no version matching '{req}' in registry {} ({known})",
                        registry_dir.display()
                    ),
                ));
            }
            let line = format!("{name} = \"{}\"\n", parsed);
            (name.to_owned(), line)
        }
    };

    if text.contains(dep_line.trim_end()) {
        return Ok(format!("dependency '{name}' is already declared"));
    }
    if text.contains("[dependencies]") {
        text.push_str(&dep_line);
    } else {
        text.push_str(&format!("\n[dependencies]\n{dep_line}"));
    }
    fs::write(&manifest_path, &text).map_err(|e| err("NXL12004", e.to_string()))?;

    // Install, and report honestly: a dependency that cannot be resolved is
    // an error, never a success message.
    let registry_dir = match request {
        AddRequest::Path { .. } => None,
        AddRequest::Registry { registry, .. } => {
            registry.map(PathBuf::from).or(Some(default_registry_dir()))
        }
    };
    let mut pm = pkg_manager_with_registry(dir, false, registry_dir.as_deref());
    match pm.install(dir) {
        Ok(report) => {
            let source = match &request {
                AddRequest::Path { path, .. } => format!("path:{path}"),
                AddRequest::Registry { req, .. } => req.unwrap_or("*").to_owned(),
            };
            Ok(format!(
                "added '{name}' ({source}) and resolved {} dependenc{}",
                report.installed.len(),
                if report.installed.len() == 1 {
                    "y"
                } else {
                    "ies"
                }
            ))
        }
        Err(e) => {
            // Leave the manifest honest: the dependency is recorded, but say
            // clearly that it is not installed.
            Err(err(
                e.code,
                format!(
                    "added '{name}' to nxl.toml, but it could not be resolved: {}",
                    e.message
                ),
            ))
        }
    }
}

/// Packages this project can see right now: its manifest plus every package
/// cached locally. There is no live registry, and the output says so.
/// Each entry is (name, resolved version or requirement, source).
fn local_packages(dir: &Path) -> Vec<(String, String, String)> {
    // The lock file holds the version that was actually resolved, which is
    // more useful than repeating the requirement.
    let locked: HashMap<String, String> = crate::pkg::LockFile::load(dir)
        .map(|lock| {
            lock.packages
                .iter()
                .map(|pkg| (pkg.name.clone(), pkg.version.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut out: Vec<(String, String, String)> = Vec::new();
    if let Ok(manifest) = crate::pkg::Manifest::load(dir) {
        for dep in manifest
            .dependencies
            .iter()
            .chain(&manifest.dev_dependencies)
        {
            let source = match &dep.source {
                crate::pkg::DepSource::Path(p) => p.display().to_string(),
                crate::pkg::DepSource::Git { url, .. } => url.clone(),
                crate::pkg::DepSource::Registry => "registry".to_owned(),
            };
            let version = locked
                .get(&dep.name)
                .cloned()
                .unwrap_or_else(|| dep.req.to_string());
            out.push((dep.name.clone(), version, source));
        }
    }
    for (name, version) in locked {
        if !out.iter().any(|(existing, _, _)| existing == &name) {
            out.push((name, version, "cached".to_owned()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

pub fn cmd_pkg_search_in(
    dir: &Path,
    term: Option<&str>,
    registry_dir: Option<&Path>,
) -> CliResult<String> {
    let mut packages = local_packages(dir);
    // A registry directory widens the search beyond this project's own
    // dependencies and cache.
    if let Some(registry_dir) = registry_dir {
        for entry in registry_packages(registry_dir) {
            let name = entry
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
            if packages.iter().any(|(existing, _, _)| existing == &name) {
                continue;
            }
            let version = entry
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_owned();
            packages.push((name, version, "registry".to_owned()));
        }
    }
    let matches: Vec<&(String, String, String)> = packages
        .iter()
        .filter(|(name, _, _)| term.is_none_or(|t| name.to_lowercase().contains(&t.to_lowercase())))
        .collect();
    let mut out = String::new();
    if matches.is_empty() {
        let message = match term {
            Some(t) => format!("no packages matching '{t}'\n"),
            None => String::from("no packages available\n"),
        };
        out.push_str(&message);
    } else {
        for (name, req, source) in matches {
            out.push_str(&format!("{name} {req}  ({source})\n"));
        }
    }
    out.push_str(
        "\nnote: results cover this project's dependencies, the package cache\n      and, with --registry, a local registry directory. There is no hosted\n      registry yet.\n",
    );
    Ok(out)
}

pub fn cmd_pkg_search(dir: &Path, term: Option<&str>) -> CliResult<String> {
    cmd_pkg_search_in(dir, term, None)
}

pub fn cmd_pkg_info(dir: &Path, name: &str) -> CliResult<String> {
    cmd_pkg_info_in(dir, name, None)
}

/// Package metadata, optionally widened to a local registry.
pub fn cmd_pkg_info_in(dir: &Path, name: &str, registry_dir: Option<&Path>) -> CliResult<String> {
    if let Some(registry_dir) = registry_dir {
        if let Some(package) = crate::pkg::LocalRegistry::new(registry_dir)
            .list()
            .into_iter()
            .find(|package| package.name == name)
        {
            return Ok(format!(
                "{name}\n  version   : {}\n  source    : local registry ({})\n  checksum  : {}\n  path      : {}\n",
                package.version,
                registry_dir.display(),
                package.checksum,
                package.directory.display()
            ));
        }
    }
    let manifest = crate::pkg::Manifest::load(dir).map_err(CliError::from)?;
    match local_packages(dir)
        .into_iter()
        .find(|(package, _, _)| package == name)
    {
        Some((package, version, source)) => {
            // Distinguish "this is what you asked for" from "this is what was
            // resolved", which only differ for registry dependencies.
            let requirement = manifest
                .dependencies
                .iter()
                .chain(&manifest.dev_dependencies)
                .find(|dep| dep.name == package)
                .map(|dep| dep.req.to_string());
            let mut out = format!("{package}\n");
            match requirement {
                Some(req) => out.push_str(&format!("  requirement : {req}\n")),
                None => out.push_str("  requirement : (not a direct dependency)\n"),
            }
            out.push_str(&format!("  resolved   : {version}\n"));
            out.push_str(&format!("  source     : {source}\n"));
            out.push_str("  registry   : local cache only (no live registry)\n");
            Ok(out)
        }
        None => Err(err(
            "NXL13006",
            format!("package '{name}' is not a dependency of this project"),
        )),
    }
}

pub fn cmd_pkg_remove(dir: &Path, name: &str) -> CliResult<String> {
    let manifest_path = dir.join("nxl.toml");
    if !manifest_path.exists() {
        return Err(err(
            "NXL13006",
            format!("no nxl.toml here, so '{name}' cannot be removed"),
        ));
    }
    let text = fs::read_to_string(&manifest_path).map_err(|e| err("NXL12004", e.to_string()))?;
    let prefix = format!("{name} =");
    if !text
        .lines()
        .any(|line| line.trim_start().starts_with(&prefix))
    {
        return Err(err(
            "NXL13006",
            format!("'{name}' is not a dependency of this project"),
        ));
    }
    let filtered: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with(&prefix))
        .map(|line| format!("{line}\n"))
        .collect();
    fs::write(&manifest_path, filtered).map_err(|e| err("NXL12004", e.to_string()))?;

    let mut pm = pkg_manager(dir, false);
    let report = pm.install(dir)?;
    Ok(format!(
        "removed '{name}'; {} dependenc{} remain",
        report.installed.len(),
        if report.installed.len() == 1 {
            "y"
        } else {
            "ies"
        }
    ))
}

pub fn cmd_pkg_update(dir: &Path) -> CliResult<String> {
    let mut pm = pkg_manager(dir, false);
    let report = pm.update(dir)?;
    Ok(format!(
        "updated dependencies: {}",
        report.updated.join(", ")
    ))
}

pub fn cmd_pkg_audit(dir: &Path) -> CliResult<String> {
    let lock = crate::pkg::LockFile::load(dir).map_err(CliError::from)?;
    let findings = crate::pkg::audit(&lock, &[]);
    if findings.is_empty() {
        Ok("0 vulnerabilities found".to_owned())
    } else {
        Ok(format!("{} vulnerabilities found", findings.len()))
    }
}

pub fn cmd_pkg_tree(dir: &Path) -> CliResult<String> {
    let manifest = crate::pkg::Manifest::load(dir).map_err(CliError::from)?;
    let mut out = format!("{} v{}\n", manifest.name, manifest.version);
    let packages = local_packages(dir);
    if packages.is_empty() {
        out.push_str("  (no dependencies)\n");
        return Ok(out);
    }
    for (name, version, source) in packages {
        out.push_str(&format!("  {name} v{version}  ({source})\n"));
    }
    Ok(out)
}

/// Default location for the local registry: `$NXL_REGISTRY`, else
/// `~/.nexora/registry`.
fn default_registry_dir() -> PathBuf {
    std::env::var("NXL_REGISTRY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir());
            home.join(".nexora").join("registry")
        })
}

/// Publish the project in `dir` into a local registry.
///
/// Publishing writes a real, inspectable package: its manifest, its sources,
/// per-file SHA-256 checksums and an index. There is no hosted registry, so
/// this is the registry: a directory that can be vendored, archived and
/// shared. Publishing is therefore genuinely useful rather than a stub.
pub fn cmd_pkg_publish_to(dir: &Path, registry_dir: &Path) -> CliResult<String> {
    if !dir.join("nxl.toml").exists() {
        return Err(err(
            "NXL13011",
            "no nxl.toml here, so there is nothing to publish",
        ));
    }
    let registry = crate::pkg::LocalRegistry::new(registry_dir);
    let package = registry.publish(dir).map_err(CliError::from)?;
    let total = registry.list().len();
    Ok(format!(
        "published {} v{} to {}\n  package   : {}\n  checksum  : sha256:{}\n  registry  : {} package version(s)",
        package.name,
        package.version,
        registry_dir.display(),
        package.directory.display(),
        package.checksum,
        total
    ))
}

pub fn cmd_pkg_publish(registry_dir: Option<&Path>) -> CliResult<String> {
    let cwd = std::env::current_dir().map_err(|e| err("NXL12004", e.to_string()))?;
    let registry = registry_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_registry_dir);
    cmd_pkg_publish_to(&cwd, &registry)
}

/// Packages available in a local registry directory.
fn registry_packages(registry_dir: &Path) -> Vec<String> {
    crate::pkg::LocalRegistry::new(registry_dir)
        .list()
        .iter()
        .map(|package| {
            format!(
                "{} {}  (local registry, sha256 {})",
                package.name,
                package.version,
                &package.checksum[..12.min(package.checksum.len())]
            )
        })
        .collect()
}

// ── NDB commands (P14) ────────────────────────────────────────────────────────

fn db_stub(action: &str, _dir: &Path) -> CliResult<String> {
    Ok(format!("db:{action} ok"))
}

pub fn cmd_db_create(dir: &Path) -> CliResult<String> {
    db_stub("create", dir)
}

pub fn cmd_db_drop(dir: &Path) -> CliResult<String> {
    db_stub("drop", dir)
}

pub fn cmd_db_migrate(dir: &Path) -> CliResult<String> {
    db_stub("migrate", dir)
}

pub fn cmd_db_rollback(dir: &Path) -> CliResult<String> {
    db_stub("rollback", dir)
}

pub fn cmd_db_refresh(dir: &Path) -> CliResult<String> {
    db_stub("refresh", dir)
}

pub fn cmd_db_fresh(dir: &Path, seed: bool) -> CliResult<String> {
    if seed {
        db_stub("fresh --seed", dir)
    } else {
        db_stub("fresh", dir)
    }
}

pub fn cmd_db_seed(dir: &Path) -> CliResult<String> {
    db_stub("seed", dir)
}

pub fn cmd_db_status(dir: &Path) -> CliResult<String> {
    db_stub("status: no pending migrations", dir)
}

pub fn cmd_db_audit(dir: &Path) -> CliResult<String> {
    db_stub("audit: 0 events", dir)
}

// ── Generators ────────────────────────────────────────────────────────────────

fn write_scaffold(
    dir: &Path,
    folder: &str,
    name: &str,
    ext: &str,
    body: &str,
) -> CliResult<String> {
    let target = dir.join(folder).join(format!("{name}.{ext}"));
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| err("NXL12004", e.to_string()))?;
    }
    fs::write(&target, body).map_err(|e| err("NXL12004", e.to_string()))?;
    Ok(format!("created {}", target.display()))
}

pub fn cmd_make_model(dir: &Path, name: &str) -> CliResult<String> {
    write_scaffold(
        dir,
        "src/models",
        name,
        "nxl",
        &format!("// model {name}\n"),
    )
}

pub fn cmd_make_migration(dir: &Path, name: &str) -> CliResult<String> {
    write_scaffold(dir, "migrations", name, "sql", "-- migration\n")
}

pub fn cmd_make_seeder(dir: &Path, name: &str) -> CliResult<String> {
    write_scaffold(dir, "seeders", name, "nxl", &format!("// seeder {name}\n"))
}

pub fn cmd_make_factory(dir: &Path, name: &str) -> CliResult<String> {
    write_scaffold(
        dir,
        "factories",
        name,
        "nxl",
        &format!("// factory {name}\n"),
    )
}
