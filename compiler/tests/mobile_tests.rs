//! Module 6 — the mobile SDK surface: scaffolding, configuration, generated
//! Android resources and the command-line target.

use nxl_compiler::cli::{
    cmd_new_from_template_in, cmd_test, parse_args, Command, MOBILE_BINARIES, MOBILE_TEMPLATE,
};
use nxl_compiler::mobile::MobileConfig;
use std::fs;
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("nxl_mobile_{tag}_{}_{nanos}", std::process::id()));
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

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler/ has a parent")
        .to_path_buf()
}

// ── command surface ────────────────────────────────────────────────────────────

#[test]
fn new_accepts_the_mobile_target_in_every_documented_form() {
    for form in [
        vec!["new", "mobile", "my-app"],
        vec!["new", "--target", "mobile", "my-app"],
        vec!["new", "--target=mobile", "my-app"],
        vec!["new", "--template", "mobile", "my-app"],
    ] {
        assert_eq!(
            parse_args(&args(&form)).unwrap(),
            Command::New {
                name: "my-app".into(),
                template: Some("mobile".into())
            },
            "form {form:?} should parse"
        );
    }
}

#[test]
fn build_accepts_an_explicit_android_target() {
    for form in [
        vec!["build", "--target", "android"],
        vec!["build", "--target=android"],
        vec!["build", "--release", "--target", "android"],
    ] {
        assert_eq!(
            parse_args(&args(&form)).unwrap(),
            Command::Build {
                release: form.contains(&"--release"),
                target: Some("android".into())
            },
            "form {form:?} should parse"
        );
    }
    assert_eq!(
        parse_args(&args(&["build"])).unwrap(),
        Command::Build {
            release: false,
            target: None
        }
    );
    assert!(parse_args(&args(&["build", "--target"])).is_err());
}

// ── scaffolding ────────────────────────────────────────────────────────────────

#[test]
fn mobile_template_creates_a_customizable_android_project() {
    let dir = TempDir::new("scaffold");
    let message =
        cmd_new_from_template_in(dir.path(), "my-mobile-app", "mobile").expect("mobile template");
    assert!(message.contains("my-mobile-app"), "{message}");
    assert!(
        message.contains("nexora build --target android"),
        "the next step must be the Android build: {message}"
    );

    let project = dir.path().join("my-mobile-app");
    for file in [
        "nxl.toml",
        "README.md",
        ".gitignore",
        "src/main.nxl",
        "tests/basic.nxl",
        "build.gradle",
        "settings.gradle",
        "gradle.properties",
        "gradle/wrapper/gradle-wrapper.properties",
        "gradlew",
        "gradlew.bat",
        "app/build.gradle",
        "app/nxl.gradle",
        "app/src/main/AndroidManifest.xml",
        "app/src/main/java/com/nexora/nxl/MainActivity.java",
        "app/src/main/java/com/nexora/nxl/NxlRuntime.java",
        "app/src/main/assets/mobile.nxl",
        "app/src/main/assets/welcome.nxl",
        "app/src/main/assets/features.nxl",
        "app/src/main/assets/about.nxl",
        "app/src/main/res/values/nxl.xml",
        "app/src/main/res/values/styles.xml",
        "app/src/main/res/drawable/nxl_splash.xml",
    ] {
        assert!(
            project.join(file).is_file(),
            "the mobile template must create {file}"
        );
    }
    for relative in [
        "gradle/wrapper/gradle-wrapper.jar",
        "app/src/main/jniLibs/x86_64/libnxl_android.so",
        "app/src/main/jniLibs/arm64-v8a/libnxl_android.so",
    ] {
        let path = project.join(relative);
        let size = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("{relative}: {error}"))
            .len();
        assert!(size > 1024, "{relative} was written as {size} bytes");
    }

    // The wrapper is executed with `sh gradlew` on Unix; a CRLF script would
    // fail there even though Windows uses gradlew.bat.
    let gradlew = fs::read_to_string(project.join("gradlew")).unwrap();
    assert!(!gradlew.contains('\r'), "gradlew must be LF-only");

    // The id is derived from the project name and the manifest is consistent.
    let manifest = fs::read_to_string(project.join("nxl.toml")).unwrap();
    assert!(manifest.contains("name = \"my-mobile-app\""), "{manifest}");
    assert!(
        manifest.contains("application_id = \"com.nexora.mymobileapp\""),
        "{manifest}"
    );
    assert!(
        manifest.contains("targets = [\"android\"]"),
        "the manifest must say which target is real: {manifest}"
    );

    // The host-side test file runs, and `nexora test` also compiles every
    // screen with the generated prelude.
    let report = cmd_test(&project, None).expect("generated mobile project tests");
    assert_eq!(report.failures, Vec::<String>::new(), "{report:?}");
    assert_eq!(report.passed, report.total, "{report:?}");
    assert!(report.passed >= 1, "{report:?}");
}

