#!/usr/bin/env bash
# NEXORA / NXL installer for Linux and macOS.
#
# Verifies the downloaded artifact against its SHA-256 checksum, installs it
# into a user-writable directory, adds that directory to the user's shell
# PATH configuration, and proves the installation works by running the
# installed compiler. Nothing is written unless every check passes, and the
# system-wide environment is never modified.
#
#   ./install.sh              install for the current user
#   ./install.sh --dry-run    verify and show the plan, change nothing
#   ./install.sh --uninstall  remove an existing installation
#   ./install.sh --prefix DIR install into DIR instead of the default

set -euo pipefail

PACKAGE_VERSION="0.1.0"
EXE_NAME="nxlc"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DRY_RUN=0
UNINSTALL=0
PREFIX=""

for arg in "$@"; do
    case "$arg" in
        --dry-run | --dryrun) DRY_RUN=1 ;;
        --uninstall) UNINSTALL=1 ;;
        --prefix)
            shift
            PREFIX="${1:-}"
            if [ -z "$PREFIX" ]; then
                echo "ERROR: --prefix needs a directory" >&2
                exit 1
            fi
            ;;
        -h | --help)
            sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "ERROR: unknown option '$arg'" >&2
            exit 1
            ;;
    esac
    shift || true
done

say() { printf '==> %s\n' "$1"; }
ok() { printf '    ok  %s\n' "$1"; }
warn() { printf '    warn %s\n' "$1" >&2; }
die() {
    printf 'ERROR: %s\n' "$1" >&2
    exit 1
}

# ----------------------------------------------------------------------------
say "Checking platform"
kernel="$(uname -s)"
machine="$(uname -m)"
case "$kernel" in
    Linux) expected_os="linux" ;;
    Darwin) expected_os="macos" ;;
    *) die "unsupported operating system '$kernel'. This package targets Linux and macOS." ;;
esac
case "$machine" in
    x86_64 | amd64) expected_arch="x64" ;;
    arm64 | aarch64) expected_arch="arm64" ;;
    *) die "unsupported CPU architecture '$machine'. Supported: x86_64 and arm64." ;;
esac
# The package directory is named <os>-<arch>; refuse to run the wrong one.
package_id="$(basename "$SCRIPT_DIR")"
if [ "$package_id" != "${expected_os}-${expected_arch}" ] && [ "$package_id" != "unix" ]; then
    die "this package is '$package_id' but you are on ${expected_os}-${expected_arch}. Download the matching release archive."
fi
ok "${kernel} ${machine} (${expected_os}-${expected_arch})"

# ----------------------------------------------------------------------------
say "Verifying artifact integrity"
payload="$SCRIPT_DIR/$EXE_NAME"
checksum_file="$SCRIPT_DIR/checksums.sha256"
[ -f "$payload" ] || die "$EXE_NAME not found next to this script ($SCRIPT_DIR). Re-download the full release package."
[ -f "$checksum_file" ] || die "checksums.sha256 not found next to this script. Refusing to install an unverified binary."

if command -v sha256sum >/dev/null 2>&1; then
    expected="$(awk -v name="$EXE_NAME" '$2 == name || $2 == "*"name { print $1 }' "$checksum_file")"
    actual="$(sha256sum "$payload" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
    expected="$(awk -v name="$EXE_NAME" '$2 == name || $2 == "*"name { print $1 }' "$checksum_file")"
    actual="$(shasum -a 256 "$payload" | awk '{print $1}')"
else
    die "no sha256sum or shasum found, so the download cannot be verified. Install coreutils or run the release archive through your package manager."
fi
[ -n "$expected" ] || die "checksums.sha256 has no entry for $EXE_NAME."
[ "$expected" = "$actual" ] || die "SHA-256 mismatch for $EXE_NAME.
  expected $expected
  actual   $actual
The download is corrupt or tampered with; nothing was installed."
ok "SHA-256 matches (${actual%${actual#????????}}...)"

# ----------------------------------------------------------------------------
if [ -z "$PREFIX" ]; then
    PREFIX="$HOME/.nexora/bin"
fi

if [ "$UNINSTALL" -eq 1 ]; then
    say "Uninstalling"
    if [ -e "$PREFIX/$EXE_NAME" ]; then
        if [ "$DRY_RUN" -eq 1 ]; then
            ok "would remove $PREFIX/$EXE_NAME"
        else
            rm -f "$PREFIX/$EXE_NAME"
            ok "removed $PREFIX/$EXE_NAME"
        fi
    else
        warn "nothing installed at $PREFIX/$EXE_NAME"
    fi
    if [ -f "$HOME/.nexora/env" ]; then
        if [ "$DRY_RUN" -eq 1 ]; then
            ok "would remove $HOME/.nexora/env"
        else
            rm -f "$HOME/.nexora/env"
            ok "removed $HOME/.nexora/env"
        fi
    fi
    say "Done"
    printf 'Remove any remaining PATH line for %s from your shell profile.\n' "$PREFIX"
    exit 0
fi

# ----------------------------------------------------------------------------
say "Installing to $PREFIX"
if [ -d "$PREFIX" ]; then
    foreign="$(find "$PREFIX" -maxdepth 1 -type f ! -name "$EXE_NAME" 2>/dev/null | head -n 1)"
    [ -z "$foreign" ] || die "$PREFIX already contains files this installer did not create ($foreign). Choose --prefix or remove them first; nothing was changed."
    ok "existing installation found; it will be replaced in place (projects are untouched)"
fi

if [ "$DRY_RUN" -eq 1 ]; then
    ok "would copy $EXE_NAME to $PREFIX"
    ok "would add $PREFIX to your shell PATH"
    ok "would verify the installed binary with '$EXE_NAME --version'"
    printf 'Dry run complete. Nothing was written.\n'
    exit 0
fi

mkdir -p "$PREFIX"
cp "$payload" "$PREFIX/$EXE_NAME"
chmod +x "$PREFIX/$EXE_NAME"
ok "installed $EXE_NAME"

# ----------------------------------------------------------------------------
say "Configuring PATH for your shell"
env_file="$HOME/.nexora/env"
mkdir -p "$(dirname "$env_file")"
line="export PATH=\"$PREFIX:\$PATH\""
if [ -f "$env_file" ] && grep -Fq "$line" "$env_file"; then
    ok "PATH entry already present in $env_file"
else
    printf '# Added by the NEXORA installer\n%s\n' "$line" >>"$env_file"
    ok "wrote $env_file"
fi
# Make it work in the current session too.
case ":$PATH:" in
    *":$PREFIX:"*) ok "install directory already on this session's PATH" ;;
    *)
        export PATH="$PREFIX:$PATH"
        ok "added the install directory to this session's PATH"
        ;;
esac
if ! grep -Fq '.nexora/env' "$HOME/.profile" 2>/dev/null; then
    printf '\n# Added by the NEXORA installer\n[ -f "$HOME/.nexora/env" ] && . "$HOME/.nexora/env"\n' >>"$HOME/.profile"
    ok "added a loader line to ~/.profile"
fi

# ----------------------------------------------------------------------------
say "Verifying the installation"
version_output="$("$PREFIX/$EXE_NAME" --version)"
ok "installed compiler reports: $version_output"
"$PREFIX/$EXE_NAME" doctor || true

cat <<EOF

NEXORA $PACKAGE_VERSION installed for the current user.
  compiler : $PREFIX/$EXE_NAME
  project  : nxl new my-app && cd my-app && nxl run

Open a NEW terminal (or run: source ~/.nexora/env) so the PATH change applies.
Uninstall: $0 --uninstall
EOF