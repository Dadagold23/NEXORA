//! Module 6 — the mobile SDK surface: project configuration, Android resource
//! generation, and the `build`/`dev`/`test` behaviour for mobile projects.
//!
//! # The contract
//!
//! A mobile project is an ordinary NXL project (`nxl.toml` with `[package]`)
//! that also carries a `[mobile]` section. That section is the single source of
//! truth for the app name, application id, colours, screens, welcome copy and
//! initial destination. [`sync_android`] turns it into Android resources and an
//! NXL prelude; [`build_android`] runs Gradle; [`dev_android`] installs and
//! launches the result on a connected device.
//!
//! Nothing in this module writes signing credentials. Release signing is read
//! from environment variables by the generated `app/build.gradle`, so a secret
//! can never end up in the project or in source control.
//!
//! # What is deliberately not here
//!
//! The prebuilt `libnxl_android.so` for the two ABIs is part of the template,
//! because the desktop binary cannot cross-compile a `cdylib` for Android
//! without the NXL source tree and an NDK. The starter's README says so, and
//! `docs/25-mobile.md` repeats it rather than implying otherwise.

use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

use crate::cli::{CliError, CliResult};
use crate::diagnostic::Diagnostic;
use crate::source::SourceFile;

/// Platform targets with a real implementation in this toolchain.
pub const SUPPORTED_TARGETS: [&str; 1] = ["android"];

const CONFIG_ERROR: &str = "NXL12020";
const TOOLCHAIN_ERROR: &str = "NXL12021";
const BUILD_ERROR: &str = "NXL12022";
const DEVICE_ERROR: &str = "NXL12023";
const NOT_MOBILE: &str = "NXL12024";

/// One tab: the label the activity shows and the NXL program it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct MobileScreen {
    pub label: String,
    pub asset: String,
}

/// Parsed `[mobile]` configuration, with defaults for anything omitted.
///
/// `present` is false when the manifest has no `[mobile]` section at all; in
/// that case the other fields hold defaults and must not be used.
#[derive(Debug, Clone, PartialEq)]
pub struct MobileConfig {
    pub present: bool,
    pub app_name: String,
    pub application_id: String,
    pub version_name: String,
    pub version_code: u32,
    pub primary_color: String,
    pub secondary_color: String,
    pub welcome_heading: String,
    pub welcome_body: String,
    pub initial_screen: String,
    pub screens: Vec<MobileScreen>,
    pub targets: Vec<String>,
    /// Project-relative path to a PNG/JPG/WebP/vector drawable, if any.
    pub logo: Option<String>,
    /// Project-relative path to a splash image, if any.
    pub splash: Option<String>,
}

