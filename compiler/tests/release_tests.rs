//! Release-packaging and installer tests (R03/R04).
//!
//! These exercise the artifacts a user actually downloads, not just the
//! development build. Platform-specific checks skip cleanly when their tools
//! are unavailable so the suite still runs on the CI matrix.

use nxl_compiler::cli::{format_doctor_report, run_doctor_in, DoctorCheck};
use nxl_compiler::source::SourceFile;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler/ has a parent")
        .to_path_buf()
}

fn windows_powershell_available() -> bool {
    cfg!(windows)
        && Command::new("powershell")
            .arg("-NoProfile")
            .output()
            .is_ok()
}

/// A temporary directory that removes itself when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        let unique = format!("nxl_release_{tag}_{}_{nanos}", std::process::id());
        let path = std::env::temp_dir().join(unique);
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

// ── Source handling ─────────────────────────────────────────────────────────────

#[test]
fn utf8_bom_is_stripped_from_source() {
    // Windows editors and PowerShell's `utf8` encoding emit a BOM; the first
    // character of such a file must still lex.
    let source = SourceFile::new("bom.nxl", "\u{feff}print(\"hi\");");
    assert!(!source.text.starts_with('\u{feff}'), "BOM must be removed");
    let result = nxl_compiler::vm::run(&source).expect("BOM file should compile");
    assert_eq!(result.output, vec!["hi"]);
}

#[test]
fn bom_free_source_is_unchanged() {
    let source = SourceFile::new("plain.nxl", "print(\"hi\");");
    assert_eq!(source.text, "print(\"hi\");");
}

// ── doctor ──────────────────────────────────────────────────────────────────────

#[test]
fn doctor_reports_a_valid_project_layout() {
    let dir = TempDir::new("doctor_ok");
    fs::write(
        dir.path().join("nxl.toml"),
        "[package]\nentry = \"src/main.nxl\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.nxl"), "print(1);").unwrap();

    let checks = run_doctor_in(dir.path());
    let layout = checks.iter().find(|c| c.name == "project layout").unwrap();
    assert!(layout.ok, "valid layout must pass: {layout:?}");
    assert!(
        layout.detail.contains("src/main.nxl"),
        "doctor should name the entry point: {}",
        layout.detail
    );
    assert!(checks.iter().all(|c| !c.blocks_release()));
}

#[test]
fn doctor_fails_when_the_declared_entry_is_missing() {
    let dir = TempDir::new("doctor_bad_entry");
    fs::write(
        dir.path().join("nxl.toml"),
        "[package]\nentry = \"src/gone.nxl\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();

    let checks = run_doctor_in(dir.path());
    let layout = checks.iter().find(|c| c.name == "project layout").unwrap();
    assert!(!layout.ok, "missing entry must be reported");
    assert!(layout.blocks_release(), "a broken entry must fail doctor");
    assert!(format_doctor_report(&checks).contains("[fail]"));
}

#[test]
fn doctor_outside_a_project_is_informational_only() {
    let dir = TempDir::new("doctor_no_project");
    let checks = run_doctor_in(dir.path());
    let layout = checks.iter().find(|c| c.name == "project layout").unwrap();
    assert!(layout.ok, "not being in a project is not an error");
    assert!(!layout.required, "it must not block a normal run");
}

#[test]
fn doctor_report_labels_optional_failures_as_warnings() {
    let checks = vec![
        DoctorCheck::pass("ok thing", true, "fine"),
        DoctorCheck::warn("optional thing", false, "missing"),
    ];
    let report = format_doctor_report(&checks);
    assert!(report.contains("[ok] ok thing"));
    assert!(report.contains("[warn] optional thing"));
    assert!(report.contains("result: ready"));
    assert!(!checks.iter().any(DoctorCheck::blocks_release));
}

#[test]
fn doctor_reports_the_compiler_version() {
    let checks = run_doctor_in(&std::env::temp_dir());
    let compiler = checks.iter().find(|c| c.name == "nxlc compiler").unwrap();
    assert!(compiler.ok);
    assert!(
        compiler.detail.starts_with("nxlc "),
        "version must be real, not a placeholder: {}",
        compiler.detail
    );
}

// ── installer ───────────────────────────────────────────────────────────────────

/// Where this platform's staged release lives. On CI the packaging step stages
/// the directory for the runner it is running on, so these tests check the real
/// artifact rather than a Windows-only copy.
fn stage_dir() -> PathBuf {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x64",
        (os, arch) => panic!("no packaged release target for {os}-{arch}"),
    };
    repo_root().join("dist").join(target)
}

