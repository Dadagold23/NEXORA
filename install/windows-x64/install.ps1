<#
.SYNOPSIS
    Installs the NEXORA/NXL compiler (nxlc) for the current user.

.DESCRIPTION
    Verifies the downloaded artifact against its SHA-256 checksum, copies it
    into a user-writable directory, adds that directory to the user PATH, and
    proves the installation works by running the installed executable.

    Nothing is installed and no setting is changed unless -DryRun is omitted
    and every verification step passes. System-wide settings are never
    modified and privileges are never elevated.

.PARAMETER DryRun
    Perform every check and print the planned actions without writing files
    or changing PATH.

.PARAMETER Uninstall
    Remove a previously installed copy and its PATH entry.

.PARAMETER InstallDir
    Override the install directory (default: %LOCALAPPDATA%\NEXORA\bin).

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 -DryRun

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 -Uninstall
#>
[CmdletBinding(DefaultParameterSetName = 'Install')]
param(
    [switch]$DryRun,
    [switch]$Uninstall,
    [string]$InstallDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ExeName = 'nxlc.exe'
$script:PackageVersion = '0.1.0'

function Write-Step { param([string]$Message) Write-Host "==> $Message" }
function Write-Ok { param([string]$Message) Write-Host "    ok  $Message" }
function Write-Warn { param([string]$Message) Write-Host "    warn $Message" -ForegroundColor Yellow }
function Fail { param([string]$Message) throw $Message }

# ----------------------------------------------------------------------------
Write-Step 'Checking platform'
# `$IsWindows` only exists in PowerShell Core and `$PSVersionTable.Platform`
# only in newer hosts, so neither is assumed. Windows PowerShell 5.1 cannot run
# this script anywhere but Windows, which makes the default correct there.
$isWindowsHost = $true
if (Test-Path variable:global:IsWindows) { $isWindowsHost = [bool]$global:IsWindows }
if (-not $isWindowsHost) {
    Fail "This installer is for Windows. Use dist/macos-*/install.sh or dist/linux-*/install.sh instead."
}
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne 'AMD64' -and $arch -ne 'x64') {
    Fail "Unsupported CPU architecture '$arch'. This package is windows-x64 (ARM64 is not built yet)."
}
Write-Ok "Windows x64"

# ----------------------------------------------------------------------------
$sourceDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$sourceExe = Join-Path $sourceDir $script:ExeName
$checksumFile = Join-Path $sourceDir 'checksums.sha256'

Write-Step 'Verifying artifact integrity'
if (-not (Test-Path -LiteralPath $sourceExe)) {
    Fail "nxlc.exe not found next to this script ($sourceDir). Re-download the full release package."
}
if (-not (Test-Path -LiteralPath $checksumFile)) {
    Fail "checksums.sha256 not found next to this script. Refusing to install an unverified binary."
}

$expected = $null
foreach ($line in Get-Content -LiteralPath $checksumFile) {
    if ($line -match '^([0-9a-fA-F]{64})\s+\*?(\S+)$') {
        if ($Matches[2] -eq $script:ExeName) { $expected = $Matches[1].ToLowerInvariant() }
    }
}
if (-not $expected) { Fail "checksums.sha256 has no entry for $($script:ExeName)." }

$actual = (Get-FileHash -LiteralPath $sourceExe -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) {
    Fail "SHA-256 mismatch for $($script:ExeName).`n  expected $expected`n  actual   $actual`nThe download is corrupt or tampered with; nothing was installed."
}
Write-Ok "SHA-256 matches ($($actual.Substring(0,16))...)"

if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'NEXORA\bin'
}

# ----------------------------------------------------------------------------
if ($Uninstall) {
    Write-Step 'Uninstalling'
    $targetExe = Join-Path $InstallDir $script:ExeName
    if (Test-Path -LiteralPath $targetExe) {
        if ($DryRun) { Write-Ok "would remove $targetExe" }
        else {
            Remove-Item -LiteralPath $targetExe -Force
            Write-Ok "removed $targetExe"
        }
    } else {
        Write-Warn "nothing installed at $targetExe"
    }
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath) {
        $kept = ($userPath -split ';' | Where-Object { $_ -and $_.TrimEnd('\') -ne $InstallDir.TrimEnd('\') })
        if ($kept.Count -ne ($userPath -split ';').Count) {
            if ($DryRun) { Write-Ok "would remove $InstallDir from the user PATH" }
            else {
                [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')
                Write-Ok 'removed install directory from the user PATH'
            }
        } else { Write-Ok 'install directory was not on the user PATH' }
    }
    Write-Host 'Uninstall complete. Open a new terminal for PATH changes to take effect.'
    exit 0
}

# ----------------------------------------------------------------------------
Write-Step "Installing to $InstallDir"
if (Test-Path -LiteralPath $InstallDir) {
    $foreign = Get-ChildItem -LiteralPath $InstallDir -File | Where-Object { $_.Name -ne $script:ExeName }
    if ($foreign) {
        Fail "$InstallDir already contains files this installer did not create ($($foreign[0].Name)). Choose -InstallDir or remove them first; nothing was changed."
    }
    Write-Ok 'existing installation found; it will be replaced in place (projects are untouched)'
}

if ($DryRun) {
    Write-Ok "would copy $($script:ExeName) to $InstallDir"
    Write-Ok "would add $InstallDir to the user PATH"
    Write-Ok 'would verify the installed binary with `nxlc.exe --version`'
    Write-Host 'Dry run complete. Nothing was written.'
    exit 0
}

New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination (Join-Path $InstallDir $script:ExeName) -Force
Write-Ok "installed $script:ExeName"

# ----------------------------------------------------------------------------
Write-Step 'Configuring the user PATH'
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = @($userPath -split ';' | Where-Object { $_ })
$already = $entries | Where-Object { $_.TrimEnd('\') -eq $InstallDir.TrimEnd('\') }
if ($already) {
    Write-Ok 'install directory already on the user PATH'
} else {
    [Environment]::SetEnvironmentVariable('Path', (($entries + $InstallDir) -join ';'), 'User')
    Write-Ok 'added the install directory to the user PATH'
}
$processPath = $env:Path
if (($processPath -split ';') -notcontains $InstallDir) {
    $env:Path = "$processPath;$InstallDir"
}

# ----------------------------------------------------------------------------
Write-Step 'Verifying the installation'
$installedExe = Join-Path $InstallDir $script:ExeName
$versionOutput = & $installedExe --version 2>&1
if ($LASTEXITCODE -ne 0) {
    Fail "The installed compiler did not run (exit $LASTEXITCODE). Installation is incomplete; please report this."
}
Write-Ok "installed compiler reports: $versionOutput"

$doctorOutput = & $installedExe doctor 2>&1
Write-Host $doctorOutput

Write-Host ''
Write-Host "NEXORA $($script:PackageVersion) installed for the current user."
Write-Host "  compiler : $installedExe"
Write-Host "  project  : nxl new my-app && cd my-app && nxl run"
Write-Host 'Open a NEW terminal (or restart your shell) so the PATH change is picked up.'
Write-Host 'Uninstall: powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 -Uninstall'
exit 0