impl MobileConfig {
    /// Read `dir/nxl.toml`, returning `None` when the project is not mobile.
    ///
    /// A malformed `[mobile]` section is an error even though the rest of the
    /// project is fine: silently ignoring a typo would build an app whose
    /// identity is not what the manifest says.
    pub fn load(dir: &Path) -> CliResult<Option<Self>> {
        let path = dir.join("nxl.toml");
        let Ok(text) = fs::read_to_string(&path) else {
            return Ok(None);
        };
        if !has_mobile_section(&text) {
            return Ok(None);
        }
        let project_name = package_field(&text, "name").unwrap_or_else(|| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("app")
                .to_owned()
        });
        let config = Self::parse(&text, &project_name)?;
        Ok(Some(config))
    }

    /// Parse a manifest's text. `project_name` supplies the defaults that the
    /// `[package]` section normally provides.
    pub fn parse(toml: &str, project_name: &str) -> CliResult<Self> {
        let version_name = package_field(toml, "version").unwrap_or_else(|| "0.1.0".to_owned());
        let mut config = Self::defaults(project_name, &version_name);
        let mut section = "";
        let mut seen_mobile = false;
        let mut screens_started = false;
        let mut heading_set = false;
        let mut initial_set = false;

        for raw in toml.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with("[[") {
                if line == "[[mobile.screens]]" {
                    if !screens_started {
                        config.screens.clear();
                        screens_started = true;
                    }
                    config.screens.push(MobileScreen {
                        label: String::new(),
                        asset: String::new(),
                    });
                    section = "screen";
                } else {
                    section = "other";
                }
                continue;
            }
            if line.starts_with('[') {
                section = if line == "[mobile]" {
                    seen_mobile = true;
                    "mobile"
                } else {
                    "other"
                };
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match section {
                "mobile" => {
                    let text = parse_string(key, value)?;
                    match key {
                        "app_name" => config.app_name = text,
                        "application_id" => config.application_id = text,
                        "primary_color" => config.primary_color = text,
                        "secondary_color" => config.secondary_color = text,
                        "welcome_heading" => {
                            config.welcome_heading = text;
                            heading_set = true;
                        }
                        "welcome_body" => config.welcome_body = text,
                        "initial_screen" => {
                            config.initial_screen = text;
                            initial_set = true;
                        }
                        "logo" => config.logo = non_empty(text),
                        "splash" => config.splash = non_empty(text),
                        "version_code" => {
                            config.version_code = text.parse().map_err(|_| {
                                config_err(format!(
                                    "[mobile] version_code: '{text}' is not a positive integer"
                                ))
                            })?;
                        }
                        "targets" => config.targets = parse_array(key, value)?,
                        other => {
                            return Err(config_err(format!(
                                "[mobile] unknown key '{other}' (known keys: app_name, \
                                 application_id, primary_color, secondary_color, \
                                 welcome_heading, welcome_body, initial_screen, logo, splash, \
                                 version_code, targets; screens use [[mobile.screens]])"
                            )))
                        }
                    }
                }
                "screen" => {
                    let text = parse_string(key, value)?;
                    let screen = config
                        .screens
                        .last_mut()
                        .expect("a [[mobile.screens]] table always pushes one entry");
                    match key {
                        "label" => screen.label = text,
                        "asset" => screen.asset = text,
                        other => {
                            return Err(config_err(format!(
                            "[[mobile.screens]] unknown key '{other}' (known keys: label, asset)"
                        )))
                        }
                    }
                }
                _ => {}
            }
        }

        if !seen_mobile {
            config.present = false;
            return Ok(config);
        }
        config.present = true;
        if !heading_set {
            config.welcome_heading = format!("Welcome to {}", config.app_name);
        }
        if !initial_set {
            if let Some(first) = config.screens.first() {
                config.initial_screen = first.label.clone();
            }
        }
        config.validate()?;
        Ok(config)
    }

    fn defaults(project_name: &str, version_name: &str) -> Self {
        let screens = default_screens();
        Self {
            present: false,
            app_name: project_name.to_owned(),
            application_id: default_application_id(project_name),
            version_name: version_name.to_owned(),
            version_code: 1,
            primary_color: "#5AA9FF".to_owned(),
            secondary_color: "#2B4B73".to_owned(),
            welcome_heading: format!("Welcome to {project_name}"),
            welcome_body: "The content of this screen was produced by NXL running on this device."
                .to_owned(),
            initial_screen: screens[0].label.clone(),
            screens,
            targets: vec!["android".to_owned()],
            logo: None,
            splash: None,
        }
    }

    /// Reject values that would produce an app that cannot be installed or a
    /// screen the activity cannot find. Called after parsing and before every
    /// build, so a hand-edited manifest fails with the manifest's line of
    /// reasoning rather than a Gradle stack trace.
    pub fn validate(&mut self) -> CliResult<()> {
        if self.app_name.trim().is_empty() {
            return Err(config_err("[mobile] app_name must not be empty"));
        }
        validate_application_id(&self.application_id)?;
        self.primary_color = normalize_color("primary_color", &self.primary_color)?;
        self.secondary_color = normalize_color("secondary_color", &self.secondary_color)?;
        if self.version_code == 0 {
            return Err(config_err("[mobile] version_code must be at least 1"));
        }
        if self.version_name.trim().is_empty() {
            return Err(config_err("[package] version must not be empty"));
        }

        if self.targets.is_empty() {
            return Err(config_err(
                "[mobile] targets must name at least one platform (supported: android)",
            ));
        }
        for target in &self.targets {
            if !SUPPORTED_TARGETS.contains(&target.as_str()) {
                return Err(config_err(format!(
                    "[mobile] target '{target}' is not implemented; this toolchain supports: {}",
                    SUPPORTED_TARGETS.join(", ")
                )));
            }
        }

        if self.screens.is_empty() {
            return Err(config_err(
                "[mobile] needs at least one [[mobile.screens]] entry with a label and an asset",
            ));
        }
        for (index, screen) in self.screens.iter().enumerate() {
            if screen.label.trim().is_empty() {
                return Err(config_err(format!(
                    "[[mobile.screens]] entry {} has no label",
                    index + 1
                )));
            }
            validate_asset_name(index, &screen.asset)?;
            if self.screens[..index]
                .iter()
                .any(|earlier| earlier.label == screen.label)
            {
                return Err(config_err(format!(
                    "[[mobile.screens]] label '{}' is used more than once",
                    screen.label
                )));
            }
            if self.screens[..index]
                .iter()
                .any(|earlier| earlier.asset == screen.asset)
            {
                return Err(config_err(format!(
                    "[[mobile.screens]] asset '{}' is used more than once",
                    screen.asset
                )));
            }
        }
        if !self
            .screens
            .iter()
            .any(|screen| screen.label == self.initial_screen)
        {
            return Err(config_err(format!(
                "[mobile] initial_screen '{}' does not match any screen label ({})",
                self.initial_screen,
                self.screens
                    .iter()
                    .map(|screen| format!("'{}'", screen.label))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        for (key, value) in [("logo", &self.logo), ("splash", &self.splash)] {
            if let Some(path) = value {
                if !is_safe_relative(path) {
                    return Err(config_err(format!(
                        "[mobile] {key} must be a project-relative path without '..': {path}"
                    )));
                }
            }
        }
        Ok(())
    }

    fn initial_index(&self) -> usize {
        self.screens
            .iter()
            .position(|screen| screen.label == self.initial_screen)
            .unwrap_or(0)
    }

    /// The NXL prelude that is prepended to every screen program on the device.
    fn prelude(&self) -> String {
        let labels = self
            .screens
            .iter()
            .map(|screen| format!("\"{}\"", nxl_escape(&screen.label)))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "// Generated by `nexora` from nxl.toml [mobile]. Do not edit by hand;\n\
             // change nxl.toml and run `nexora build` again.\n\
             let app_name = \"{}\";\n\
             let app_version = \"{}\";\n\
             let welcome_heading = \"{}\";\n\
             let welcome_body = \"{}\";\n\
             let screen_labels = [{}];\n",
            nxl_escape(&self.app_name),
            nxl_escape(&self.version_name),
            nxl_escape(&self.welcome_heading),
            nxl_escape(&self.welcome_body),
            labels,
        )
    }
}

fn default_screens() -> Vec<MobileScreen> {
    [
        ("Welcome", "welcome.nxl"),
        ("Features", "features.nxl"),
        ("About", "about.nxl"),
    ]
    .into_iter()
    .map(|(label, asset)| MobileScreen {
        label: label.to_owned(),
        asset: asset.to_owned(),
    })
    .collect()
}

/// A package id that compiles and passes the Play Store's shape rules closely
/// enough for a starter: at least two dot-separated segments, each starting
/// with a letter, containing only letters, digits and underscores.
pub fn default_application_id(project_name: &str) -> String {
    let mut slug: String = project_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    if slug.is_empty() {
        slug = "app".to_owned();
    }
    if slug.starts_with(|c: char| c.is_ascii_digit()) {
        slug.insert_str(0, "app");
    }
    format!("com.nexora.{slug}")
}

fn validate_application_id(value: &str) -> CliResult<()> {
    let segments: Vec<&str> = value.split('.').collect();
    let valid = segments.len() >= 2
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment.starts_with(|c: char| c.is_ascii_alphabetic())
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
    if valid {
        Ok(())
    } else {
        Err(config_err(format!(
            "[mobile] application_id '{value}' is not a valid Android package id; \
             use at least two dot-separated segments, e.g. com.example.myapp"
        )))
    }
}

fn validate_asset_name(index: usize, asset: &str) -> CliResult<()> {
    let name = format!("[[mobile.screens]] entry {} asset", index + 1);
    if asset.trim().is_empty() {
        return Err(config_err(format!("{name} must not be empty")));
    }
    if !asset.ends_with(".nxl") {
        return Err(config_err(format!("{name} '{asset}' must end in .nxl")));
    }
    if asset.contains('/') || asset.contains('\\') || asset.contains("..") {
        return Err(config_err(format!(
            "{name} '{asset}' must be a file name in app/src/main/assets/, not a path"
        )));
    }
    Ok(())
}

fn normalize_color(key: &str, value: &str) -> CliResult<String> {
    let valid = value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|c| c.is_ascii_hexdigit());
    if !valid {
        return Err(config_err(format!(
            "[mobile] {key} '{value}' must be a #RRGGBB colour"
        )));
    }
    Ok(format!("#{}", value[1..].to_ascii_uppercase()))
}

/// Write the generated Android side of the project from `config`.
///
/// This is the only writer of `app/nxl.gradle`, `res/values/nxl.xml`,
/// `assets/mobile.nxl` and the two splash/logo drawables. The activity and the
/// Gradle build files are authored template files and are never overwritten.
pub fn sync_android(dir: &Path, config: &MobileConfig) -> CliResult<()> {
    let result_values = dir.join("app/src/main/res/values");
    let result_drawable = dir.join("app/src/main/res/drawable");
    let assets = dir.join("app/src/main/assets");
    for path in [&result_values, &result_drawable, &assets] {
        fs::create_dir_all(path).map_err(|error| {
            CliError::new(
                BUILD_ERROR,
                format!("cannot create {}: {error}", path.display()),
            )
        })?;
    }

    write_file(
        &dir.join("app/nxl.gradle"),
        &gradle_extensions(config),
        "generated Gradle values",
    )?;
    write_file(
        &result_values.join("nxl.xml"),
        &values_xml(config),
        "generated Android resources",
    )?;
    write_file(
        &assets.join("mobile.nxl"),
        &config.prelude(),
        "generated NXL prelude",
    )?;

    let has_splash = install_drawable(dir, config.splash.as_deref(), "nxl_splash_image", false)?;
    install_drawable(dir, config.logo.as_deref(), "nxl_logo", true)?;
    write_file(
        &result_drawable.join("nxl_splash.xml"),
        &splash_xml(has_splash),
        "generated splash drawable",
    )?;
    Ok(())
}

fn write_file(path: &Path, contents: &str, what: &str) -> CliResult<()> {
    fs::write(path, contents).map_err(|error| {
        CliError::new(
            BUILD_ERROR,
            format!("cannot write {what} to {}: {error}", path.display()),
        )
    })
}

fn gradle_extensions(config: &MobileConfig) -> String {
    format!(
        "// Generated by `nexora` from nxl.toml [mobile]. Do not edit.\n\
         ext {{\n\
         \x20   nxlApplicationId = \"{}\"\n\
         \x20   nxlVersionName = \"{}\"\n\
         \x20   nxlVersionCode = {}\n\
         }}\n",
        groovy_escape(&config.application_id),
        groovy_escape(&config.version_name),
        config.version_code,
    )
}

fn values_xml(config: &MobileConfig) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <!-- Generated by `nexora` from nxl.toml [mobile]. Do not edit by hand. -->\n\
         <resources>\n",
    );
    xml.push_str(&format!(
        "    <string name=\"nxl_app_name\">{}</string>\n",
        xml_escape(&config.app_name)
    ));
    xml.push_str(&format!(
        "    <color name=\"nxl_primary\">{}</color>\n",
        config.primary_color
    ));
    xml.push_str(&format!(
        "    <color name=\"nxl_secondary\">{}</color>\n",
        config.secondary_color
    ));
    for (name, value) in [
        ("nxl_background", "#0B0E14"),
        ("nxl_surface", "#141A24"),
        ("nxl_surface_raised", "#1B2432"),
        ("nxl_border", "#263243"),
        ("nxl_text_primary", "#E8EEF7"),
        ("nxl_text_secondary", "#93A3B8"),
    ] {
        xml.push_str(&format!("    <color name=\"{name}\">{value}</color>\n"));
    }
    xml.push_str("    <string-array name=\"nxl_screen_labels\">\n");
    for screen in &config.screens {
        xml.push_str(&format!(
            "        <item>{}</item>\n",
            xml_escape(&screen.label)
        ));
    }
    xml.push_str("    </string-array>\n");
    xml.push_str("    <string-array name=\"nxl_screen_assets\">\n");
    for screen in &config.screens {
        xml.push_str(&format!(
            "        <item>{}</item>\n",
            xml_escape(&screen.asset)
        ));
    }
    xml.push_str("    </string-array>\n");
    xml.push_str(&format!(
        "    <integer name=\"nxl_initial_screen\">{}</integer>\n",
        config.initial_index()
    ));
    xml.push_str("</resources>\n");
    xml
}