/// The compiler inside the staged directory, whatever it is called.
fn staged_compiler() -> PathBuf {
    let stage = stage_dir();
    if cfg!(windows) {
        stage.join("nxlc.exe")
    } else {
        stage.join("nxlc")
    }
}

/// Run the packaged installer against a copy of the staged release.
fn run_installer(stage: &Path, args: &[&str]) -> (i32, String) {
    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(stage.join("install.ps1"))
        .args(args)
        .output()
        .expect("installer runs");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), text)
}

#[test]
fn installer_dry_run_reports_success_without_installing() {
    if !windows_powershell_available() {
        return;
    }
    let stage = stage_dir();
    if !stage.join("install.ps1").exists() || !stage.join("nxlc.exe").exists() {
        return; // not packaged yet
    }
    // Dry run must not touch the default install directory.
    let install_dir = std::env::var("LOCALAPPDATA")
        .map(|base| Path::new(&base).join("NEXORA").join("bin"))
        .unwrap();
    let existed = install_dir.exists();

    let (code, text) = run_installer(&stage, &["-DryRun"]);
    assert_eq!(code, 0, "dry run should succeed:\n{text}");
    assert!(text.contains("Verifying artifact integrity"), "{text}");
    assert!(text.contains("SHA-256 matches"), "{text}");
    assert!(text.contains("Dry run complete"), "{text}");
    assert_eq!(
        install_dir.exists(),
        existed,
        "dry run must not create the install directory"
    );
}

#[test]
fn installer_rejects_a_tampered_payload() {
    if !windows_powershell_available() {
        return;
    }
    let stage = stage_dir();
    if !stage.join("install.ps1").exists() || !stage.join("nxlc.exe").exists() {
        return;
    }
    let copy = TempDir::new("tamper");
    for item in ["install.ps1", "nxlc.exe", "checksums.sha256"] {
        fs::copy(stage.join(item), copy.path().join(item)).expect("copy staged file");
    }
    // Flip the payload without touching the checksum file.
    let mut bytes = fs::read(copy.path().join("nxlc.exe")).expect("read payload");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    fs::write(copy.path().join("nxlc.exe"), bytes).expect("write payload");

    let (code, text) = run_installer(copy.path(), &["-DryRun"]);
    assert_ne!(code, 0, "a tampered payload must be rejected:\n{text}");
    assert!(text.contains("SHA-256 mismatch"), "{text}");
    assert!(text.contains("nothing was installed"), "{text}");
}

#[test]
fn installer_refuses_to_run_without_a_checksum_file() {
    if !windows_powershell_available() {
        return;
    }
    let stage = stage_dir();
    if !stage.join("install.ps1").exists() || !stage.join("nxlc.exe").exists() {
        return;
    }
    let copy = TempDir::new("nochecksum");
    for item in ["install.ps1", "nxlc.exe"] {
        fs::copy(stage.join(item), copy.path().join(item)).expect("copy staged file");
    }
    let (code, text) = run_installer(copy.path(), &["-DryRun"]);
    assert_ne!(code, 0, "an unverified payload must be refused:\n{text}");
    assert!(text.contains("checksums.sha256 not found"), "{text}");
}

// ── Unix installer ──────────────────────────────────────────────────────────────

/// A bash that exists on this machine. CI runners always have one; a Windows
/// developer with Git for Windows also has `bash` on PATH, which is enough to
/// syntax-check and to prove the platform gate refuses a wrong host.
fn bash_path() -> Option<String> {
    if cfg!(windows) {
        for candidate in [
            "bash",
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
        ] {
            if Command::new(candidate)
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return Some(candidate.to_owned());
            }
        }
        None
    } else if Command::new("bash").arg("--version").output().is_ok() {
        Some("bash".to_owned())
    } else {
        None
    }
}

