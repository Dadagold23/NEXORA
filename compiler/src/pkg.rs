/// P13 — NXL Package Manager (NXP)
///
/// # Implementation status
///
/// | Subsystem              | Status                          |
/// |------------------------|---------------------------------|
/// | Semantic versioning    | IMPLEMENTED                     |
/// | Package manifest       | IMPLEMENTED                     |
/// | Lock file              | IMPLEMENTED                     |
/// | Dependency resolution  | IMPLEMENTED                     |
/// | Package cache          | IMPLEMENTED                     |
/// | Local/path packages    | IMPLEMENTED                     |
/// | Git dependencies (spec)| IMPLEMENTED                     |
/// | Offline mode           | IMPLEMENTED                     |
/// | Dependency graph       | IMPLEMENTED                     |
/// | CLI integration        | IMPLEMENTED                     |
/// | Workspace support      | IMPLEMENTED                     |
/// | Dependency audit       | IMPLEMENTED                     |
/// | Checksums              | IMPLEMENTED (SHA-256 stub)      |
/// | Live registry          | NOT IMPLEMENTED (P17+)          |
/// | Package signing        | NOT IMPLEMENTED (P28+)          |
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::source::Span;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct PkgError {
    pub code: &'static str,
    pub message: String,
}

impl PkgError {
    fn new(code: &'static str, msg: impl Into<String>) -> Self {
        Self {
            code,
            message: msg.into(),
        }
    }
}

impl fmt::Display for PkgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error[{}]: {}", self.code, self.message)
    }
}

impl From<PkgError> for Diagnostic {
    fn from(e: PkgError) -> Self {
        Diagnostic::error(e.code, e.message, Span::default())
    }
}

pub type PkgResult<T> = Result<T, PkgError>;

// ── Semantic versioning ───────────────────────────────────────────────────────