#[test]
fn customizing_the_manifest_changes_resources_prelude_and_initial_screen() {
    let dir = TempDir::new("customize");
    cmd_new_from_template_in(dir.path(), "demo", "mobile").unwrap();
    let project = dir.path().join("demo");

    let manifest = fs::read_to_string(project.join("nxl.toml")).unwrap();
    let customized = manifest
        .replace("app_name = \"demo\"", "app_name = \"A&B <Demo>\"")
        .replace(
            "application_id = \"com.nexora.demo\"",
            "application_id = \"com.example.demo\"",
        )
        .replace("primary_color = \"#5AA9FF\"", "primary_color = \"#aa00ff\"")
        .replace(
            "welcome_heading = \"Welcome to demo\"",
            "welcome_heading = \"Hello A&B <Demo>\"",
        )
        .replace("initial_screen = \"Welcome\"", "initial_screen = \"About\"");
    fs::write(project.join("nxl.toml"), customized).unwrap();

    let config = MobileConfig::load(&project)
        .expect("the customized manifest parses")
        .expect("the project is mobile");
    assert_eq!(config.app_name, "A&B <Demo>");
    assert_eq!(config.application_id, "com.example.demo");
    assert_eq!(config.primary_color, "#AA00FF");
    assert_eq!(config.initial_screen, "About");

    nxl_compiler::mobile::sync_android(&project, &config).expect("resources regenerate");

    let xml = fs::read_to_string(project.join("app/src/main/res/values/nxl.xml")).unwrap();
    assert!(xml.contains("A&amp;B &lt;Demo&gt;"), "{xml}");
    assert!(xml.contains("<color name=\"nxl_primary\">#AA00FF</color>"));
    assert!(xml.contains("<integer name=\"nxl_initial_screen\">2</integer>"));

    let gradle = fs::read_to_string(project.join("app/nxl.gradle")).unwrap();
    assert!(
        gradle.contains("nxlApplicationId = \"com.example.demo\""),
        "{gradle}"
    );

    let prelude = fs::read_to_string(project.join("app/src/main/assets/mobile.nxl")).unwrap();
    assert!(
        prelude.contains("let app_name = \"A&B <Demo>\";"),
        "{prelude}"
    );
    assert!(
        prelude.contains("let welcome_heading = \"Hello A&B <Demo>\";"),
        "{prelude}"
    );
    assert!(
        prelude.contains("let screen_labels = [\"Welcome\", \"Features\", \"About\"];"),
        "{prelude}"
    );

    // The customized project still compiles every screen.
    let report = cmd_test(&project, None).expect("customized mobile project tests");
    assert_eq!(report.failures, Vec::<String>::new(), "{report:?}");
}

#[test]
fn optional_logo_and_splash_are_copied_only_when_configured() {
    let dir = TempDir::new("assets");
    cmd_new_from_template_in(dir.path(), "demo", "mobile").unwrap();
    let project = dir.path().join("demo");

    // No logo/splash configured: no stray drawables, and the activity falls
    // back to a letter badge.
    let drawable = project.join("app/src/main/res/drawable");
    assert!(!drawable.join("nxl_logo.png").exists());
    assert!(!drawable.join("nxl_splash_image.png").exists());

    fs::write(project.join("logo.png"), b"not a real png, but a real file").unwrap();
    fs::write(project.join("splash.png"), b"also not a png").unwrap();
    let manifest = fs::read_to_string(project.join("nxl.toml")).unwrap();
    let customized = manifest.replace(
        "targets = [\"android\"]",
        "logo = \"logo.png\"\nsplash = \"splash.png\"\ntargets = [\"android\"]",
    );
    fs::write(project.join("nxl.toml"), customized).unwrap();

    let config = MobileConfig::load(&project).unwrap().unwrap();
    nxl_compiler::mobile::sync_android(&project, &config).unwrap();
    assert!(drawable.join("nxl_logo.png").is_file());
    assert!(drawable.join("nxl_splash_image.png").is_file());
    let splash = fs::read_to_string(drawable.join("nxl_splash.xml")).unwrap();
    assert!(
        splash.contains("@drawable/nxl_splash_image"),
        "the splash drawable must reference the supplied image: {splash}"
    );
}

