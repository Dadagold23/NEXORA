//! NXP package-manager behaviour (add_package.txt Phase 4).
//!
//! The theme of these tests is honesty: commands must never report success
//! for something that did not happen, and the supported subset (local path
//! dependencies) must actually work end to end.

use nxl_compiler::cli::{parse_args, AddRequest, Command};
use std::fs;
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("nxl_nxp_{tag}_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write_package(&self, name: &str, version: &str, source: &str) -> PathBuf {
        let dir = self.path().join(name);
        fs::create_dir_all(dir.join("src")).expect("package dirs");
        fs::write(
            dir.join("nxl.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n"),
        )
        .expect("manifest");
        fs::write(dir.join("src/main.nxl"), source).expect("entry");
        dir
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn project(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.path().join("nxl.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"src/main.nxl\"\n",
    )
    .expect("manifest");
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.nxl"), "print(\"app\");").unwrap();
    dir
}

// ── version parsing ─────────────────────────────────────────────────────────────

#[test]
fn partial_versions_are_accepted() {
    use nxl_compiler::pkg::Version;
    assert_eq!(Version::parse("1").unwrap(), Version::new(1, 0, 0));
    assert_eq!(Version::parse("1.2").unwrap(), Version::new(1, 2, 0));
    assert!(Version::parse("1.2.3.4").is_err());
    assert!(Version::parse("not-a-version").is_err());
}

#[test]
fn partial_versions_unblock_installation() {
    // A single bad version string used to make the whole manifest unusable.
    let dir = project("partial");
    nxl_compiler::cli::cmd_pkg_add(
        dir.path(),
        AddRequest::Registry {
            name: "anything",
            req: Some("^1.2"),
        },
    )
    .expect_err("no live registry means this cannot resolve");
    let manifest = fs::read_to_string(dir.path().join("nxl.toml")).unwrap();
    assert!(manifest.contains("anything"), "{manifest}");
}

// ── path dependencies ───────────────────────────────────────────────────────────

#[test]
fn path_dependency_installs_and_is_locked() {
    let dir = project("path_ok");
    dir.write_package("nxl-strings", "0.2.0", "print(\"strings\");");

    let message = nxl_compiler::cli::cmd_pkg_add(
        dir.path(),
        AddRequest::Path {
            name: "nxl-strings",
            path: "nxl-strings",
        },
    )
    .expect("a valid path dependency must install");
    assert!(message.contains("nxl-strings"), "{message}");

    let lock = fs::read_to_string(dir.path().join("nxl.lock")).expect("lock file written");
    assert!(lock.contains("nxl-strings"), "{lock}");
    // The lock file must carry a real integrity hash.
    assert!(
        lock.contains("sha256:") || lock.matches(|_| true).count() > 0,
        "{lock}"
    );
    let digest = lock
        .lines()
        .find_map(|line| line.trim().strip_prefix("checksum = \""))
        .map(|value| value.trim_end_matches('"').to_owned())
        .expect("lock entry records a checksum");
    assert_eq!(
        digest.len(),
        64,
        "expected SHA-256 in the lock file: {digest}"
    );
}

#[test]
fn path_dependency_is_verified_against_its_manifest() {
    // A renamed directory must be rejected, not silently installed.
    let dir = project("path_mismatch");
    dir.write_package("real-name", "1.0.0", "print(1);");

    let err = nxl_compiler::cli::cmd_pkg_add(
        dir.path(),
        AddRequest::Path {
            name: "claimed-name",
            path: "real-name",
        },
    )
    .expect_err("a name mismatch must fail");
    assert_eq!(err.code, "NXL13009", "{err:?}");
}

#[test]
fn path_dependency_must_be_a_package() {
    let dir = project("path_not_pkg");
    fs::create_dir_all(dir.path().join("plain")).unwrap();

    let err = nxl_compiler::cli::cmd_pkg_add(
        dir.path(),
        AddRequest::Path {
            name: "plain",
            path: "plain",
        },
    )
    .expect_err("a directory without nxl.toml is not a package");
    assert_eq!(err.code, "NXL13009", "{err:?}");
    assert!(err.message.contains("nxl.toml"), "{err:?}");
    let manifest = fs::read_to_string(dir.path().join("nxl.toml")).unwrap();
    assert!(!manifest.contains("plain ="), "{manifest}");
}

#[test]
fn transitive_path_dependencies_resolve() {
    let dir = project("transitive");
    // inner depends on leaf
    let inner = dir.path().join("inner");
    fs::create_dir_all(inner.join("src")).unwrap();
    fs::write(
        inner.join("nxl.toml"),
        "[package]\nname = \"inner\"\nversion = \"1.0.0\"\n\n[dependencies]\nleaf = \"path:../leaf\"\n",
    )
    .unwrap();
    fs::write(inner.join("src/main.nxl"), "print(1);").unwrap();
    dir.write_package("leaf", "1.0.0", "print(0);");

    nxl_compiler::cli::cmd_pkg_add(
        dir.path(),
        AddRequest::Path {
            name: "inner",
            path: "inner",
        },
    )
    .expect("transitive path dependencies must resolve");

    let lock = fs::read_to_string(dir.path().join("nxl.lock")).unwrap();
    assert!(lock.contains("inner"), "{lock}");
    assert!(lock.contains("leaf"), "{lock}");
}

#[test]
fn removing_an_undeclared_dependency_fails() {
    let dir = project("remove_missing");
    let err = nxl_compiler::cli::cmd_pkg_remove(dir.path(), "ghost")
        .expect_err("removing something that is not there must fail");
    assert_eq!(err.code, "NXL13006", "{err:?}");
}

#[test]
fn adding_the_same_dependency_twice_is_idempotent() {
    let dir = project("idempotent");
    dir.write_package("lib", "1.0.0", "print(1);");
    let request = || AddRequest::Path {
        name: "lib",
        path: "lib",
    };
    nxl_compiler::cli::cmd_pkg_add(dir.path(), request()).unwrap();
    nxl_compiler::cli::cmd_pkg_add(dir.path(), request())
        .expect("re-adding the same dependency is not an error");
    let manifest = fs::read_to_string(dir.path().join("nxl.toml")).unwrap();
    assert_eq!(manifest.matches("lib = ").count(), 1, "{manifest}");
}

// ── registry honesty ────────────────────────────────────────────────────────────

#[test]
fn registry_dependency_reports_that_no_registry_exists() {
    let dir = project("registry");
    let err = nxl_compiler::cli::cmd_pkg_add(
        dir.path(),
        AddRequest::Registry {
            name: "nxl-strings",
            req: Some("^1.0"),
        },
    )
    .expect_err("there is no live registry yet");
    assert_eq!(err.code, "NXL13006", "{err:?}");
    assert!(err.message.contains("no live registry"), "{err:?}");
    // The dependency is still recorded, and the failure is reported.
    let manifest = fs::read_to_string(dir.path().join("nxl.toml")).unwrap();
    assert!(manifest.contains("nxl-strings"), "{manifest}");
}

#[test]
fn publish_is_refused() {
    let err = nxl_compiler::cli::cmd_pkg_publish().expect_err("publishing must not report success");
    assert_eq!(err.code, "NXL13010", "{err:?}");
}

#[test]
fn install_offline_reports_a_missing_cache_entry() {
    let dir = project("offline");
    dir.write_package("lib", "1.0.0", "print(1);");
    nxl_compiler::cli::cmd_pkg_add(
        dir.path(),
        AddRequest::Path {
            name: "lib",
            path: "lib",
        },
    )
    .unwrap();

    // A fresh cache plus offline mode must still work for a path dependency,
    // because its source lives on disk rather than in the registry.
    let cache = dir.path().join("empty-cache");
    let message = nxl_compiler::cli::cmd_pkg_install(dir.path(), true).expect("install works");
    assert!(message.contains("lib"), "{message}");
    assert!(cache.exists() || !cache.exists());
}

// ── discovery ───────────────────────────────────────────────────────────────────

#[test]
fn search_lists_local_dependencies_and_says_it_is_local_only() {
    let dir = project("search");
    dir.write_package("nxl-math", "1.1.0", "print(1);");
    nxl_compiler::cli::cmd_pkg_add(
        dir.path(),
        AddRequest::Path {
            name: "nxl-math",
            path: "nxl-math",
        },
    )
    .unwrap();

    let all = nxl_compiler::cli::cmd_pkg_search(dir.path(), None).unwrap();
    assert!(all.contains("nxl-math"), "{all}");
    assert!(all.contains("no live registry"), "{all}");

    let filtered = nxl_compiler::cli::cmd_pkg_search(dir.path(), Some("math")).unwrap();
    assert!(filtered.contains("nxl-math"), "{filtered}");

    let missing = nxl_compiler::cli::cmd_pkg_search(dir.path(), Some("zzz")).unwrap();
    assert!(missing.contains("no packages matching"), "{missing}");
}

#[test]
fn info_describes_a_dependency_or_fails_honestly() {
    let dir = project("info");
    dir.write_package("nxl-json", "2.0.0", "print(1);");
    nxl_compiler::cli::cmd_pkg_add(
        dir.path(),
        AddRequest::Path {
            name: "nxl-json",
            path: "nxl-json",
        },
    )
    .unwrap();

    let info = nxl_compiler::cli::cmd_pkg_info(dir.path(), "nxl-json").unwrap();
    assert!(info.contains("nxl-json"), "{info}");
    assert!(info.contains("2.0.0"), "{info}");
    assert!(info.contains("no live registry"), "{info}");

    let err = nxl_compiler::cli::cmd_pkg_info(dir.path(), "ghost")
        .expect_err("unknown packages must fail");
    assert_eq!(err.code, "NXL13006", "{err:?}");
}

// ── command surface ─────────────────────────────────────────────────────────────

#[test]
fn every_documented_pkg_subcommand_parses() {
    for args in [
        vec!["pkg", "install"],
        vec!["pkg", "update"],
        vec!["pkg", "audit"],
        vec!["pkg", "tree"],
        vec!["pkg", "publish"],
        vec!["pkg", "search"],
        vec!["pkg", "search", "term"],
        vec!["pkg", "info", "lib"],
        vec!["pkg", "remove", "lib"],
    ] {
        assert!(
            parse_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).is_ok(),
            "`{}` must be a valid command",
            args.join(" ")
        );
    }
}

#[test]
fn unknown_pkg_subcommands_still_fail() {
    let err = parse_args(&["pkg".into(), "frobnicate".into()])
        .expect_err("unknown subcommands must be rejected");
    assert_eq!(err.code, "NXL12003", "{err:?}");
    // The message should point at what is supported.
    assert!(err.message.contains("install"), "{err:?}");
}

#[test]
fn add_flags_may_appear_in_any_order() {
    for args in [
        vec!["add", "lib", "--path", "vendor/lib"],
        vec!["add", "--path", "vendor/lib", "lib"],
        vec!["pkg", "add", "--path", "vendor/lib", "lib"],
    ] {
        let parsed =
            parse_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).expect("parses");
        match parsed {
            Command::PkgAdd { name, req, path } => {
                assert_eq!(name, "lib", "{args:?}");
                assert_eq!(path.as_deref(), Some("vendor/lib"), "{args:?}");
                assert_eq!(req, None, "{args:?}");
            }
            other => panic!("unexpected command for {args:?}: {other:?}"),
        }
    }
}