fn splash_xml(has_image: bool) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <!-- Generated by `nexora` from nxl.toml [mobile]. Do not edit by hand. -->\n\
         <layer-list xmlns:android=\"http://schemas.android.com/apk/res/android\">\n\
         \x20   <item android:drawable=\"@color/nxl_background\" />\n",
    );
    if has_image {
        xml.push_str(
            "    <item>\n\
             \x20       <bitmap android:gravity=\"center\" android:src=\"@drawable/nxl_splash_image\" />\n\
             \x20   </item>\n",
        );
    }
    xml.push_str("</layer-list>\n");
    xml
}

/// Copy an optional logo/splash into `res/drawable`, removing a stale file with
/// the same resource name first so a changed extension does not leave two.
///
/// `allow_vector` is true for the logo, which the activity loads with
/// `setImageResource` and may therefore be a vector drawable. The splash is
/// referenced from a `<bitmap>` layer, so it must be a raster image.
fn install_drawable(
    dir: &Path,
    source: Option<&str>,
    stem: &str,
    allow_vector: bool,
) -> CliResult<bool> {
    let drawable = dir.join("app/src/main/res/drawable");
    fs::create_dir_all(&drawable).map_err(|error| {
        CliError::new(
            BUILD_ERROR,
            format!("cannot create {}: {error}", drawable.display()),
        )
    })?;

    // Validate the source before touching an existing drawable: a bad manifest
    // must not leave the project without the resource it had before.
    let prepared = if let Some(relative) = source {
        let from = dir.join(relative);
        if !from.is_file() {
            return Err(config_err(format!(
                "[mobile] asset '{relative}' does not exist (looked in {})",
                from.display()
            )));
        }
        let extension = from
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let allowed: &[&str] = if allow_vector {
            &["png", "jpg", "jpeg", "webp", "xml"]
        } else {
            &["png", "jpg", "jpeg", "webp"]
        };
        if !allowed.contains(&extension.as_str()) {
            let hint = if allow_vector {
                "png, jpg, jpeg, webp or an Android vector xml"
            } else {
                "png, jpg, jpeg or webp"
            };
            return Err(config_err(format!(
                "[mobile] asset '{relative}' has unsupported extension '.{extension}'; use {hint}"
            )));
        }
        Some((from, extension))
    } else {
        None
    };

    if let Ok(entries) = fs::read_dir(&drawable) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_stem().and_then(|name| name.to_str()) == Some(stem) {
                fs::remove_file(&path).map_err(|error| {
                    CliError::new(
                        BUILD_ERROR,
                        format!("cannot replace {}: {error}", path.display()),
                    )
                })?;
            }
        }
    }

    let Some((from, extension)) = prepared else {
        return Ok(false);
    };
    let to = drawable.join(format!("{stem}.{extension}"));
    fs::copy(&from, &to).map_err(|error| {
        CliError::new(
            BUILD_ERROR,
            format!(
                "cannot copy {} to {}: {error}",
                from.display(),
                to.display()
            ),
        )
    })?;
    Ok(true)
}

