<#
.SYNOPSIS
    Builds and packages the NEXORA Windows x64 release.

.DESCRIPTION
    Runs the release build, stages the platform layout described in
    re_prompt.txt section 4 (dist/windows-x64/), generates SHA-256 checksums,
    verifies the packaged binary actually runs, and produces the release
    archive. Quality gates run first: if formatting, clippy or the test suite
    fail, nothing is packaged and the script exits non-zero.

.PARAMETER SkipTests
    Skip the test-suite gate (for local iteration only; the release notes
    must still state that tests were not run).

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-release.ps1
#>
[CmdletBinding()]
param(
    [switch]$SkipTests
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$compilerDir = Join-Path $repoRoot 'compiler'
$distDir = Join-Path $repoRoot 'dist'
$stageDir = Join-Path $distDir 'windows-x64'
$installerSrc = Join-Path $repoRoot 'install\windows-x64'
$version = (Select-String -Path (Join-Path $compilerDir 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value

function Step { param([string]$m) Write-Host "==> $m" -ForegroundColor Cyan }

Push-Location $compilerDir
try {
    Step 'cargo fmt --check'
    cargo fmt -- --check
    if ($LASTEXITCODE -ne 0) { throw 'cargo fmt --check failed' }

    Step 'cargo clippy -D warnings'
    cargo clippy --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'clippy reported problems' }

    Step 'cargo build --release'
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'release build failed' }
}
finally {
    Pop-Location
}

# Staging happens before the test suite on purpose: the artifact tests in
# `release_tests.rs` check the exact bytes that will ship, including their
# checksums, so they must run against a freshly staged directory.

Step "Staging dist/windows-x64 (version $version)"
if (Test-Path -LiteralPath $stageDir) { Remove-Item -LiteralPath $stageDir -Recurse -Force }
New-Item -ItemType Directory -Path (Join-Path $stageDir 'examples') -Force | Out-Null

Copy-Item -LiteralPath (Join-Path $compilerDir 'target\release\nxlc.exe') -Destination (Join-Path $stageDir 'nxlc.exe')
Copy-Item -LiteralPath (Join-Path $installerSrc 'install.ps1') -Destination $stageDir
Copy-Item -LiteralPath (Join-Path $installerSrc 'install.bat') -Destination $stageDir
Copy-Item -LiteralPath (Join-Path $installerSrc 'README.md') -Destination $stageDir
foreach ($doc in @('LICENSE', 'CHANGELOG.md', 'SECURITY.md')) {
    $from = Join-Path $repoRoot $doc
    if (Test-Path -LiteralPath $from) { Copy-Item -LiteralPath $from -Destination $stageDir }
}
Copy-Item -LiteralPath (Join-Path $repoRoot 'examples\hello\src\main.nxl') -Destination (Join-Path $stageDir 'examples\main.nxl')
# Both project targets are compiled into the compiler, so `nxlc new console` and
# `nxlc new web` work from an installed binary with nothing else on disk.
#
# They are deliberately NOT shipped as files. A `templates/web/src` directory
# sitting next to a real project looks like a project: `cd` into it and
# `nxlc run` reports a missing entry point rather than the fact that it is a
# template. `new` is the supported way to start a project.

Step 'Generating SHA-256 checksums'
$checksums = Join-Path $stageDir 'checksums.sha256'
$hashes = foreach ($file in (Get-ChildItem -LiteralPath $stageDir -File | Where-Object { $_.Name -ne 'checksums.sha256' } | Sort-Object Name)) {
    '{0}  {1}' -f (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $file.Name
}
Set-Content -LiteralPath $checksums -Value $hashes -Encoding ascii

if (-not $SkipTests) {
    Step 'cargo test (validates the staged artifact, not just the dev build)'
    Push-Location $compilerDir
    try {
        cargo test
        if ($LASTEXITCODE -ne 0) { throw 'test suite failed; refusing to package' }
    } finally {
        Pop-Location
    }
} else {
    Write-Warning 'tests were SKIPPED for this build'
}

Step 'Verifying the packaged artifact (not the dev build)'
$packaged = Join-Path $stageDir 'nxlc.exe'
$versionOut = & $packaged --version
if ($LASTEXITCODE -ne 0) { throw 'packaged nxlc.exe did not run' }
Write-Host "    version : $versionOut"
$doctorOut = & $packaged doctor
if ($LASTEXITCODE -ne 0) { throw 'packaged nxlc.exe doctor failed' }
foreach ($doctorLine in $doctorOut) { Write-Host "    doctor  : $doctorLine" }

Step 'Smoke-testing the packaged compiler on the bundled example'
$exampleOut = & $packaged run (Join-Path $stageDir 'examples\main.nxl')
if ($LASTEXITCODE -ne 0) { throw 'packaged nxlc.exe could not run the bundled example' }
Write-Host "    example : $exampleOut"

# The console template must produce a project that the packaged binary can run,
# which is the documented first-run experience.
$smokeRoot = Join-Path $env:TEMP ("nxl_release_smoke_" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $smokeRoot -Force | Out-Null
try {
    Push-Location $smokeRoot
    try {
        $newOut = & $packaged new console smoke-app 2>&1
        if ($LASTEXITCODE -ne 0) { throw "packaged nxlc.exe could not create a project: $newOut" }
        Push-Location (Join-Path $smokeRoot 'smoke-app')
        try {
            $runOut = & $packaged run 2>&1
            if ($LASTEXITCODE -ne 0) { throw "generated project did not run: $runOut" }
            $templateOut = ($runOut | Select-Object -First 1)
            if ("$templateOut" -ne 'Hello, NXL!') { throw "unexpected template output: $templateOut" }
            Write-Host "    template: $templateOut"
        } finally {
            Pop-Location
        }
    } finally {
        Pop-Location
    }
} finally {
    Remove-Item -LiteralPath $smokeRoot -Recurse -Force -ErrorAction SilentlyContinue
}

# Invalid input must fail loudly with a non-zero exit code. Native stderr is
# captured without tripping the error preference.
$badInput = Join-Path $env:TEMP 'nxl_release_probe.toml'
Set-Content -LiteralPath $badInput -Value 'this is not valid NXL {{{' -Encoding utf8
$previousPreference = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$brokenOut = & $packaged check $badInput 2>&1
$brokenExit = $LASTEXITCODE
$ErrorActionPreference = $previousPreference
Remove-Item -LiteralPath $badInput -Force -ErrorAction SilentlyContinue
if ($brokenExit -eq 0) { throw 'invalid source was accepted; the compiler must exit non-zero' }
Write-Host "    invalid input rejected (exit $brokenExit): $(($brokenOut | Select-Object -First 1))"

$archive = Join-Path $distDir ("NEXORA-{0}-windows-x64.zip" -f $version)
Step "Creating $archive"
if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
Compress-Archive -Path (Join-Path $stageDir '*') -DestinationPath $archive -CompressionLevel Optimal
$archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath ("$archive.sha256") -Value ("{0}  {1}" -f $archiveHash, (Split-Path -Leaf $archive)) -Encoding ascii

Step 'Done'
Write-Host "    stage   : $stageDir"
Write-Host "    archive : $archive ($([Math]::Round((Get-Item $archive).Length / 1MB, 2)) MB)"
Write-Host "    sha256  : $archiveHash"
if ($SkipTests) { Write-Warning 'tests were NOT run for this build' }