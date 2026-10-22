# Mobile applications (Android)

NXL mobile projects are Android applications. The activity, the views, the taps,
the header and the tab bar are Android's; the words in each screen's content
area are produced by NXL programs that the app compiles and runs on the device
through a JNI bridge to the same compiler and virtual machine the desktop
toolchain uses.

Android is the only implemented mobile target. iOS is not implemented, and the
CLI says so instead of scaffolding a project that cannot build.

## Requirements

| Tool | Version | Required for |
| --- | --- | --- |
| JDK | 17 or newer | `build`, `dev` |
| Android SDK | platform 35, build-tools 35 | `build`, `dev` |
| Device or emulator | API 21+, USB debugging | `dev` only |

`nexora` finds the JDK through `JAVA_HOME`, then through Android Studio's
bundled JBR (the usual install locations are checked). It finds the SDK through
`ANDROID_HOME` or `ANDROID_SDK_ROOT`, then through the common install locations.
Set the variables explicitly in CI:

```sh
export JAVA_HOME=/path/to/jdk-17
export ANDROID_HOME=/path/to/android-sdk
```

Rust and the NDK are **not** required to build a generated project. The starter
ships prebuilt `libnxl_android.so` for `x86_64` (emulators) and `arm64-v8a`
(physical devices). Rebuilding that library from source requires the NXL
repository and the Android NDK; the desktop `nexora` binary does not
cross-compile it.

The Gradle wrapper is committed, so a Gradle installation is not required. The
first build downloads Gradle and the Android Gradle Plugin if they are not in
the Gradle cache; after that the build works offline.

## Quick start

```sh
nexora new mobile my-mobile-app
cd my-mobile-app
nexora test
nexora build --target android
nexora dev
```

`nexora build` prints the artifact path, normally
`app/build/outputs/apk/debug/app-debug.apk`. `nexora dev` builds the debug APK,
installs it on the single connected device, and launches
`com.nexora.nxl.MainActivity`. Start an emulator or connect a device first.

## Commands

| Command | Behaviour |
| --- | --- |
| `new mobile <name>` | Create an Android starter project. The application id defaults to `com.nexora.<lowercase name without punctuation>`. |
| `test [filter]` | Run every `tests/*.nxl` file on the desktop VM, **and** compile every screen asset with the generated prelude. This catches a broken screen before install. It does not run on-device instrumentation tests. |
| `build [--target android] [--release]` | Regenerate the Android resources from `[mobile]`, verify the screens, run Gradle, and report the APK. In a mobile project Android is the default target, so the flag is optional. |
| `dev` | Build the debug APK, select the connected device (or `ANDROID_SERIAL`), install it with `adb install -r`, and launch the activity. A mobile project does not start the web dev server. |
| `build --target <other>` | Refused with `NXL12025`. Only `android` is implemented. |

`nexora run`, `check`, `fmt`, `lint` and the package commands keep working on
the host entry point in `src/main.nxl`; they do not touch the Android app.

## Configuration

`nxl.toml` is the single source of truth. `build` and `dev` regenerate
`app/nxl.gradle`, `app/src/main/res/values/nxl.xml`,
`app/src/main/assets/mobile.nxl` and the splash drawables from the `[mobile]`
section. Edit the manifest, not the generated files.

```toml
[mobile]
app_name = "Field Notes"
application_id = "com.example.fieldnotes"
primary_color = "#FF8800"
secondary_color = "#223344"
welcome_heading = "Welcome to Field Notes"
welcome_body = "The content of this screen was produced by NXL."
initial_screen = "Welcome"
logo = "branding/logo.png"       # optional
splash = "branding/splash.png"   # optional
targets = ["android"]

[[mobile.screens]]
label = "Welcome"
asset = "welcome.nxl"

[[mobile.screens]]
label = "Features"
asset = "features.nxl"

[[mobile.screens]]
label = "About"
asset = "about.nxl"
```

| Key | Default | What it changes |
| --- | --- | --- |
| `app_name` | `[package] name` | Launcher label, in-app header and the letter badge when no logo is set. |
| `application_id` | `com.nexora.<slug>` | Installed package id. At least two dot-separated segments; each starts with a letter and contains only letters, digits and underscores. The Java namespace stays `com.nexora.nxl`, so changing the id does not rename source files. |
| `primary_color` | `#5AA9FF` | `#RRGGBB` accent: active tab, bullets and splash base. |
| `secondary_color` | `#2B4B73` | `#RRGGBB` logo badge and inactive-title tone. |
| `welcome_heading` | `Welcome to <app_name>` | First heading on the welcome screen. |
| `welcome_body` | a fixed sentence | Opening paragraph on the welcome screen. |
| `initial_screen` | first screen | Label of the tab shown at launch; must match a `[[mobile.screens]]` label. |
| `logo` | — | Project-relative PNG/JPG/JPEG/WebP/Android vector XML. Copied to `res/drawable/nxl_logo`. Without it the header draws the app's first letter on the secondary colour. |
| `splash` | — | Project-relative PNG/JPG/JPEG/WebP. Copied to `res/drawable/nxl_splash_image` and centred on the background during launch. |
| `version_code` | `1` | Android `versionCode`. |
| `targets` | `["android"]` | Supported platforms. Any value other than `android` is refused. |
| `[[mobile.screens]]` | Welcome, Features, About | Ordered tabs. `label` is the button text; `asset` is a file name in `app/src/main/assets/` ending in `.nxl`. |