/// Check that the Android side of the project exists and that every screen
/// compiles with the generated prelude. Used by `build`, `dev` and `test`.
pub fn validate_project(dir: &Path, config: &MobileConfig) -> CliResult<()> {
    for relative in [
        "settings.gradle",
        "app/build.gradle",
        "app/src/main/AndroidManifest.xml",
        "app/src/main/java/com/nexora/nxl/MainActivity.java",
        "app/src/main/java/com/nexora/nxl/NxlRuntime.java",
    ] {
        if !dir.join(relative).is_file() {
            return Err(config_err(format!(
                "mobile project is missing {relative}; restore the starter or recreate the \
                 project with `nexora new mobile <name>`"
            )));
        }
    }

    let prelude = config.prelude();
    let assets = dir.join("app/src/main/assets");
    for screen in &config.screens {
        let path = assets.join(&screen.asset);
        let source = fs::read_to_string(&path).map_err(|error| {
            config_err(format!(
                "screen '{}' cannot be read from {}: {error}",
                screen.label,
                path.display()
            ))
        })?;
        compile_source(&screen.asset, &format!("{prelude}\n{source}")).map_err(|error| {
            config_err(format!(
                "screen '{}' does not compile: {}",
                screen.label, error.message
            ))
        })?;
    }

    for (key, value) in [("logo", &config.logo), ("splash", &config.splash)] {
        if let Some(relative) = value {
            if !dir.join(relative).is_file() {
                return Err(config_err(format!(
                    "[mobile] {key} '{relative}' does not exist"
                )));
            }
        }
    }
    Ok(())
}

