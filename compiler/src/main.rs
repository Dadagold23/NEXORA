use nxl_compiler::dev;
use nxl_compiler::mobile;
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use nxl_compiler::cli::{
    self, cmd_bench, cmd_bridge, cmd_build, cmd_check, cmd_clean, cmd_db_audit, cmd_db_create,
    cmd_db_drop, cmd_db_fresh, cmd_db_migrate, cmd_db_refresh, cmd_db_rollback, cmd_db_seed,
    cmd_db_status, cmd_debug, cmd_docs, cmd_fmt, cmd_ide, cmd_init, cmd_lint, cmd_lsp,
    cmd_make_factory, cmd_make_migration, cmd_make_model, cmd_make_seeder, cmd_new,
    cmd_new_from_template, cmd_pkg_add, cmd_pkg_audit, cmd_pkg_info_in, cmd_pkg_install_in,
    cmd_pkg_publish, cmd_pkg_remove, cmd_pkg_search_in, cmd_pkg_tree, cmd_pkg_update,
    cmd_platforms, cmd_repl, cmd_run, cmd_serve, cmd_test, format_doctor_report, help_text,
    parse_args, prompt_destructive, run_doctor, version_text, AddRequest, Command,
};

/// Resolve confirmation for a destructive database command.
///
/// `--yes` short-circuits; an interactive terminal gets a prompt naming the
/// target; anything else falls through so `cmd_db_*` reports NXL12010 with
/// the `--yes` hint. `None` means the operator aborted at the prompt.
fn confirmed_destructive(dir: &Path, action: &str, yes: bool) -> Option<bool> {
    use std::io::IsTerminal;
    if yes || !std::io::stdin().is_terminal() {
        return Some(yes);
    }
    if prompt_destructive(dir, action) {
        Some(true)
    } else {
        println!("aborted; nothing was changed.");
        None
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    // `--json` selects machine-readable diagnostics for check/run/build. It is
    // stripped before parsing so it is never mistaken for a file argument,
    // which also makes `check --json file.nxl` and `check file.nxl --json`
    // behave identically.
    let json = raw_args.iter().any(|a| a == "--json");
    let args: Vec<String> = raw_args
        .iter()
        .filter(|a| a.as_str() != "--json")
        .cloned()
        .collect();
    // dev may bind a host:port, optionally overridden with --port.

    let mut dev_config = dev::DevConfig::default();
    if let Some(index) = args.iter().position(|a| a == "--port" || a == "--host") {
        let flag = args[index].clone();
        let value = args.get(index + 1).cloned();
        if let Some(value) = value {
            if flag == "--port" {
                dev_config.address = format!("127.0.0.1:{value}");
            } else {
                dev_config.address = value;
            }
        }
    }
    if args.iter().any(|a| a == "--no-watch") {
        dev_config.watch = false;
    }

    let command = match parse_args(&args) {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    match command {
        Command::Help => {
            print!("{}", help_text());
            ExitCode::SUCCESS
        }

        Command::Version => {
            println!("{}", version_text());
            ExitCode::SUCCESS
        }

        Command::New { name, template } => {
            let outcome = match template {
                Some(template) => cmd_new_from_template(&name, &template),
                None => cmd_new(&name),
            };
            match outcome {
                Ok(msg) => {
                    println!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }

        Command::Dev => match cli::cmd_dev(&cwd, dev_config).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::ProjectTest => match cmd_test(&cwd, None) {
            Ok(report) => {
                println!("test: {}/{} passed", report.passed, report.total);
                for failure in &report.failures {
                    println!("  failed: {failure}");
                }
                if report.failures.is_empty() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::Init => match cmd_init() {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::Run { file } => match cmd_run(&cwd, file.as_deref()) {
            Ok(result) => {
                print!("{}", result.stdout);
                if !result.stderr.is_empty() {
                    eprint!("{}", result.stderr);
                }
                if result.success() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(e) => {
                if json {
                    // The program owns stdout, so run's diagnostics go to
                    // stderr: a pipeline can parse either stream alone.
                    eprintln!("{}", cli::json_diagnostics(&[e]));
                } else {
                    eprintln!("{e}");
                }
                ExitCode::FAILURE
            }
        },

        Command::Build { release, target } => {
            // A mobile project defaults to its own target; an explicit
            // `--target` overrides, and anything but android is refused rather
            // than silently ignored.
            let use_android = match target.as_deref() {
                Some("android") => true,
                Some(other) => {
                    let error = cli::CliError::new(
                        "NXL12025",
                        format!(
                            "unknown build target '{other}'; implemented targets: {}",
                            mobile::SUPPORTED_TARGETS.join(", ")
                        ),
                    );
                    if json {
                        eprintln!("{}", cli::json_diagnostics(&[error]));
                    } else {
                        eprintln!("{error}");
                    }
                    return ExitCode::FAILURE;
                }
                None => match mobile::MobileConfig::load(&cwd) {
                    Ok(config) => config.is_some(),
                    Err(error) => {
                        if json {
                            eprintln!("{}", cli::json_diagnostics(&[error]));
                        } else {
                            eprintln!("{error}");
                        }
                        return ExitCode::FAILURE;
                    }
                },
            };

            if use_android {
                match mobile::build_android(&cwd, release) {
                    Ok(artifact) => {
                        let path = artifact.display().to_string();
                        let profile = if release { "release" } else { "debug" };
                        if json {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "status": "ok",
                                    "profile": profile,
                                    "target": "android",
                                    "artifact": path,
                                })
                            );
                        } else {
                            println!("build ({profile}, android) → {path}");
                        }
                        ExitCode::SUCCESS
                    }
                    Err(error) => {
                        if json {
                            eprintln!("{}", cli::json_diagnostics(&[error]));
                        } else {
                            eprintln!("{error}");
                        }
                        ExitCode::FAILURE
                    }
                }
            } else {
                match cmd_build(&cwd, release) {
                    Ok(result) => {
                        let path = result
                            .artifact_path
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        if json {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "status": "ok",
                                    "profile": result.profile.to_string(),
                                    "artifact": path,
                                })
                            );
                        } else {
                            println!("build ({}) → {path}", result.profile);
                        }
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        if json {
                            eprintln!("{}", cli::json_diagnostics(&[e]));
                        } else {
                            eprintln!("{e}");
                        }
                        ExitCode::FAILURE
                    }
                }
            }
        }

        Command::Check { file } => match cmd_check(&cwd, file.as_deref()) {
            Ok(msgs) => {
                if json {
                    // A clean check has nothing to report: an empty array.
                    println!("{}", cli::json_diagnostics(&[]));
                } else {
                    for m in msgs {
                        println!("{m}");
                    }
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                if json {
                    // check exists to report diagnostics, so they are its
                    // primary output and go to stdout — success or failure.
                    println!("{}", cli::json_diagnostics(&[e]));
                } else {
                    eprintln!("{e}");
                }
                ExitCode::FAILURE
            }
        },

        Command::Fmt { file } => match cmd_fmt(&cwd, file.as_deref()) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::Lint { file } => match cmd_lint(&cwd, file.as_deref()) {
            Ok(msgs) => {
                for m in msgs {
                    println!("{m}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::Clean => match cmd_clean(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::Test { filter } => match cmd_test(&cwd, filter.as_deref()) {
            Ok(report) => {
                println!("test: {}/{} passed", report.passed, report.total);
                for f in &report.failures {
                    eprintln!("  FAIL: {f}");
                }
                if report.success() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::PkgInstall { offline, registry } => {
            match cmd_pkg_install_in(&cwd, offline, registry.as_deref().map(Path::new)) {
                Ok(msg) => {
                    println!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }

        Command::PkgAdd {
            name,
            req,
            path,
            registry,
        } => {
            let outcome = match path {
                Some(path) => cmd_pkg_add(
                    &cwd,
                    AddRequest::Path {
                        name: &name,
                        path: &path,
                    },
                ),
                None => cmd_pkg_add(
                    &cwd,
                    AddRequest::Registry {
                        name: &name,
                        req: req.as_deref(),
                        registry: registry.as_deref(),
                    },
                ),
            };
            match outcome {
                Ok(msg) => {
                    println!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }

        Command::PkgRemove { name } => match cmd_pkg_remove(&cwd, &name) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::PkgUpdate => match cmd_pkg_update(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::PkgAudit => match cmd_pkg_audit(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::PkgTree => match cmd_pkg_tree(&cwd) {
            Ok(tree) => {
                print!("{tree}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::PkgPublish { registry } => {
            match cmd_pkg_publish(registry.as_deref().map(Path::new)) {
                Ok(msg) => {
                    println!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }

        Command::PkgSearch { term, registry } => {
            match cmd_pkg_search_in(&cwd, term.as_deref(), registry.as_deref().map(Path::new)) {
                Ok(msg) => {
                    print!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }

        Command::PkgInfo { name, registry } => {
            match cmd_pkg_info_in(&cwd, &name, registry.as_deref().map(Path::new)) {
                Ok(msg) => {
                    print!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }

        Command::DbCreate => match cmd_db_create(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::DbDrop { yes } => match confirmed_destructive(&cwd, "db:drop", yes) {
            None => ExitCode::FAILURE,
            Some(confirmed) => match cmd_db_drop(&cwd, confirmed) {
                Ok(msg) => {
                    println!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            },
        },
        Command::DbMigrate => match cmd_db_migrate(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::DbRollback => match cmd_db_rollback(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::DbRefresh { yes } => match confirmed_destructive(&cwd, "db:refresh", yes) {
            None => ExitCode::FAILURE,
            Some(confirmed) => match cmd_db_refresh(&cwd, confirmed) {
                Ok(msg) => {
                    println!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            },
        },
        Command::DbFresh { seed, yes } => match confirmed_destructive(&cwd, "db:fresh", yes) {
            None => ExitCode::FAILURE,
            Some(confirmed) => match cmd_db_fresh(&cwd, seed, confirmed) {
                Ok(msg) => {
                    println!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            },
        },
        Command::DbSeed => match cmd_db_seed(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::DbStatus => match cmd_db_status(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::DbAudit => match cmd_db_audit(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::MakeModel { name } => match cmd_make_model(&cwd, &name) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::MakeMigration { name } => match cmd_make_migration(&cwd, &name) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::MakeSeeder { name } => match cmd_make_seeder(&cwd, &name) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::MakeFactory { name } => match cmd_make_factory(&cwd, &name) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::Bridge => {
            println!("{}", cmd_bridge());
            ExitCode::SUCCESS
        }
        Command::Ide => {
            println!("{}", cmd_ide());
            ExitCode::SUCCESS
        }
        Command::Bench => {
            println!("{}", cmd_bench());
            ExitCode::SUCCESS
        }
        Command::Lsp => match cmd_lsp() {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::Platforms => {
            println!("{}", cmd_platforms());
            ExitCode::SUCCESS
        }
        Command::Serve => match cmd_serve(&cwd).await {
            Ok(msg) => {
                if !msg.is_empty() {
                    println!("{msg}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::Repl => match cmd_repl() {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::Debug {
            file,
            breakpoints,
            inspect,
            steps,
        } => {
            println!("{}", cmd_debug(&file, &breakpoints, inspect, steps));
            ExitCode::SUCCESS
        }
        Command::Docs => {
            println!("{}", cmd_docs());
            ExitCode::SUCCESS
        }

        Command::Doctor => {
            let checks = run_doctor();
            print!("{}", format_doctor_report(&checks));
            // Optional gaps (Node, not being inside a project) are reported
            // but do not fail the run; required failures do.
            if checks.iter().all(|c| !c.blocks_release()) {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
    }
}
