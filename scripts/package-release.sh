#!/usr/bin/env bash
# Stage, checksum, verify and archive a NEXORA release for this machine.
#
# Used by the release workflow on native Linux and macOS runners. It refuses to
# produce an artifact for a platform it is not running on, so a mislabelled
# binary can never be published.
#
#   ./scripts/package-release.sh <linux-x64|linux-arm64|macos-arm64|macos-x64>
#
# Quality gates (fmt, clippy, tests) run first; nothing is packaged if they fail.

set -euo pipefail

target="${1:-}"
if [ -z "$target" ]; then
    echo "usage: package-release.sh <linux-x64|linux-arm64|macos-arm64|macos-x64>" >&2
    exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compiler_dir="$repo_root/compiler"
dist_dir="$repo_root/dist"
stage_dir="$dist_dir/$target"
installer_src="$repo_root/install/unix"
version="$(awk -F'"' '/^version *= *"/ { print $2; exit }' "$compiler_dir/Cargo.toml")"

say() { printf '==> %s\n' "$1"; }

# ----------------------------------------------------------------------------
kernel="$(uname -s)"
machine="$(uname -m)"
case "$kernel" in
    Linux) actual_os="linux" ;;
    Darwin) actual_os="macos" ;;
    *) echo "unsupported operating system '$kernel'" >&2; exit 1 ;;
esac
case "$machine" in
    x86_64 | amd64) actual_arch="x64" ;;
    arm64 | aarch64) actual_arch="arm64" ;;
    *) echo "unsupported architecture '$machine'" >&2; exit 1 ;;
esac
if [ "${actual_os}-${actual_arch}" != "$target" ]; then
    echo "refusing to package '$target' on ${actual_os}-${actual_arch}: build each platform on its own runner" >&2
    exit 1
fi

# ----------------------------------------------------------------------------
cd "$compiler_dir"
say "cargo fmt --check"
cargo fmt -- --check
say "cargo clippy -D warnings"
cargo clippy --all-targets -- -D warnings
say "cargo test"
cargo test
say "cargo build --release"
cargo build --release

# ----------------------------------------------------------------------------
say "Staging dist/$target (version $version)"
rm -rf "$stage_dir"
mkdir -p "$stage_dir/examples"
cp "$compiler_dir/target/release/nxlc" "$stage_dir/nxlc"
cp "$installer_src/install.sh" "$stage_dir/install.sh"
cp "$installer_src/README.md" "$stage_dir/README.md"
chmod +x "$stage_dir/install.sh"
for doc in LICENSE CHANGELOG.md SECURITY.md; do
    [ -f "$repo_root/$doc" ] && cp "$repo_root/$doc" "$stage_dir/"
done
cp "$repo_root/examples/hello/src/main.nxl" "$stage_dir/examples/main.nxl"

say "Generating SHA-256 checksums"
(
    cd "$stage_dir"
    if command -v sha256sum >/dev/null 2>&1; then
        find . -maxdepth 1 -type f ! -name checksums.sha256 -printf '%P\n' | sort | xargs sha256sum >checksums.sha256
    else
        find . -maxdepth 1 -type f ! -name checksums.sha256 -exec basename {} \; | sort | xargs shasum -a 256 >checksums.sha256
    fi
)

# ----------------------------------------------------------------------------
say "Verifying the packaged artifact"
"$stage_dir/nxlc" --version
"$stage_dir/nxlc" doctor

say "Smoke-testing the packaged compiler on the bundled example"
example_output="$("$stage_dir/nxlc" run "$stage_dir/examples/main.nxl")"
echo "    example : $example_output"
[ "$example_output" = "30" ] || {
    echo "packaged compiler produced unexpected output: $example_output" >&2
    exit 1
}

# The console template must produce a project the packaged binary can run: that
# is the documented first-run experience.
say "Smoke-testing the console template"
smoke_dir="$(mktemp -d)"
(
    cd "$smoke_dir"
    "$stage_dir/nxlc" new console smoke-app >/dev/null
    cd smoke-app
    template_output="$("$stage_dir/nxlc" run)"
    [ "$template_output" = "Hello, NXL!" ] || {
        echo "unexpected template output: $template_output" >&2
        exit 1
    }
)
rm -rf "$smoke_dir"
echo "    template: Hello, NXL!"
probe="$(mktemp)"
printf 'fn ((( broken' >"$probe"
if "$stage_dir/nxlc" check "$probe" >/dev/null 2>&1; then
    echo "invalid source was accepted; refusing to package" >&2
    rm -f "$probe"
    exit 1
fi
rm -f "$probe"
echo "    invalid input is rejected with a non-zero exit code"

say "Dry-running the installer"
# bash, not sh: the installer uses pipefail, which dash (/bin/sh on Ubuntu)
# does not support.
bash "$stage_dir/install.sh" --dry-run

# ----------------------------------------------------------------------------
say "Creating archive"
archive="$dist_dir/NEXORA-${version}-${target}.tar.gz"
rm -f "$archive"
tar -czf "$archive" -C "$stage_dir" .
if command -v sha256sum >/dev/null 2>&1; then
    (cd "$dist_dir" && sha256sum "$(basename "$archive")" >"$(basename "$archive").sha256")
else
    (cd "$dist_dir" && shasum -a 256 "$(basename "$archive")" >"$(basename "$archive").sha256")
fi

say "Done"
printf '    stage   : %s\n    archive : %s (%s KB)\n' \
    "$stage_dir" "$archive" "$(( $(wc -c <"$archive") / 1024 ))"