fn compile_source(name: &str, source: &str) -> CliResult<()> {
    let file = SourceFile::new(name, source.to_owned());
    let diagnostic = |error: Diagnostic| {
        CliError::new(
            CONFIG_ERROR,
            format!("error[{}]: {}", error.code, error.message),
        )
    };
    let program = crate::parse_source(&file).map_err(diagnostic)?;
    let hir = crate::hir::analyze(&program, name.to_owned()).map_err(diagnostic)?;
    crate::nir::lower_hir(&hir).map_err(diagnostic)?;
    Ok(())
}

/// Build a debug or release APK. Returns the artifact path.
pub fn build_android(dir: &Path, release: bool) -> CliResult<PathBuf> {
    let config = MobileConfig::load(dir)?.ok_or_else(not_mobile)?;
    sync_android(dir, &config)?;
    validate_project(dir, &config)?;

    let java_home = discover_java_home()?;
    let sdk = discover_android_sdk()?;
    let task = if release {
        "assembleRelease"
    } else {
        "assembleDebug"
    };
    let output = run_gradle(dir, &java_home, &sdk, task)?;
    if !output.status.success() {
        return Err(CliError::new(
            BUILD_ERROR,
            format!("Android build failed ({task}).\n{}", command_tail(&output)),
        ));
    }
    artifact_path(dir, release)
}

