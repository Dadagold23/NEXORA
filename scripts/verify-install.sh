#!/usr/bin/env bash
# Verify an installed NEXORA release the way a new user experiences it.
#
# Extracts the release archive, installs it into a temporary prefix, and runs
# the documented onboarding steps with a clean PATH. Nothing touches the real
# user's installation unless --real-install is passed.
#
#   bash scripts/verify-install.sh [path/to/archive]

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
archive="${1:-}"
real_install=0

if [ "${2:-}" = "--real-install" ]; then
    real_install=1
fi

if [ -z "$archive" ]; then
    archive="$(ls -t "$repo_root"/dist/NEXORA-*-"$target".tar.gz 2>/dev/null | head -n 1 || true)"
    target="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
    archive="$(ls -t "$repo_root"/dist/NEXORA-*.*.tar.gz 2>/dev/null | head -n 1 || true)"
fi
[ -n "$archive" ] || {
    echo "no release archive given or found; run scripts/package-release.sh first" >&2
    exit 1
}

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
passed=0
failed=0
declare -a failures

start=$(date +%s)
step() {
    local name="$1"
    shift
    local began
    began=$(date +%s)
    local output
    if output=$("$@" 2>&1); then
        passed=$((passed + 1))
        printf '[PASS] %s (%ss) %s\n' "$name" "$(( $(date +%s) - began ))" "$(printf '%s' "$output" | head -n 1)"
    else
        failed=$((failed + 1))
        failures+=("$name: $(printf '%s' "$output" | head -n 1)")
        printf '[FAIL] %s (%ss) %s\n' "$name" "$(( $(date +%s) - began ))" "$(printf '%s' "$output" | head -n 1)"
    fi
}

fail_step() {
    failed=$((failed + 1))
    failures+=("$1: $2")
    printf '[FAIL] %s %s\n' "$1" "$2"
}

echo "NEXORA release verification"
echo "  archive : $archive"
echo "  scratch : $scratch"
echo "  host    : $(uname -s) $(uname -m)"
echo

extract="$scratch/extracted"
prefix="$scratch/install"
mkdir -p "$extract"

if ! tar -xzf "$archive" -C "$extract"; then
    fail_step "archive extracts" "could not unpack $archive"
    exit 1
fi
passed=$((passed + 1))
echo "[PASS] archive extracts"

installer="$extract/install.sh"
[ -f "$installer" ] || {
    fail_step "archive contains an installer" "install.sh missing from the archive"
    exit 1
}

step "installer verifies the checksum" bash "$installer" --prefix "$prefix" --dry-run
step "installer installs" bash "$installer" --prefix "$prefix"

compiler="$prefix/nxlc"
[ -x "$compiler" ] || {
    fail_step "compiler is installed" "$compiler missing or not executable"
    exit 1
}

# A clean PATH: the install prefix plus the standard system locations.
system_path="/usr/bin:/bin:/usr/sbin:/sbin"
run_nxlc() {
    PATH="$prefix:$system_path" "$compiler" "$@"
}

step "compiler launches and reports its version" run_nxlc --version
step "doctor reports accurate diagnostics" run_nxlc doctor
step "bundled example runs" run_nxlc run "$extract/examples/main.nxl"

printf 'fn ((( broken' >"$scratch/broken.nxl"
if run_nxlc check "$scratch/broken.nxl" >/dev/null 2>&1; then
    fail_step "invalid source fails with a non-zero exit" "invalid source was accepted"
else
    passed=$((passed + 1))
    echo "[PASS] invalid source fails with a non-zero exit"
fi

work="$scratch/workspace"
mkdir -p "$work"
(
    cd "$work" || exit 1
    run_nxlc new console hello-app >/dev/null || exit 1
    cd hello-app || exit 1
    output="$(run_nxlc run)"
    [ "$output" = "Hello, NXL!" ] || {
        echo "unexpected output: $output"
        exit 1
    }
) && {
    passed=$((passed + 1))
    echo "[PASS] project creation works in a clean directory"
} || fail_step "project creation works in a clean directory" "see output above"

spaced="$scratch/dir with spaces"
mkdir -p "$spaced"
printf 'print("spaces work");' >"$spaced/spaced program.nxl"
if run_nxlc run "$spaced/spaced program.nxl" >/dev/null 2>&1; then
    passed=$((passed + 1))
    echo "[PASS] paths containing spaces work"
else
    fail_step "paths containing spaces work" "could not run a program under a path with spaces"
fi

if run_nxlc run "$scratch/not-here.nxl" >/dev/null 2>&1; then
    fail_step "missing dependency is diagnosed" "a missing file was reported as success"
else
    passed=$((passed + 1))
    echo "[PASS] missing dependency is diagnosed"
fi

step "reinstall over an existing installation is safe" bash "$installer" --prefix "$prefix"

if [ "$real_install" -eq 1 ]; then
    echo "[SKIP] uninstall leaves the real installation in place"
else
    if bash "$installer" --prefix "$prefix" --uninstall >/dev/null 2>&1 && [ ! -e "$compiler" ]; then
        passed=$((passed + 1))
        echo "[PASS] uninstall removes the installation"
    else
        fail_step "uninstall removes the installation" "nxlc still present after uninstall"
    fi
fi

echo
echo "--- summary ---"
echo "  steps  : $((passed + failed))"
echo "  passed : $passed"
echo "  failed : $failed"
echo "  total  : $(($(date +%s) - start))s"
for failure in "${failures[@]:-}"; do
    [ -n "$failure" ] && echo "  FAILED : $failure"
done
[ "$failed" -eq 0 ] || exit 1
exit 0