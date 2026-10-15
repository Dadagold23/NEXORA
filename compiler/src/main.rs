use nxl_compiler::dev;
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
    parse_args, run_doctor, version_text, AddRequest, Command,
};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
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
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::Build { release } => match cmd_build(&cwd, release) {
            Ok(result) => {
                let path = result
                    .artifact_path
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                println!("build ({}) → {path}", result.profile);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },

        Command::Check { file } => match cmd_check(&cwd, file.as_deref()) {
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
        Command::DbDrop => match cmd_db_drop(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
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
        Command::DbRefresh => match cmd_db_refresh(&cwd) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::DbFresh { seed } => match cmd_db_fresh(&cwd, seed) {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
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
        Command::Serve => match cmd_serve(&cwd) {
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
        Command::Debug { file } => {
            println!("{}", cmd_debug(&file));
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