fn repo_file(relative: &str) -> PathBuf {
    repo_root().join(relative)
}

#[test]
fn unix_installer_and_packaging_scripts_are_syntactically_valid() {
    let Some(bash) = bash_path() else {
        return;
    };
    for script in ["install/unix/install.sh", "scripts/package-release.sh"] {
        let output = Command::new(&bash)
            .arg("-n")
            .arg(repo_file(script))
            .output()
            .expect("bash runs");
        assert!(
            output.status.success(),
            "{script} has a syntax error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn unix_scripts_use_unix_line_endings_and_no_bom() {
    // A CRLF or BOM-prefixed shebang breaks `sh install.sh` outright.
    for script in ["install/unix/install.sh", "scripts/package-release.sh"] {
        let bytes = fs::read(repo_file(script)).expect("script exists");
        assert!(
            !bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
            "{script} must not start with a UTF-8 BOM"
        );
        assert_eq!(
            bytes.first(),
            Some(&b'#'),
            "{script} must start with a shebang"
        );
        assert!(
            !bytes.windows(2).any(|pair| pair == b"\r\n"),
            "{script} must use LF line endings"
        );
    }
}

#[test]
fn unix_installer_refuses_a_wrong_platform() {
    let Some(bash) = bash_path() else {
        return;
    };
    let installer = repo_file("install/unix/install.sh");
    // Running on a host the script does not target must fail loudly rather
    // than half-installing.
    let output = Command::new(&bash)
        .arg(&installer)
        .arg("--dry-run")
        .output()
        .expect("installer runs");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let host_ok = cfg!(target_os = "linux") || cfg!(target_os = "macos");
    if host_ok {
        assert!(output.status.success(), "on a supported host: {combined}");
    } else {
        assert!(
            !output.status.success(),
            "a Windows host must be refused: {combined}"
        );
        assert!(
            combined.contains("unsupported operating system")
                || combined.contains("this package is"),
            "the refusal must explain itself: {combined}"
        );
    }
}

#[test]
fn unix_packaging_refuses_a_platform_it_is_not_running_on() {
    let Some(bash) = bash_path() else {
        return;
    };
    let script = repo_file("scripts/package-release.sh");
    // No argument at all is a usage error, and a mismatched target must never
    // produce an archive.
    let usage = Command::new(&bash).arg(&script).output().expect("runs");
    assert!(!usage.status.success(), "a target argument is required");

    let wrong = if cfg!(target_os = "linux") {
        "macos-arm64"
    } else {
        "linux-x64"
    };
    let output = Command::new(&bash)
        .arg(&script)
        .arg(wrong)
        .output()
        .expect("runs");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "packaging for the wrong platform must fail: {combined}"
    );
    assert!(
        combined.contains("refusing to package")
            || combined.contains("unsupported operating system"),
        "the refusal must explain itself: {combined}"
    );
}

#[test]
fn unix_installer_dry_run_matches_its_staged_copy() {
    // The copy shipped in a staged release must still refuse mismatched hosts
    // and must never write anything during a dry run.
    let installer = if cfg!(windows) {
        stage_dir().join("install.ps1")
    } else {
        stage_dir().join("install.sh")
    };
    if !installer.exists() {
        return; // not packaged for this platform yet
    }
    let Some(bash) = bash_path() else {
        return;
    };
    let stage = stage_dir();
    let before = fs::read_dir(&stage).map(|d| d.count()).unwrap_or(0);
    let output = Command::new(&bash)
        .arg("sh")
        .arg(&installer)
        .arg("--dry-run")
        .output()
        .expect("installer runs");
    let after = fs::read_dir(&stage).map(|d| d.count()).unwrap_or(0);
    assert_eq!(before, after, "a dry run must not change the directory");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!combined.is_empty(), "the installer must explain itself");
}