#[test]
fn splash_must_be_raster_but_the_logo_may_be_a_vector() {
    let dir = TempDir::new("vector");
    cmd_new_from_template_in(dir.path(), "demo", "mobile").unwrap();
    let project = dir.path().join("demo");
    fs::create_dir_all(project.join("branding")).unwrap();
    fs::write(project.join("branding/logo.xml"), "<vector />").unwrap();
    fs::write(project.join("branding/splash.xml"), "<vector />").unwrap();

    let manifest = fs::read_to_string(project.join("nxl.toml")).unwrap();
    let customized = manifest.replace(
        "targets = [\"android\"]",
        "logo = \"branding/logo.xml\"\nsplash = \"branding/splash.xml\"\ntargets = [\"android\"]",
    );
    fs::write(project.join("nxl.toml"), customized).unwrap();

    let config = MobileConfig::load(&project).unwrap().unwrap();
    let error = nxl_compiler::mobile::sync_android(&project, &config)
        .expect_err("a vector splash would produce a bitmap layer that cannot inflate");
    assert_eq!(error.code, "NXL12020", "{error:?}");
    assert!(
        error.message.contains("png, jpg, jpeg or webp"),
        "{error:?}"
    );

    // The same logo as a vector is fine; only the splash is restricted.
    let manifest = fs::read_to_string(project.join("nxl.toml")).unwrap();
    fs::write(
        project.join("nxl.toml"),
        manifest.replace(
            "splash = \"branding/splash.xml\"",
            "splash = \"branding/splash.png\"",
        ),
    )
    .unwrap();
    fs::write(project.join("branding/splash.png"), b"raster stand-in").unwrap();
    let config = MobileConfig::load(&project).unwrap().unwrap();
    nxl_compiler::mobile::sync_android(&project, &config).expect("raster splash + vector logo");
    let drawable = project.join("app/src/main/res/drawable");
    assert!(drawable.join("nxl_logo.xml").is_file());
    assert!(drawable.join("nxl_splash_image.png").is_file());
}

#[test]
fn a_missing_screen_asset_is_reported_before_a_build() {
    let dir = TempDir::new("missing_asset");
    cmd_new_from_template_in(dir.path(), "demo", "mobile").unwrap();
    let project = dir.path().join("demo");
    fs::remove_file(project.join("app/src/main/assets/about.nxl")).unwrap();

    let error = cmd_test(&project, None).expect_err("a missing screen must fail validation");
    assert_eq!(error.code, "NXL12020", "{error:?}");
    assert!(error.message.contains("about.nxl"), "{error:?}");
}

#[test]
fn signing_secrets_are_read_from_the_environment_and_never_stored() {
    let dir = TempDir::new("secrets");
    cmd_new_from_template_in(dir.path(), "demo", "mobile").unwrap();
    let project = dir.path().join("demo");

    let build_gradle = fs::read_to_string(project.join("app/build.gradle")).unwrap();
    for variable in [
        "NXL_ANDROID_KEYSTORE",
        "NXL_ANDROID_KEYSTORE_PASSWORD",
        "NXL_ANDROID_KEY_ALIAS",
        "NXL_ANDROID_KEY_PASSWORD",
    ] {
        assert!(
            build_gradle.contains(variable),
            "signing must document {variable}"
        );
    }
    assert!(
        !build_gradle.contains("storePassword \""),
        "no literal store password may appear in the project"
    );
    assert!(
        !build_gradle.contains("keyPassword \""),
        "no literal key password may appear in the project"
    );

    let gitignore = fs::read_to_string(project.join(".gitignore")).unwrap();
    for pattern in ["*.jks", "*.keystore", "keystore.properties"] {
        assert!(gitignore.contains(pattern), "{gitignore}");
    }
}

#[test]
fn no_signing_material_is_checked_into_the_template() {
    let repo = repo_root().join("templates/mobile");
    if !repo.exists() {
        return; // installed binary, not a checkout
    }
    let mut stack = vec![repo.clone()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).expect("read template dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            assert!(
                !name.ends_with(".jks") && !name.ends_with(".keystore"),
                "signing material must not be in the template: {}",
                path.display()
            );
        }
    }
}

// ── embedded copy stays in sync ────────────────────────────────────────────────

#[test]
fn embedded_mobile_template_matches_the_repository_copy() {
    let repo = repo_root().join("templates/mobile");
    if !repo.exists() {
        return; // installed binary, not a checkout
    }
    for (relative, embedded) in MOBILE_TEMPLATE {
        let on_disk = fs::read_to_string(repo.join(relative))
            .unwrap_or_else(|error| panic!("templates/mobile/{relative}: {error}"));
        let normalise = |text: String| text.replace("\r\n", "\n").trim_end().to_owned();
        assert_eq!(
            normalise(on_disk),
            normalise(embedded.to_owned()),
            "templates/mobile/{relative} and the embedded copy have drifted apart"
        );
    }
    for (relative, embedded) in MOBILE_BINARIES {
        let on_disk = fs::read(repo.join(relative))
            .unwrap_or_else(|error| panic!("templates/mobile/{relative}: {error}"));
        assert_eq!(
            on_disk, embedded,
            "templates/mobile/{relative} and the embedded binary have drifted apart"
        );
    }
}

#[test]
fn every_file_under_the_mobile_template_is_accounted_for() {
    let repo = repo_root().join("templates/mobile");
    if !repo.exists() {
        return;
    }
    let mut on_disk = Vec::new();
    let mut stack = vec![repo.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read template dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(relative) = path.strip_prefix(&repo) {
                on_disk.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    on_disk.sort();
    let mut declared = MOBILE_TEMPLATE
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .chain(MOBILE_BINARIES.iter().map(|(path, _)| (*path).to_owned()))
        .collect::<Vec<_>>();
    declared.sort();
    assert_eq!(
        on_disk, declared,
        "templates/mobile and the embedded lists must name the same files"
    );
}