/// Build the debug APK, install it on the selected device, and launch it.
pub fn dev_android(dir: &Path) -> CliResult<()> {
    let config = MobileConfig::load(dir)?.ok_or_else(not_mobile)?;
    let apk = build_android(dir, false)?;
    let sdk = discover_android_sdk()?;
    let adb = adb_binary(&sdk)?;
    let serial = select_device(&adb)?;

    let install = Command::new(&adb)
        .args(["-s", &serial, "install", "-r"])
        .arg(&apk)
        .output()
        .map_err(|error| {
            CliError::new(
                DEVICE_ERROR,
                format!("cannot run {}: {error}", adb.display()),
            )
        })?;
    if !install.status.success() {
        return Err(CliError::new(
            DEVICE_ERROR,
            format!(
                "installing {} on {serial} failed.\n{}",
                apk.display(),
                command_tail(&install)
            ),
        ));
    }

    let component = format!("{}/com.nexora.nxl.MainActivity", config.application_id);
    let launch = Command::new(&adb)
        .args(["-s", &serial, "shell", "am", "start", "-n", &component])
        .output()
        .map_err(|error| {
            CliError::new(
                DEVICE_ERROR,
                format!("cannot run {}: {error}", adb.display()),
            )
        })?;
    if !launch.status.success() {
        return Err(CliError::new(
            DEVICE_ERROR,
            format!(
                "the APK installed, but launching {component} on {serial} failed.\n{}",
                command_tail(&launch)
            ),
        ));
    }

    println!(
        "installed and launched {} on {serial}\nartifact: {}",
        config.application_id,
        apk.display()
    );
    Ok(())
}

fn not_mobile() -> CliError {
    CliError::new(
        NOT_MOBILE,
        "this is not a mobile project: nxl.toml has no [mobile] section.\n\
         Create one with `nexora new mobile <name>`, or add [mobile] to an existing project.",
    )
}

fn run_gradle(dir: &Path, java_home: &Path, sdk: &Path, task: &str) -> CliResult<Output> {
    let mut command = if cfg!(windows) && dir.join("gradlew.bat").is_file() {
        let mut command = Command::new("cmd");
        command.args(["/C", "gradlew.bat"]);
        command
    } else if dir.join("gradlew").is_file() {
        let mut command = Command::new("sh");
        command.arg("gradlew");
        command
    } else {
        // No wrapper in the project: use a Gradle on PATH, documented as the
        // fallback rather than assumed.
        Command::new("gradle")
    };

    command
        .arg(task)
        .arg("--console=plain")
        .current_dir(dir)
        .env("JAVA_HOME", java_home)
        .env("ANDROID_HOME", sdk)
        .env("ANDROID_SDK_ROOT", sdk);

    command.output().map_err(|error| {
        CliError::new(
            TOOLCHAIN_ERROR,
            format!(
                "cannot start the Android build: {error}\n\
                 Install a JDK 17+ (set JAVA_HOME) and the Android SDK (set ANDROID_HOME); \
                 see docs/25-mobile.md."
            ),
        )
    })
}

fn artifact_path(dir: &Path, release: bool) -> CliResult<PathBuf> {
    let base = dir.join("app/build/outputs/apk");
    let direct = if release {
        base.join("release/app-release.apk")
    } else {
        base.join("debug/app-debug.apk")
    };
    if direct.is_file() {
        return Ok(direct);
    }
    if release {
        // An unsigned release is still a valid artifact; prefer a signed one.
        let explicit = base.join("release/app-release-unsigned.apk");
        if explicit.is_file() {
            return Ok(explicit);
        }
    }
    Err(CliError::new(
        BUILD_ERROR,
        format!(
            "the build succeeded but no APK was found under {}",
            base.display()
        ),
    ))
}

/// JDK home: `JAVA_HOME`, then Android Studio's bundled JBR, then the usual
/// locations. Returns an actionable error rather than an opaque Gradle failure.
pub fn discover_java_home() -> CliResult<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(home));
    }
    if cfg!(windows) {
        if let Some(program_files) = env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(program_files).join("Android/Android Studio/jbr"));
        }
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local).join("Programs/Android Studio/jbr"));
        }
        candidates.push(PathBuf::from(
            r"C:\Program Files\Android\Android Studio\jbr",
        ));
    } else if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from(
            "/Applications/Android Studio.app/Contents/jbr/Contents/Home",
        ));
    } else {
        candidates.push(PathBuf::from("/opt/android-studio/jbr"));
        candidates.push(PathBuf::from("/usr/lib/jvm/default-java"));
    }

    for home in candidates {
        if java_binary(&home).is_file() {
            return Ok(home);
        }
    }
    Err(CliError::new(
        TOOLCHAIN_ERROR,
        "no JDK found: set JAVA_HOME to a JDK 17+ installation (Android Studio's bundled \
         JBR is detected automatically)",
    ))
}