`welcome.nxl`, `features.nxl` and `about.nxl` are ordinary NXL programs. The
activity prepends the generated `mobile.nxl` prelude to each one, which binds
`app_name`, `app_version`, `welcome_heading`, `welcome_body` and
`screen_labels`. That is why a screen may use `welcome_heading` without an
import, and why running a screen asset alone on the desktop fails: it is half
of a program. `nexora test` compiles the complete pair.

The welcome copy can therefore be customized in two places, deliberately:

- `welcome_heading` / `welcome_body` in `nxl.toml` change copy only.
- `app/src/main/assets/welcome.nxl` changes structure, loops and extra lines.

Validation is strict. Unknown keys, a colour that is not `#RRGGBB`, an invalid
application id, a duplicate label or asset, an asset that is a path, an
`initial_screen` that names no tab, or a target other than `android` fails with
`error[NXL12020]` before Gradle runs.

## Release signing

Signing credentials are read from the environment by `app/build.gradle` at
build time. Nothing is written into the project, and `.gitignore` excludes
`*.jks`, `*.keystore` and `keystore.properties`.

```sh
export NXL_ANDROID_KEYSTORE=/secure/path/release.jks
export NXL_ANDROID_KEYSTORE_PASSWORD=...
export NXL_ANDROID_KEY_ALIAS=...
export NXL_ANDROID_KEY_PASSWORD=...
nexora build --release --target android
```

With those variables the artifact is `app/build/outputs/apk/release/app-release.apk`
and is signed. Without them the build still succeeds and produces
`app-release-unsigned.apk`: an unsigned APK is a real artifact, and a
committed keystore is not. A missing or unreadable keystore fails the Gradle
build rather than silently falling back to an unsigned APK.

## Device and emulator setup

1. Install the SDK platform and platform-tools, and create an AVD with API 21
   or newer (Android Studio's Device Manager is the simplest route).
2. Start the emulator, or connect a physical device with USB debugging enabled
   and authorise the computer.
3. Confirm the device is ready:

   ```sh
   adb devices
   ```

   A usable device shows `device`, not `offline` or `unauthorized`.
4. Run `nexora dev`. If more than one device is attached, set
   `ANDROID_SERIAL=<serial>` to choose one.

`nexora dev` builds before it looks for a device. If no device is connected it
reports `NXL12023` and leaves the debug APK on disk, so the build work is not
lost.

## What the generated app contains

```text
nxl.toml                              project manifest + [mobile] config
src/main.nxl                          host entry point (nexora run/check)
tests/basic.nxl                       host tests (nexora test)
build.gradle, settings.gradle         Gradle 9.3.1 / AGP 9.1.0
gradlew, gradlew.bat, gradle/wrapper  committed wrapper
app/build.gradle                      application module, minSdk 21, targetSdk 35
app/nxl.gradle                        generated application id and version
app/src/main/AndroidManifest.xml      no permissions
app/src/main/java/com/nexora/nxl/     MainActivity + NxlRuntime (JNI bridge)
app/src/main/assets/                  NXL programs, one per tab
app/src/main/res/values/nxl.xml       generated strings, colours and arrays
app/src/main/res/values/styles.xml    dark no-action-bar theme
app/src/main/res/drawable/            generated splash, optional logo/splash images
app/src/main/jniLibs/                 prebuilt NXL runtime, x86_64 + arm64-v8a
```

The app requests no permissions. It has no network, storage, database or
background service.

## Limitations

- **Android only.** The only `[mobile] targets` value that is accepted is
  `android`.
- **Two ABIs.** `x86_64` and `arm64-v8a`. A 32-bit-only device is not
  supported by the prebuilt runtime.
- **API 21–35.** `minSdk` is 21 and `compileSdk`/`targetSdk` are 35.
- **Prebuilt runtime.** Changing the compiler does not automatically update a
  generated app's `.so`; rebuild the Android crate from the NXL repository and
  replace both files.
- **No on-device test runner.** `nexora test` runs NXL tests on the desktop VM
  and compiles every screen asset. It does not install an instrumentation APK.
- **No hot reload.** `dev` builds, installs and launches; edit and run it again
  to see a change.
- **One activity, platform views.** The starter is intentionally small. The
  content markers (`#`, `-`, blank line) are a convention between the NXL
  programs and the activity, not a layout language.
- **The emulator is a host dependency.** A boot failure is an SDK/emulator
  problem (WHPX/Hyper-V, acceleration, system image), not an NXL build failure;
  `nexora build` works without one.

## Error codes

| Code | Meaning |
| --- | --- |
| `NXL12020` | Invalid or incomplete `[mobile]` configuration, missing screen or asset, or a screen that does not compile. |
| `NXL12021` | No JDK or Android SDK found, or Gradle could not be started. |
| `NXL12022` | The Android build failed, or the build produced no APK. The tail of the Gradle output is included. |
| `NXL12023` | No ready device, more than one device without `ANDROID_SERIAL`, or `adb install`/`am start` failed. |
| `NXL12024` | `build --target android` was used outside a mobile project. |
| `NXL12025` | `build --target` named a platform that is not implemented. |

## Verifying a build

```sh
nexora build --target android
aapt2 dump badging app/build/outputs/apk/debug/app-debug.apk
```

The badging output should name the configured `application_id`, show
`application-label` as `app_name`, list `native-code: 'arm64-v8a' 'x86_64'`,
and name `com.nexora.nxl.MainActivity` as the launchable activity. For a
signed release, `apksigner verify --print-certs` reports the signing
certificate.
