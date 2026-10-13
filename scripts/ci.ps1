# NEXORA local CI mirror — runs the same checks as .github/workflows/ci.yml.
$ErrorActionPreference = "Stop"
Push-Location (Join-Path (Join-Path $PSScriptRoot "..") "compiler")
try {
    cargo fmt -- --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt --check failed" }

    cargo clippy --all-targets
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed" }

    cargo test --workspace
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }

    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed" }

    Write-Output "local CI: all checks passed"
} finally {
    Pop-Location
}
