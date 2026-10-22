# PROJECT_NAME

An Android application whose screen content is produced by NXL.

The activity, the views, the taps, the header and the tab bar are Android's.
The content area is a NXL program in `app/src/main/assets/`, compiled and run on
the device by the same compiler and virtual machine the desktop toolchain uses,
through a JNI bridge.

## Requirements

| Tool | Version | Why |
| --- | --- | --- |
| JDK | 17 or newer | Gradle and the Android plugin |
| Android SDK | platform 35, build-tools 35 | `compileSdk` / `targetSdk` |
| Connected device or emulator | API 21+ | `nexora dev` installs and launches |

`nexora` finds the JDK through `JAVA_HOME`, then through Android Studio's
bundled JBR. It finds the SDK through `ANDROID_HOME` or `ANDROID_SDK_ROOT`, then
through the usual install locations. You do not need Rust or the NDK to build:
the two `libnxl_android.so` files under `app/src/main/jniLibs/` are prebuilt
from the NXL compiler release that created this project.

No network connection is needed after Gradle and the Android plugin have been
downloaded once.

## Commands

```sh
nexora test                       # host-side NXL tests + screen validation
nexora build --target android     # debug APK
nexora build --release --target android
nexora dev                        # build, install and launch on a device
```

`nexora build` defaults to Android in this project, so `--target android` is
optional. The APK lands in `app/build/outputs/apk/`.

## Customizing

Edit the `[mobile]` section of `nxl.toml`, then run `nexora build` again. The
generated files are rewritten from the manifest; do not edit them by hand.

| Key | What it changes |
| --- | --- |
| `app_name` | Launcher label and in-app header |
| `application_id` | Installed package id, e.g. `com.example.myapp` |
| `primary_color` | Accent: active tab, bullets, splash background |
| `secondary_color` | Logo badge and subtitle tone |
| `welcome_heading` | First heading on the welcome screen |
| `welcome_body` | Opening paragraph on the welcome screen |
| `initial_screen` | Tab shown at launch (matches a screen label) |
| `targets` | Supported platforms; only `android` today |
| `logo` | Optional PNG/JPG/JPEG/WebP/Android vector XML path, shown in the header |
| `splash` | Optional PNG/JPG/JPEG/WebP path, centred on the splash background |
| `[[mobile.screens]]` | Tab `label` and `asset` pairs, in order |

The welcome text is also a normal NXL program: `app/src/main/assets/welcome.nxl`.
Change it there if you want new structure, or change `welcome_heading` /
`welcome_body` in `nxl.toml` for copy only. `mobile.nxl` is generated and is
prepended to every screen, so it is not the place for edits.

## Signing a release

Release signing is read from the environment at build time. Nothing is written
into the project, and `*.jks`, `*.keystore` and `keystore.properties` are
ignored by git.

```sh
NXL_ANDROID_KEYSTORE=/secure/path/release.jks \
NXL_ANDROID_KEYSTORE_PASSWORD=... \
NXL_ANDROID_KEY_ALIAS=... \
NXL_ANDROID_KEY_PASSWORD=... \
nexora build --release --target android
```

Without those variables the release build is unsigned, which is deliberate: an
unsigned APK is a real artifact, and a checked-in keystore is not.

## Honest limits

- Android is the only mobile target implemented. iOS is not.
- The NXL runtime is a prebuilt `.so` for `x86_64` and `arm64-v8a`. Rebuilding
  it from source requires the NXL repository and an Android NDK; the desktop
  `nexora` command does not cross-compile it.
- `nexora test` runs NXL tests on the desktop VM and compiles every screen
  asset. It does not run on-device instrumentation tests.
- The starter has no database, network permission or background service.
- Content markers (`#`, `-`, blank line) are a convention between the NXL
  programs and the activity, not a layout language.

## Layout

```
nxl.toml                              project manifest + [mobile] config
src/main.nxl                          host entry point (nexora run/check)
tests/basic.nxl                       host tests (nexora test)
app/build.gradle                      Android application module
app/nxl.gradle                        generated app id + version (do not edit)
app/src/main/AndroidManifest.xml      no permissions
app/src/main/java/com/nexora/nxl/     activity + JNI bridge
app/src/main/assets/                  NXL programs, one per tab
app/src/main/res/values/nxl.xml       generated strings/colours/arrays
app/src/main/res/drawable/nxl_splash.xml
app/src/main/jniLibs/                 prebuilt NXL runtime, two ABIs
```