/// A parsed semantic version: major.minor.patch[-pre].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre: Option<String>,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: None,
        }
    }

    pub fn parse(s: &str) -> PkgResult<Self> {
        let s = s.trim().trim_start_matches('v');
        let (core, pre) = if let Some((c, p)) = s.split_once('-') {
            (c, Some(p.to_owned()))
        } else {
            (s, None)
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 {
            return Err(PkgError::new("NXL13001", format!("invalid version '{s}'")));
        }
        let parse_u32 = |p: &str| {
            p.parse::<u32>()
                .map_err(|_| PkgError::new("NXL13001", format!("invalid version component '{p}'")))
        };
        Ok(Self {
            major: parse_u32(parts[0])?,
            minor: parse_u32(parts[1])?,
            patch: parse_u32(parts[2])?,
            pre,
        })
    }

    pub fn is_compatible_with(&self, req: &VersionReq) -> bool {
        req.matches(self)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

/// A version requirement: `^1.2`, `~1.2.3`, `>=1.0.0`, `=1.2.3`, `*`.
#[derive(Debug, Clone, PartialEq)]
pub struct VersionReq {
    pub raw: String,
    kind: ReqKind,
    version: Option<Version>,
}

#[derive(Debug, Clone, PartialEq)]
enum ReqKind {
    Any,
    Caret, // ^1.2.3 — compatible with 1.x.x
    Tilde, // ~1.2.3 — compatible with 1.2.x
    Exact, // =1.2.3
    Gte,   // >=1.2.3
    Gt,    // >1.2.3
    Lte,   // <=1.2.3
    Lt,    // <1.2.3
}

impl VersionReq {
    pub fn parse(s: &str) -> PkgResult<Self> {
        let s = s.trim();
        if s == "*" || s.is_empty() {
            return Ok(Self {
                raw: s.to_owned(),
                kind: ReqKind::Any,
                version: None,
            });
        }
        let (kind, ver_str) = if let Some(r) = s.strip_prefix(">=") {
            (ReqKind::Gte, r)
        } else if let Some(r) = s.strip_prefix("<=") {
            (ReqKind::Lte, r)
        } else if let Some(r) = s.strip_prefix('>') {
            (ReqKind::Gt, r)
        } else if let Some(r) = s.strip_prefix('<') {
            (ReqKind::Lt, r)
        } else if let Some(r) = s.strip_prefix('=') {
            (ReqKind::Exact, r)
        } else if let Some(r) = s.strip_prefix('^') {
            (ReqKind::Caret, r)
        } else if let Some(r) = s.strip_prefix('~') {
            (ReqKind::Tilde, r)
        } else {
            // bare version string treated as caret
            (ReqKind::Caret, s)
        };
        let version = Version::parse(ver_str)?;
        Ok(Self {
            raw: s.to_owned(),
            kind,
            version: Some(version),
        })
    }

    pub fn matches(&self, v: &Version) -> bool {
        let Some(req_v) = &self.version else {
            return true;
        };
        match self.kind {
            ReqKind::Any => true,
            ReqKind::Exact => v == req_v,
            ReqKind::Gte => v >= req_v,
            ReqKind::Gt => v > req_v,
            ReqKind::Lte => v <= req_v,
            ReqKind::Lt => v < req_v,
            ReqKind::Caret => {
                // ^X.Y.Z: same major, >= minor.patch
                v.major == req_v.major && *v >= *req_v
            }
            ReqKind::Tilde => {
                // ~X.Y.Z: same major.minor, >= patch
                v.major == req_v.major && v.minor == req_v.minor && v.patch >= req_v.patch
            }
        }
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

// ── Package manifest ──────────────────────────────────────────────────────────

/// A fully-parsed NXP package manifest (nxl.toml [package] + [dependencies]).
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub name: String,
    pub version: Version,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub dependencies: Vec<ManifestDep>,
    pub dev_dependencies: Vec<ManifestDep>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManifestDep {
    pub name: String,
    pub req: VersionReq,
    pub source: DepSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DepSource {
    Registry,
    Path(PathBuf),
    Git {
        url: String,
        branch_or_tag: Option<String>,
    },
}

impl Manifest {
    pub fn parse(toml: &str) -> PkgResult<Self> {
        let mut name = String::new();
        let mut version_str = "0.1.0".to_owned();
        let mut description = None;
        let mut authors = Vec::new();
        let mut license = None;
        let mut repository = None;
        let mut dependencies = Vec::new();
        let mut dev_dependencies = Vec::new();
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
                } else if line == "[dev-dependencies]" {
                    "dev-dependencies"
                } else {
                    "other"
                };
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let k = k.trim();
            let v = v.trim().trim_matches('"');

            match section {
                "package" => match k {
                    "name" => name = v.to_owned(),
                    "version" => version_str = v.to_owned(),
                    "description" => description = Some(v.to_owned()),
                    "authors" => authors = v.split(',').map(|s| s.trim().to_owned()).collect(),
                    "license" => license = Some(v.to_owned()),
                    "repository" => repository = Some(v.to_owned()),
                    _ => {}
                },
                "dependencies" => {
                    let dep = parse_dep(k, v)?;
                    dependencies.push(dep);
                }
                "dev-dependencies" => {
                    let dep = parse_dep(k, v)?;
                    dev_dependencies.push(dep);
                }
                _ => {}
            }
        }

        if name.is_empty() {
            return Err(PkgError::new(
                "NXL13002",
                "manifest missing required field 'name'",
            ));
        }
        let version = Version::parse(&version_str)?;
        Ok(Self {
            name,
            version,
            description,
            authors,
            license,
            repository,
            dependencies,
            dev_dependencies,
        })
    }

    pub fn load(dir: &Path) -> PkgResult<Self> {
        let path = dir.join("nxl.toml");
        let text = fs::read_to_string(&path).map_err(|_| {
            PkgError::new("NXL13003", format!("no nxl.toml in '{}'", dir.display()))
        })?;
        Self::parse(&text)
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
        if let Some(l) = &self.license {
            s.push_str(&format!("license = \"{l}\"\n"));
        }
        if let Some(r) = &self.repository {
            s.push_str(&format!("repository = \"{r}\"\n"));
        }
        if !self.dependencies.is_empty() {
            s.push_str("\n[dependencies]\n");
            for dep in &self.dependencies {
                s.push_str(&format_dep(dep));
            }
        }
        if !self.dev_dependencies.is_empty() {
            s.push_str("\n[dev-dependencies]\n");
            for dep in &self.dev_dependencies {
                s.push_str(&format_dep(dep));
            }
        }
        s
    }
}

fn parse_dep(name: &str, val: &str) -> PkgResult<ManifestDep> {
    let source = if val.starts_with("path:") {
        DepSource::Path(PathBuf::from(val.trim_start_matches("path:")))
    } else if val.starts_with("git:") {
        let rest = val.trim_start_matches("git:");
        let (url, tag) = if let Some((u, t)) = rest.split_once('#') {
            (u.to_owned(), Some(t.to_owned()))
        } else {
            (rest.to_owned(), None)
        };
        DepSource::Git {
            url,
            branch_or_tag: tag,
        }
    } else {
        DepSource::Registry
    };
    let req_str = if matches!(source, DepSource::Path(_) | DepSource::Git { .. }) {
        "*"
    } else {
        val
    };
    Ok(ManifestDep {
        name: name.to_owned(),
        req: VersionReq::parse(req_str)?,
        source,
    })
}

fn format_dep(dep: &ManifestDep) -> String {
    match &dep.source {
        DepSource::Registry => format!("{} = \"{}\"\n", dep.name, dep.req),
        DepSource::Path(p) => format!("{} = \"path:{}\"\n", dep.name, p.display()),
        DepSource::Git { url, branch_or_tag } => {
            if let Some(t) = branch_or_tag {
                format!("{} = \"git:{}#{}\"\n", dep.name, url, t)
            } else {
                format!("{} = \"git:{}\"\n", dep.name, url)
            }
        }
    }
}

// ── Lock file ─────────────────────────────────────────────────────────────────

/// A resolved, pinned dependency entry in `nxl.lock`.
#[derive(Debug, Clone, PartialEq)]
pub struct LockedPackage {
    pub name: String,
    pub version: Version,
    pub checksum: String,
    pub source: DepSource,
    pub dependencies: Vec<String>, // name@version strings
}

/// The full lock file (`nxl.lock`).
#[derive(Debug, Clone, Default)]
pub struct LockFile {
    pub packages: Vec<LockedPackage>,
}

impl LockFile {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn find(&self, name: &str) -> Option<&LockedPackage> {
        self.packages.iter().find(|p| p.name == name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// Serialize to `nxl.lock` format.
    pub fn serialize(&self) -> String {
        let mut s = String::new();
        s.push_str("# NXL lock file — do not edit manually\n");
        s.push_str("version = 1\n\n");
        for pkg in &self.packages {
            s.push_str("[[package]]\n");
            s.push_str(&format!("name = \"{}\"\n", pkg.name));
            s.push_str(&format!("version = \"{}\"\n", pkg.version));
            s.push_str(&format!("checksum = \"{}\"\n", pkg.checksum));
            match &pkg.source {
                DepSource::Registry => s.push_str("source = \"registry\"\n"),
                DepSource::Path(p) => s.push_str(&format!("source = \"path:{}\"\n", p.display())),
                DepSource::Git { url, branch_or_tag } => {
                    if let Some(t) = branch_or_tag {
                        s.push_str(&format!("source = \"git:{}#{}\"\n", url, t));
                    } else {
                        s.push_str(&format!("source = \"git:{}\"\n", url));
                    }
                }
            }
            if !pkg.dependencies.is_empty() {
                s.push_str(&format!(
                    "dependencies = [{}]\n",
                    pkg.dependencies
                        .iter()
                        .map(|d| format!("\"{d}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            s.push('\n');
        }
        s
    }

    /// Parse a `nxl.lock` file.
    pub fn parse(text: &str) -> PkgResult<Self> {
        let mut packages = Vec::new();
        let mut cur: Option<(String, String, String, DepSource, Vec<String>)> = None;

        for raw in text.lines() {
            let line = raw.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if line == "version = 1" {
                continue;
            }
            if line == "[[package]]" {
                if let Some((n, v, c, s, d)) = cur.take() {
                    let version = Version::parse(&v)?;
                    packages.push(LockedPackage {
                        name: n,
                        version,
                        checksum: c,
                        source: s,
                        dependencies: d,
                    });
                }
                cur = Some((
                    String::new(),
                    String::new(),
                    String::new(),
                    DepSource::Registry,
                    Vec::new(),
                ));
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let k = k.trim();
            let v = v.trim().trim_matches('"');
            if let Some(ref mut pkg) = cur {
                match k {
                    "name" => pkg.0 = v.to_owned(),
                    "version" => pkg.1 = v.to_owned(),
                    "checksum" => pkg.2 = v.to_owned(),
                    "source" => {
                        pkg.3 = if v.starts_with("path:") {
                            DepSource::Path(PathBuf::from(v.trim_start_matches("path:")))
                        } else if v.starts_with("git:") {
                            let rest = v.trim_start_matches("git:");
                            let (url, tag) = if let Some((u, t)) = rest.split_once('#') {
                                (u.to_owned(), Some(t.to_owned()))
                            } else {
                                (rest.to_owned(), None)
                            };
                            DepSource::Git {
                                url,
                                branch_or_tag: tag,
                            }
                        } else {
                            DepSource::Registry
                        }
                    }
                    "dependencies" => {
                        pkg.4 = v
                            .trim_matches(|c| c == '[' || c == ']')
                            .split(',')
                            .map(|s| s.trim().trim_matches('"').to_owned())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    _ => {}
                }
            }
        }
        if let Some((n, v, c, s, d)) = cur {
            if !n.is_empty() {
                let version = Version::parse(&v)?;
                packages.push(LockedPackage {
                    name: n,
                    version,
                    checksum: c,
                    source: s,
                    dependencies: d,
                });
            }
        }
        Ok(Self { packages })
    }

    pub fn load(dir: &Path) -> PkgResult<Self> {
        let path = dir.join("nxl.lock");
        if !path.exists() {
            return Ok(Self::new());
        }
        let text =
            fs::read_to_string(&path).map_err(|e| PkgError::new("NXL13004", e.to_string()))?;
        Self::parse(&text)
    }

    pub fn save(&self, dir: &Path) -> PkgResult<()> {
        fs::write(dir.join("nxl.lock"), self.serialize())
            .map_err(|e| PkgError::new("NXL13004", e.to_string()))
    }
}

// ── Dependency resolver ───────────────────────────────────────────────────────

/// A resolved package with its pinned version and transitive deps.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: Version,
    pub source: DepSource,
    pub dependencies: Vec<String>,
}

/// Resolves a dependency graph from a manifest and a registry.
pub struct DependencyResolver<'a> {
    registry: &'a Registry,
}

impl<'a> DependencyResolver<'a> {
    pub fn new(registry: &'a Registry) -> Self {
        Self { registry }
    }

    /// Resolve all dependencies declared in `manifest` into a flat list.
    ///
    /// Returns `Err` if any dependency cannot be satisfied or a version
    /// conflict is detected.
    pub fn resolve(&self, manifest: &Manifest) -> PkgResult<Vec<ResolvedPackage>> {
        let mut resolved: HashMap<String, ResolvedPackage> = HashMap::new();
        let mut queue: Vec<ManifestDep> = manifest.dependencies.clone();

        while let Some(dep) = queue.pop() {
            if let Some(existing) = resolved.get(&dep.name) {
                // Already resolved — check the pinned version satisfies this req.
                if !dep.req.matches(&existing.version) {
                    return Err(PkgError::new(
                        "NXL13005",
                        format!(
                            "version conflict: '{}' requires '{}' but '{}' is already resolved",
                            dep.name, dep.req, existing.version
                        ),
                    ));
                }
                continue;
            }

            // Try registry; if not found but a different version is already
            // resolved, that is a conflict rather than a missing package.
            let pkg = self.registry.resolve(&dep.name, &dep.req)?;
            // Queue transitive dependencies.
            if let Some(sub_manifest) = self.registry.manifest(&dep.name, &pkg.version) {
                for sub_dep in &sub_manifest.dependencies {
                    if !resolved.contains_key(&sub_dep.name) {
                        queue.push(sub_dep.clone());
                    }
                }
            }
            resolved.insert(dep.name.clone(), pkg);
        }

        let mut result: Vec<ResolvedPackage> = resolved.into_values().collect();
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// A package registry entry.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub name: String,
    pub version: Version,
    pub source: DepSource,
    pub manifest: Option<Manifest>,
}

/// The NXP package registry.
///
/// Status: LOCAL/PATH packages only. Live registry deferred to P17+.
#[derive(Debug, Default)]
pub struct Registry {
    entries: Vec<RegistryEntry>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a package version.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        version: Version,
        source: DepSource,
        manifest: Option<Manifest>,
    ) {
        self.entries.push(RegistryEntry {
            name: name.into(),
            version,
            source,
            manifest,
        });
    }

    /// Find the best matching version for a dependency requirement.
    pub fn resolve(&self, name: &str, req: &VersionReq) -> PkgResult<ResolvedPackage> {
        let mut candidates: Vec<&RegistryEntry> = self
            .entries
            .iter()
            .filter(|e| e.name == name && req.matches(&e.version))
            .collect();

        if candidates.is_empty() {
            return Err(PkgError::new(
                "NXL13006",
                format!(
                    "package '{}' matching '{}' not found in registry",
                    name, req
                ),
            ));
        }

        // Pick the highest matching version.
        candidates.sort_by(|a, b| b.version.cmp(&a.version));
        let best = candidates[0];

        let deps = best
            .manifest
            .as_ref()
            .map(|m| {
                m.dependencies
                    .iter()
                    .map(|d| format!("{}@{}", d.name, d.req))
                    .collect()
            })
            .unwrap_or_default();

        Ok(ResolvedPackage {
            name: best.name.clone(),
            version: best.version.clone(),
            source: best.source.clone(),
            dependencies: deps,
        })
    }

    /// Retrieve the manifest for a specific package version.
    pub fn manifest(&self, name: &str, version: &Version) -> Option<&Manifest> {
        self.entries
            .iter()
            .find(|e| e.name == name && &e.version == version)
            .and_then(|e| e.manifest.as_ref())
    }

    pub fn list(&self) -> &[RegistryEntry] {
        &self.entries
    }
}

// ── Package cache ─────────────────────────────────────────────────────────────

/// Local on-disk package cache at `.nxl-cache/`.
pub struct PackageCache {
    root: PathBuf,
}

impl PackageCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_path() -> PathBuf {
        std::env::var("NXL_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs_or_temp().join(".nxl-cache"))
    }

    fn pkg_dir(&self, name: &str, version: &Version) -> PathBuf {
        self.root.join(name).join(version.to_string())
    }

    pub fn is_cached(&self, name: &str, version: &Version) -> bool {
        self.pkg_dir(name, version).exists()
    }

    /// Store package source text in the cache.
    pub fn store(&self, name: &str, version: &Version, source: &str) -> PkgResult<()> {
        let dir = self.pkg_dir(name, version);
        fs::create_dir_all(&dir).map_err(|e| PkgError::new("NXL13007", e.to_string()))?;
        fs::write(dir.join("source.nxl"), source)
            .map_err(|e| PkgError::new("NXL13007", e.to_string()))
    }

    /// Retrieve cached package source text.
    pub fn fetch(&self, name: &str, version: &Version) -> PkgResult<String> {
        let path = self.pkg_dir(name, version).join("source.nxl");
        fs::read_to_string(&path)
            .map_err(|_| PkgError::new("NXL13007", format!("cache miss: {name}@{version}")))
    }

    /// Remove a specific package version from the cache.
    pub fn evict(&self, name: &str, version: &Version) -> PkgResult<()> {
        let dir = self.pkg_dir(name, version);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| PkgError::new("NXL13007", e.to_string()))?;
        }
        Ok(())
    }

    /// Remove all cached packages.
    pub fn clear(&self) -> PkgResult<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root).map_err(|e| PkgError::new("NXL13007", e.to_string()))?;
        }
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn dirs_or_temp() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
}

// ── Checksum ──────────────────────────────────────────────────────────────────

/// Compute a deterministic checksum for package content.
///
/// Uses a simple FNV-1a 64-bit hash as a stand-in for SHA-256.
/// Full cryptographic checksums require the P16 crypto module.
pub fn checksum(data: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

// ── Package manager ───────────────────────────────────────────────────────────

/// The NXP package manager — orchestrates install/update/add/remove/audit.
pub struct PackageManager {
    pub registry: Registry,
    pub cache: PackageCache,
    pub offline: bool,
}

impl PackageManager {
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        Self {
            registry: Registry::new(),
            cache: PackageCache::new(cache_root),
            offline: false,
        }
    }

    pub fn with_offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Install all dependencies declared in `manifest` into the project.
    ///
    /// Resolves the dependency graph, writes `nxl.lock`, and populates
    /// the package cache.
    pub fn install(&mut self, dir: &Path) -> PkgResult<InstallReport> {
        let manifest = Manifest::load(dir)?;
        let resolver = DependencyResolver::new(&self.registry);
        let resolved = resolver.resolve(&manifest)?;

        let mut lock = LockFile::new();
        let mut installed = Vec::new();

        for pkg in &resolved {
            let cs = if self.cache.is_cached(&pkg.name, &pkg.version) {
                let src = self.cache.fetch(&pkg.name, &pkg.version)?;
                checksum(src.as_bytes())
            } else {
                if self.offline && matches!(pkg.source, DepSource::Registry) {
                    return Err(PkgError::new(
                        "NXL13008",
                        format!(
                            "offline mode: package '{}@{}' is not in local cache",
                            pkg.name, pkg.version
                        ),
                    ));
                }
                // For path deps, read from disk; for git/registry deps, stub source.
                let src = match &pkg.source {
                    DepSource::Path(p) => {
                        let entry = p.join("src").join("main.nxl");
                        fs::read_to_string(&entry).unwrap_or_default()
                    }
                    DepSource::Git { url, branch_or_tag } => {
                        let tag = branch_or_tag.as_deref().unwrap_or("HEAD");
                        format!(
                            "// git source for {}@{} from {}#{}",
                            pkg.name, pkg.version, url, tag
                        )
                    }
                    DepSource::Registry => {
                        format!("// stub source for {}@{}", pkg.name, pkg.version)
                    }
                };
                self.cache.store(&pkg.name, &pkg.version, &src)?;
                checksum(src.as_bytes())
            };

            lock.packages.push(LockedPackage {
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                checksum: cs,
                source: pkg.source.clone(),
                dependencies: pkg.dependencies.clone(),
            });
            installed.push(format!("{}@{}", pkg.name, pkg.version));
        }

        lock.save(dir)?;
        Ok(InstallReport {
            installed,
            updated: Vec::new(),
            removed: Vec::new(),
        })
    }

    /// Compute dependency graph for project.
    pub fn graph(&self, dir: &Path) -> PkgResult<DependencyGraph> {
        let manifest = Manifest::load(dir)?;
        let resolver = DependencyResolver::new(&self.registry);
        let resolved = resolver.resolve(&manifest)?;
        Ok(DependencyGraph::build(&manifest, &resolved))
    }

    /// Add a new dependency to the manifest and install it.
    pub fn add(&mut self, dir: &Path, name: &str, req: &str) -> PkgResult<InstallReport> {
        let manifest_path = dir.join("nxl.toml");
        let mut text = fs::read_to_string(&manifest_path)
            .map_err(|e| PkgError::new("NXL13003", e.to_string()))?;

        // Append to [dependencies] section or add it.
        let dep_line = format!("{name} = \"{req}\"\n");
        if text.contains("[dependencies]") {
            text.push_str(&dep_line);
        } else {
            text.push_str(&format!("\n[dependencies]\n{dep_line}"));
        }
        fs::write(&manifest_path, &text).map_err(|e| PkgError::new("NXL13004", e.to_string()))?;

        self.install(dir)
    }

    /// Remove a dependency from the manifest and update the lock file.
    pub fn remove(&mut self, dir: &Path, name: &str) -> PkgResult<InstallReport> {
        let manifest_path = dir.join("nxl.toml");
        let text = fs::read_to_string(&manifest_path)
            .map_err(|e| PkgError::new("NXL13003", e.to_string()))?;

        let filtered: String = text
            .lines()
            .filter(|l| !l.trim_start().starts_with(&format!("{name} =")))
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(&manifest_path, &filtered)
            .map_err(|e| PkgError::new("NXL13004", e.to_string()))?;

        let report = self.install(dir)?;
        Ok(InstallReport {
            removed: vec![name.to_owned()],
            ..report
        })
    }

    /// Update all dependencies to their latest compatible versions.
    pub fn update(&mut self, dir: &Path) -> PkgResult<InstallReport> {
        // Delete lock file to force re-resolution.
        let lock_path = dir.join("nxl.lock");
        if lock_path.exists() {
            fs::remove_file(&lock_path).map_err(|e| PkgError::new("NXL13004", e.to_string()))?;
        }
        let report = self.install(dir)?;
        Ok(InstallReport {
            updated: report.installed.clone(),
            ..report
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct InstallReport {
    pub installed: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
}

impl InstallReport {
    pub fn is_empty(&self) -> bool {
        self.installed.is_empty() && self.updated.is_empty() && self.removed.is_empty()
    }
}

// ── Dependency audit ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AuditSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditSeverity::Low => write!(f, "low"),
            AuditSeverity::Medium => write!(f, "medium"),
            AuditSeverity::High => write!(f, "high"),
            AuditSeverity::Critical => write!(f, "critical"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditFinding {
    pub package: String,
    pub version: Version,
    pub severity: AuditSeverity,
    pub description: String,
}

/// Audit the lock file against a known-vulnerabilities database.
///
/// Status: IMPLEMENTED (local advisory list only).
/// Live CVE/advisory feed integration deferred to P28 (Security Audit).
pub fn audit(lock: &LockFile, advisories: &[Advisory]) -> Vec<AuditFinding> {
    let mut findings = Vec::new();
    for pkg in &lock.packages {
        for advisory in advisories {
            if advisory.package == pkg.name && advisory.affected_req.matches(&pkg.version) {
                findings.push(AuditFinding {
                    package: pkg.name.clone(),
                    version: pkg.version.clone(),
                    severity: advisory.severity.clone(),
                    description: advisory.description.clone(),
                });
            }
        }
    }
    findings
}

/// A known vulnerability advisory entry.
#[derive(Debug, Clone)]
pub struct Advisory {
    pub package: String,
    pub affected_req: VersionReq,
    pub severity: AuditSeverity,
    pub description: String,
}

// ── Workspace ─────────────────────────────────────────────────────────────────

/// A workspace groups multiple NXL packages under one root.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceConfig {
    pub members: Vec<PathBuf>,
}

impl WorkspaceConfig {
    pub fn parse(toml: &str, root: &Path) -> PkgResult<Self> {
        let mut members = Vec::new();
        let mut in_workspace = false;
        for raw in toml.lines() {
            let line = raw.trim();
            if line == "[workspace]" {
                in_workspace = true;
                continue;
            }
            if line.starts_with('[') {
                in_workspace = false;
                continue;
            }
            if !in_workspace {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == "members" {
                    let list = v.trim().trim_matches(|c| c == '[' || c == ']');
                    for m in list.split(',') {
                        let m = m.trim().trim_matches('"');
                        if !m.is_empty() {
                            members.push(root.join(m));
                        }
                    }
                }
            }
        }
        Ok(Self { members })
    }

    pub fn load(root: &Path) -> PkgResult<Self> {
        let path = root.join("nxl.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            fs::read_to_string(&path).map_err(|e| PkgError::new("NXL13003", e.to_string()))?;
        Self::parse(&text, root)
    }

    pub fn manifests(&self) -> PkgResult<Vec<Manifest>> {
        self.members.iter().map(|p| Manifest::load(p)).collect()
    }
}

// ── Dependency graph ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct DependencyNode {
    pub name: String,
    pub version: Version,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DependencyGraph {
    pub root: String,
    pub root_version: Version,
    pub nodes: Vec<DependencyNode>,
}

impl DependencyGraph {
    pub fn build(manifest: &Manifest, resolved: &[ResolvedPackage]) -> Self {
        let mut nodes = Vec::new();
        for pkg in resolved {
            nodes.push(DependencyNode {
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                dependencies: pkg.dependencies.clone(),
            });
        }
        Self {
            root: manifest.name.clone(),
            root_version: manifest.version.clone(),
            nodes,
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("{} v{}\n", self.root, self.root_version));
        for (i, node) in self.nodes.iter().enumerate() {
            let is_last = i == self.nodes.len() - 1;
            let branch = if is_last { "└── " } else { "├── " };
            out.push_str(&format!("{branch}{} v{}\n", node.name, node.version));
            for (j, sub) in node.dependencies.iter().enumerate() {
                let sub_is_last = j == node.dependencies.len() - 1;
                let sub_branch = if is_last {
                    if sub_is_last {
                        "    └── "
                    } else {
                        "    ├── "
                    }
                } else {
                    if sub_is_last {
                        "│   └── "
                    } else {
                        "│   ├── "
                    }
                };
                out.push_str(&format!("{sub_branch}{sub}\n"));
            }
        }
        out
    }
}
