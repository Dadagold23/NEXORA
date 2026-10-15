//! Publishing to, and installing from, the local registry.

use nxl_compiler::cli::{
    cmd_pkg_add, cmd_pkg_install_in, cmd_pkg_publish_to, cmd_pkg_search_in, AddRequest,
};
use nxl_compiler::pkg::LocalRegistry;
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
            std::env::temp_dir().join(format!("nxl_pub_{tag}_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A library package on disk.
fn library(root: &Path, name: &str, version: &str, body: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("nxl.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"{version}\"\ndescription = \"a test package\"\nentry = \"src/main.nxl\"\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("src/main.nxl"), body).unwrap();
    dir
}

/// An application project that consumes packages.
fn application(root: &Path, name: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("nxl.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"src/main.nxl\"\n"),
    )
    .unwrap();
    fs::write(dir.join("src/main.nxl"), "fn main() { print(\"app\"); }").unwrap();
    dir
}

// ── publishing ─────────────────────────────────────────────────────────────────

#[test]
fn publishing_writes_an_inspectable_package() {
    let dir = TempDir::new("publish");
    let registry = dir.path().join("registry");
    library(
        dir.path(),
        "mylib",
        "1.2.3",
        "fn greet() -> String { return \"hi\"; }",
    );

    let message = cmd_pkg_publish_to(&dir.path().join("mylib"), &registry).expect("publishes");
    assert!(message.contains("mylib"), "{message}");
    assert!(message.contains("1.2.3"), "{message}");
    assert!(message.contains("sha256:"), "{message}");

    let published = registry.join("mylib/1.2.3");
    assert!(published.join("package.json").exists(), "metadata missing");
    assert!(published.join("src/main.nxl").exists(), "sources missing");
    assert!(
        published.join("checksums.sha256").exists(),
        "checksums missing"
    );
    assert!(registry.join("index.json").exists(), "index missing");

    let metadata = fs::read_to_string(published.join("package.json")).unwrap();
    assert!(metadata.contains("\"name\": \"mylib\""), "{metadata}");
    assert!(metadata.contains("\"version\": \"1.2.3\""), "{metadata}");
    assert!(
        metadata.contains("\"description\": \"a test package\""),
        "{metadata}"
    );

    let index = fs::read_to_string(registry.join("index.json")).unwrap();
    assert!(index.contains("\"mylib\""), "{index}");
}

#[test]
fn published_sources_are_the_real_files() {
    let dir = TempDir::new("sources");
    let registry = dir.path().join("registry");
    library(
        dir.path(),
        "mathlib",
        "0.1.0",
        "fn double(n: Int) -> Int { return n * 2; }",
    );
    cmd_pkg_publish_to(&dir.path().join("mathlib"), &registry).expect("publishes");

    let local = LocalRegistry::new(&registry);
    let package = local.list().into_iter().next().expect("one package");
    let sources = local.read_sources(&package).expect("sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].0, "src/main.nxl");
    assert!(sources[0].1.contains("double"), "{}", sources[0].1);
}

#[test]
fn the_published_checksum_changes_with_the_source() {
    let dir = TempDir::new("checksum");
    let registry = dir.path().join("registry");
    let lib = library(
        dir.path(),
        "changing",
        "0.1.0",
        "fn one() -> Int { return 1; }",
    );
    let first = cmd_pkg_publish_to(&lib, &registry).expect("publishes");
    fs::write(lib.join("src/main.nxl"), "fn one() -> Int { return 2; }").unwrap();
    let second = cmd_pkg_publish_to(&lib, &registry).expect("republishes");

    let extract = |message: &str| {
        message
            .lines()
            .find_map(|line| line.trim().strip_prefix("checksum  : sha256:"))
            .map(|value| value.trim().to_owned())
            .expect("checksum in the report")
    };
    assert_ne!(
        extract(&first),
        extract(&second),
        "editing a package must change its checksum"
    );
}

#[test]
fn publishing_something_without_sources_is_refused() {
    let dir = TempDir::new("no_sources");
    let registry = dir.path().join("registry");
    let empty = dir.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    fs::write(empty.join("nxl.toml"), "[package]\nname = \"empty\"\n").unwrap();

    let error = cmd_pkg_publish_to(&empty, &registry).expect_err("nothing to publish");
    assert_eq!(error.code, "NXL13011", "{error:?}");
    assert!(error.message.contains("src/*.nxl"), "{error:?}");
}

#[test]
fn publishing_outside_a_project_is_refused() {
    let dir = TempDir::new("no_manifest");
    let error =
        cmd_pkg_publish_to(dir.path(), &dir.path().join("registry")).expect_err("no manifest");
    assert_eq!(error.code, "NXL13011", "{error:?}");
}

// ── consuming a published package ──────────────────────────────────────────────

#[test]
fn a_published_package_can_be_added_and_installed() {
    let dir = TempDir::new("roundtrip");
    let registry = dir.path().join("registry");
    library(
        dir.path(),
        "mylib",
        "1.0.0",
        "fn greet() -> String { return \"hi\"; }",
    );
    cmd_pkg_publish_to(&dir.path().join("mylib"), &registry).expect("publishes");

    let app = application(dir.path(), "consumer");
    let message = cmd_pkg_add(
        &app,
        AddRequest::Registry {
            name: "mylib",
            req: Some("^1.0"),
            registry: Some(registry.to_str().unwrap()),
        },
    )
    .expect("adds from the registry");
    assert!(message.contains("mylib"), "{message}");

    let manifest = fs::read_to_string(app.join("nxl.toml")).unwrap();
    assert!(manifest.contains("mylib"), "{manifest}");

    let installed = cmd_pkg_install_in(&app, false, Some(&registry)).expect("installs");
    assert!(installed.contains("mylib@1.0.0"), "{installed}");

    // The lock file records the resolved version and a real SHA-256.
    let lock = fs::read_to_string(app.join("nxl.lock")).expect("lock written");
    assert!(lock.contains("name = \"mylib\""), "{lock}");
    assert!(lock.contains("version = \"1.0.0\""), "{lock}");
    assert!(lock.contains("source = \"registry\""), "{lock}");
    let checksum = lock
        .lines()
        .find_map(|line| line.trim().strip_prefix("checksum = \""))
        .map(|value| value.trim_end_matches('"').to_owned())
        .expect("checksum recorded");
    assert_eq!(checksum.len(), 64, "expected SHA-256: {checksum}");

    // The cached source is the published source, not a placeholder.
    let cached = fs::read_to_string(app.join(".nxl-cache/mylib/1.0.0/source.nxl")).expect("cached");
    assert!(cached.contains("greet"), "{cached}");
    assert!(
        !cached.contains("stub source"),
        "must be real source: {cached}"
    );
}

#[test]
fn adding_an_unpublished_package_fails_with_a_reason() {
    let dir = TempDir::new("missing_pkg");
    let registry = dir.path().join("registry");
    fs::create_dir_all(&registry).unwrap();
    let app = application(dir.path(), "consumer");

    let error = cmd_pkg_add(
        &app,
        AddRequest::Registry {
            name: "ghost",
            req: Some("^1.0"),
            registry: Some(registry.to_str().unwrap()),
        },
    )
    .expect_err("an empty registry cannot satisfy anything");
    assert_eq!(error.code, "NXL13006", "{error:?}");
    assert!(error.message.contains("empty"), "{error:?}");

    // Nothing may be written for a dependency that does not exist.
    let manifest = fs::read_to_string(app.join("nxl.toml")).unwrap();
    assert!(!manifest.contains("ghost"), "{manifest}");
}

#[test]
fn adding_from_a_missing_registry_points_at_publish() {
    let dir = TempDir::new("no_registry");
    let registry = dir.path().join("not-there");
    let app = application(dir.path(), "consumer");

    let error = cmd_pkg_add(
        &app,
        AddRequest::Registry {
            name: "mylib",
            req: None,
            registry: Some(registry.to_str().unwrap()),
        },
    )
    .expect_err("no registry directory");
    assert_eq!(error.code, "NXL13006", "{error:?}");
    assert!(error.message.contains("pkg publish"), "{error:?}");
}

#[test]
fn a_version_outside_the_requirement_is_refused() {
    let dir = TempDir::new("mismatch");
    let registry = dir.path().join("registry");
    library(
        dir.path(),
        "mylib",
        "2.0.0",
        "fn greet() -> String { return \"hi\"; }",
    );
    cmd_pkg_publish_to(&dir.path().join("mylib"), &registry).expect("publishes");

    let app = application(dir.path(), "consumer");
    let error = cmd_pkg_add(
        &app,
        AddRequest::Registry {
            name: "mylib",
            req: Some("^1.0"),
            registry: Some(registry.to_str().unwrap()),
        },
    )
    .expect_err("2.0.0 does not satisfy ^1.0");
    assert_eq!(error.code, "NXL13006", "{error:?}");
    assert!(
        error.message.contains("2.0.0"),
        "the available version helps: {error:?}"
    );
}

// ── discovery ─────────────────────────────────────────────────────────────────

#[test]
fn search_includes_registry_packages() {
    let dir = TempDir::new("search");
    let registry = dir.path().join("registry");
    library(
        dir.path(),
        "nxl-json",
        "1.0.0",
        "fn parse() -> Int { return 1; }",
    );
    cmd_pkg_publish_to(&dir.path().join("nxl-json"), &registry).expect("publishes");

    let app = application(dir.path(), "consumer");
    let all = cmd_pkg_search_in(&app, None, Some(&registry)).expect("search");
    assert!(all.contains("nxl-json"), "{all}");
    assert!(all.contains("no hosted"), "it must stay honest: {all}");

    let filtered = cmd_pkg_search_in(&app, Some("json"), Some(&registry)).expect("search");
    assert!(filtered.contains("nxl-json"), "{filtered}");

    let nothing = cmd_pkg_search_in(&app, Some("zzz"), Some(&registry)).expect("search");
    assert!(nothing.contains("no packages matching"), "{nothing}");
}

#[test]
fn the_registry_lists_every_published_version() {
    let dir = TempDir::new("versions");
    let registry = dir.path().join("registry");
    library(dir.path(), "mylib", "1.0.0", "fn a() -> Int { return 1; }");
    cmd_pkg_publish_to(&dir.path().join("mylib"), &registry).expect("publishes 1.0.0");

    // Publishing a different version keeps the first one.
    let updated = dir.path().join("mylib");
    fs::write(
        updated.join("nxl.toml"),
        "[package]\nname = \"mylib\"\nversion = \"2.0.0\"\n",
    )
    .unwrap();
    cmd_pkg_publish_to(&updated, &registry).expect("publishes 2.0.0");

    let local = LocalRegistry::new(&registry);
    let names: Vec<String> = local
        .list()
        .iter()
        .map(|p| format!("{} {}", p.name, p.version))
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"mylib 1.0.0".to_owned()), "{names:?}");
    assert!(names.contains(&"mylib 2.0.0".to_owned()), "{names:?}");
}