/// Android SDK root: `ANDROID_HOME`, `ANDROID_SDK_ROOT`, then common installs.
pub fn discover_android_sdk() -> CliResult<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for variable in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(value) = env::var_os(variable) {
            candidates.push(PathBuf::from(value));
        }
    }
    if cfg!(windows) {
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local).join("Android/Sdk"));
        }
        candidates.push(PathBuf::from(r"C:\Android\Sdk"));
    } else if cfg!(target_os = "macos") {
        if let Some(home) = env::var_os("HOME") {
            candidates.push(PathBuf::from(home).join("Library/Android/sdk"));
        }
    } else if let Some(home) = env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join("Android/Sdk"));
    }

    for sdk in candidates {
        if sdk.join("platforms").is_dir() || sdk.join("platform-tools").is_dir() {
            return Ok(sdk);
        }
    }
    Err(CliError::new(
        TOOLCHAIN_ERROR,
        "no Android SDK found: install it with Android Studio, or set ANDROID_HOME to the \
         SDK directory (it must contain platforms/ and platform-tools/)",
    ))
}

fn adb_binary(sdk: &Path) -> CliResult<PathBuf> {
    let adb = sdk
        .join("platform-tools")
        .join(if cfg!(windows) { "adb.exe" } else { "adb" });
    if adb.is_file() {
        Ok(adb)
    } else {
        Err(CliError::new(
            TOOLCHAIN_ERROR,
            format!(
                "adb was not found at {}; install the SDK platform-tools package",
                adb.display()
            ),
        ))
    }
}

fn select_device(adb: &Path) -> CliResult<String> {
    let output = Command::new(adb).arg("devices").output().map_err(|error| {
        CliError::new(
            DEVICE_ERROR,
            format!("cannot run {}: {error}", adb.display()),
        )
    })?;
    let text = String::from_utf8_lossy(&output.stdout);
    let devices: Vec<String> = text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?;
            let state = fields.next()?;
            (state == "device").then(|| serial.to_owned())
        })
        .collect();

    if let Ok(wanted) = env::var("ANDROID_SERIAL") {
        if devices.iter().any(|serial| serial == &wanted) {
            return Ok(wanted);
        }
        return Err(CliError::new(
            DEVICE_ERROR,
            format!(
                "ANDROID_SERIAL={wanted} is not a connected, ready device (ready: {})",
                if devices.is_empty() {
                    "none".to_owned()
                } else {
                    devices.join(", ")
                }
            ),
        ));
    }
    match devices.len() {
        1 => Ok(devices[0].clone()),
        0 => Err(CliError::new(
            DEVICE_ERROR,
            "no Android device or emulator is connected.\n\
             Start one (Android Studio > Device Manager, or `emulator -avd <name>`), \
             enable USB debugging for a physical device, then retry `nexora dev`.\n\
             The APK was built; use `nexora build --target android` to keep the artifact.",
        )),
        _ => Err(CliError::new(
            DEVICE_ERROR,
            format!(
                "more than one Android device is connected ({}). Set ANDROID_SERIAL to the \
                 serial you want.",
                devices.join(", ")
            ),
        )),
    }
}

fn command_tail(output: &Output) -> String {
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(40);
    let tail = lines[start..].join("\n");
    if tail.chars().count() > 6000 {
        tail.chars().skip(tail.chars().count() - 6000).collect()
    } else {
        tail
    }
}

fn java_binary(home: &Path) -> PathBuf {
    home.join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" })
}

fn package_field<'a>(toml: &'a str, key: &str) -> Option<String> {
    let mut in_package = false;
    for raw in toml.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some((name, value)) = line.split_once('=') {
            if name.trim() == key {
                return Some(value.trim().trim_matches('"').trim_matches('\'').to_owned());
            }
        }
    }
    None
}

fn has_mobile_section(toml: &str) -> bool {
    toml.lines().any(|line| line.trim() == "[mobile]")
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn parse_string(key: &str, value: &str) -> CliResult<String> {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        let mut out = String::new();
        let mut chars = value[1..value.len() - 1].chars();
        while let Some(character) = chars.next() {
            if character != '\\' {
                out.push(character);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    return Err(config_err(format!(
                        "[mobile] {key}: unsupported escape '\\{other}'"
                    )))
                }
                None => {
                    return Err(config_err(format!(
                        "[mobile] {key}: string ends with a backslash"
                    )))
                }
            }
        }
        return Ok(out);
    }
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        return Ok(value[1..value.len() - 1].to_owned());
    }
    Ok(value.to_owned())
}

fn parse_array(key: &str, value: &str) -> CliResult<Vec<String>> {
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        return Err(config_err(format!(
            "[mobile] {key} must be an inline array, e.g. [\"android\"]"
        )));
    }
    let inner = value[1..value.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|item| parse_string(key, item.trim()))
        .collect()
}