#[test]
fn unix_installer_has_expected_permissions_and_files() {
    let stage = stage_dir();
    if cfg!(windows) || !stage.join("nxlc").exists() {
        return; // Windows packages, or not packaged yet
    }
    for file in [
        "nxlc",
        "install.sh",
        "checksums.sha256",
        "examples/main.nxl",
    ] {
        assert!(
            stage.join(file).exists(),
            "a Unix release must contain {file}"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for file in ["nxlc", "install.sh"] {
            let mode = fs::metadata(stage.join(file)).unwrap().permissions().mode();
            assert!(
                mode & 0o111 != 0,
                "{file} must be executable (mode {mode:o})"
            );
        }
    }
}

// ── packaged artifact ───────────────────────────────────────────────────────────

#[test]
fn packaged_compiler_runs_the_bundled_example() {
    let exe = staged_compiler();
    if !exe.exists() {
        return; // not packaged yet
    }
    let version = Command::new(&exe).arg("--version").output().expect("runs");
    assert!(version.status.success());
    let reported = String::from_utf8_lossy(&version.stdout);
    assert!(
        reported.starts_with("nxlc "),
        "packaged binary must report a version: {reported}"
    );

    let example = stage_dir().join("examples").join("main.nxl");
    if !example.exists() {
        return;
    }
    let run = Command::new(&exe)
        .arg("run")
        .arg(&example)
        .output()
        .expect("runs");
    assert!(
        run.status.success(),
        "packaged binary must run the bundled example: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "30");
}

#[test]
fn packaged_compiler_rejects_invalid_source() {
    let exe = staged_compiler();
    if !exe.exists() {
        return;
    }
    let dir = TempDir::new("bad_source");
    let bad = dir.path().join("bad.nxl");
    fs::write(&bad, "fn ((( broken").unwrap();
    let run = Command::new(&exe)
        .arg("check")
        .arg(&bad)
        .output()
        .expect("runs");
    assert!(
        !run.status.success(),
        "invalid source must exit non-zero, not report success"
    );
}

#[test]
fn packaged_checksums_match_the_staged_files() {
    let stage = stage_dir();
    let checksum_file = stage.join("checksums.sha256");
    if !checksum_file.exists() {
        return; // not packaged yet
    }
    let text = fs::read_to_string(&checksum_file).expect("read checksums");
    let mut checked = 0;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(expected), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        let file = stage.join(name.trim_start_matches('*'));
        if !file.exists() {
            continue; // the archive itself is not part of its own listing
        }
        let actual = sha256::hex(&fs::read(&file).unwrap());
        assert_eq!(
            actual,
            expected.to_lowercase(),
            "checksum mismatch for {name}"
        );
        checked += 1;
    }
    assert!(checked > 0, "no staged files were verified");
}

/// SHA-256 (FIPS 180-4) so release integrity checks in this suite do not
/// depend on an external crate being available offline.
mod sha256 {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    pub fn hex(data: &[u8]) -> String {
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];

        let mut message = data.to_vec();
        let bit_len = (data.len() as u64).wrapping_mul(8);
        message.push(0x80);
        while message.len() % 64 != 56 {
            message.push(0);
        }
        message.extend_from_slice(&bit_len.to_be_bytes());

        for chunk in message.chunks(64) {
            let mut w = [0u32; 64];
            for (i, word) in chunk.chunks(4).enumerate() {
                w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }

            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let choose = (e & f) ^ ((!e) & g);
                let temp1 = hh
                    .wrapping_add(s1)
                    .wrapping_add(choose)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let majority = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(majority);

                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }
            for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
                *slot = slot.wrapping_add(value);
            }
        }

        h.iter().flat_map(|word| word.to_be_bytes()).fold(
            String::with_capacity(64),
            |mut acc, byte| {
                acc.push_str(&format!("{byte:02x}"));
                acc
            },
        )
    }
}

#[test]
fn sha256_helper_matches_known_vectors() {
    // If this ever drifts, every checksum assertion above becomes meaningless.
    assert_eq!(
        sha256::hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256::hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256::hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}