fn is_safe_relative(path: &str) -> bool {
    let path = Path::new(path);
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| !matches!(component, Component::ParentDir))
}

fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(character),
        }
    }
    // A leading @ or ? would be read as a resource reference by aapt.
    if out.starts_with('@') || out.starts_with('?') {
        out.insert(0, '\\');
    }
    out
}

fn nxl_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(character),
        }
    }
    out
}

fn groovy_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(character),
        }
    }
    out
}

fn config_err(message: impl Into<String>) -> CliError {
    CliError::new(CONFIG_ERROR, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r##"
[package]
name = "demo"
version = "1.2.3"
entry = "src/main.nxl"

[mobile]
app_name = "Demo App"
application_id = "com.example.demo"
primary_color = "#112233"
secondary_color = "#445566"
welcome_heading = "Hello"
welcome_body = "From NXL"
initial_screen = "About"
targets = ["android"]

[[mobile.screens]]
label = "Welcome"
asset = "welcome.nxl"

[[mobile.screens]]
label = "About"
asset = "about.nxl"
"##;

    #[test]
    fn defaults_are_complete_and_mobile_is_absent_when_not_declared() {
        let config = MobileConfig::parse("[package]\nname = \"plain\"\n", "plain").unwrap();
        assert!(!config.present);
        assert_eq!(config.screens.len(), 3);
        assert_eq!(config.targets, ["android"]);
    }

    #[test]
    fn a_mobile_section_is_parsed_including_screens_and_initial_destination() {
        let config = MobileConfig::parse(MANIFEST, "demo").unwrap();
        assert!(config.present);
        assert_eq!(config.app_name, "Demo App");
        assert_eq!(config.application_id, "com.example.demo");
        assert_eq!(config.primary_color, "#112233");
        assert_eq!(config.version_name, "1.2.3");
        assert_eq!(config.screens.len(), 2);
        assert_eq!(config.initial_index(), 1);
        assert_eq!(config.welcome_heading, "Hello");
    }

    #[test]
    fn welcome_heading_defaults_to_the_final_app_name() {
        let config = MobileConfig::parse(
            "[package]\nname = \"demo\"\n\n[mobile]\napp_name = \"Renamed\"\n",
            "demo",
        )
        .unwrap();
        assert_eq!(config.welcome_heading, "Welcome to Renamed");
        assert_eq!(config.initial_screen, "Welcome");
    }

    #[test]
    fn invalid_identity_colour_target_and_screens_are_refused() {
        for (needle, replacement) in [
            (
                "application_id = \"com.example.demo\"",
                "application_id = \"demo\"",
            ),
            ("primary_color = \"#112233\"", "primary_color = \"blue\""),
            ("targets = [\"android\"]", "targets = [\"ios\"]"),
            ("initial_screen = \"About\"", "initial_screen = \"Missing\""),
            ("asset = \"about.nxl\"", "asset = \"about.txt\""),
        ] {
            let broken = MANIFEST.replace(needle, replacement);
            let error = MobileConfig::parse(&broken, "demo").unwrap_err();
            assert_eq!(error.code, CONFIG_ERROR, "{error:?}");
        }
    }

    #[test]
    fn unknown_keys_are_refused_rather_than_silently_dropped() {
        let broken = MANIFEST.replace("app_name = \"Demo App\"", "app_nmae = \"Demo App\"");
        let error = MobileConfig::parse(&broken, "demo").unwrap_err();
        assert!(error.message.contains("unknown key"), "{error:?}");
    }

    #[test]
    fn generated_resources_carry_the_configuration_and_escape_values() {
        let config =
            MobileConfig::parse(&MANIFEST.replace("Demo App", "A&B <Demo>"), "demo").unwrap();
        let xml = values_xml(&config);
        assert!(xml.contains("A&amp;B &lt;Demo&gt;"));
        assert!(xml.contains("<color name=\"nxl_primary\">#112233</color>"));
        assert!(xml.contains("<integer name=\"nxl_initial_screen\">1</integer>"));
        assert!(xml.contains("<item>about.nxl</item>"));

        let prelude = config.prelude();
        assert!(prelude.contains("let app_name = \"A&B <Demo>\";"));
        assert!(prelude.contains("let app_version = \"1.2.3\";"));
        assert!(prelude.contains("let screen_labels = [\"Welcome\", \"About\"];"));
    }

    #[test]
    fn application_ids_are_derived_from_project_names() {
        assert_eq!(default_application_id("my-app"), "com.nexora.myapp");
        assert_eq!(default_application_id("2cool"), "com.nexora.app2cool");
        assert_eq!(default_application_id("---"), "com.nexora.app");
    }
}
