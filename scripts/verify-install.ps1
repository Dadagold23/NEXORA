<#
.SYNOPSIS
    Verifies an installed NEXORA release the way a new user experiences it.

.DESCRIPTION
    Runs the release checklist end to end against a scratch installation: the
    archive is extracted, installed to a temporary prefix, and the documented
    onboarding steps are executed with a clean PATH. Nothing touches the real
    user's installation or PATH unless -RealInstall is passed.

    Each step records how long it took, so the result can be used as evidence
    for the onboarding time target.

.PARAMETER Archive
    Path to a release archive (.zip). Defaults to the newest
    dist/NEXORA-*-windows-x64.zip.

.PARAMETER RealInstall
    Install into the default user location and offer to add it to the user
    PATH, instead of a temporary prefix. Off by default.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-install.ps1
#>
[CmdletBinding()]
param(
    [string]$Archive,
    [switch]$RealInstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'

$repoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not $Archive) {
    $candidates = @(Get-ChildItem (Join-Path $repoRoot 'dist') -Filter 'NEXORA-*-windows-x64.zip' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending)
    if (-not $candidates) { throw "no release archive found in dist/; run scripts/package-release.ps1 first" }
    $Archive = $candidates[0].FullName
}
if (-not (Test-Path -LiteralPath $Archive)) { throw "archive not found: $Archive" }

$scratch = Join-Path $env:TEMP ("nxl_verify_" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$results = New-Object System.Collections.Generic.List[object]
$overall = [System.Diagnostics.Stopwatch]::StartNew()

function Step {
    param([string]$Name, [scriptblock]$Action)
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $note = ''
    try {
        $note = & $Action
        $ok = $true
    } catch {
        $note = $_.Exception.Message
        $ok = $false
    }
    $watch.Stop()
    $script:results.Add([pscustomobject]@{ Step = $Name; Ok = $ok; Seconds = [math]::Round($watch.Elapsed.TotalSeconds, 2); Note = "$note" })
    $mark = if ($ok) { 'PASS' } else { 'FAIL' }
    Write-Host ("[{0}] {1} ({2}s) {3}" -f $mark, $Name, [math]::Round($watch.Elapsed.TotalSeconds, 2), $note)
}

Write-Host "NEXORA release verification"
Write-Host "  archive : $Archive"
Write-Host "  scratch : $scratch"
Write-Host "  host    : $([System.Environment]::OSVersion.VersionString) x64"
Write-Host ''

try {
    New-Item -ItemType Directory -Path $scratch -Force | Out-Null
    $extract = Join-Path $scratch 'extracted'

    Step 'archive extracts' {
        Expand-Archive -LiteralPath $Archive -DestinationPath $extract -Force
        "extracted to $extract"
    }

    $installer = Join-Path $extract 'install.ps1'
    $prefix = if ($RealInstall) {
        Join-Path $env:LOCALAPPDATA 'NEXORA\bin'
    } else {
        Join-Path $scratch 'install'
    }

    Step 'installer verifies the checksum' {
        $output = & powershell -NoProfile -ExecutionPolicy Bypass -File $installer -InstallDir $prefix -DryRun 2>&1
        if ($LASTEXITCODE -ne 0) { throw ($output | Out-String) }
        ($output | Select-String 'SHA-256 matches' | Out-String).Trim()
    }

    Step 'installer installs' {
        $output = & powershell -NoProfile -ExecutionPolicy Bypass -File $installer -InstallDir $prefix 2>&1
        if ($LASTEXITCODE -ne 0) { throw ($output | Out-String) }
        if (-not (Test-Path -LiteralPath (Join-Path $prefix 'nxlc.exe'))) { throw 'nxlc.exe was not installed' }
        "installed to $prefix"
    }

    # A clean PATH: only the prefix plus the Windows system directories.
    $systemPath = ($env:Path -split ';' | Where-Object { $_ -match '^(C:\\Windows|C:\\Program Files)' }) -join ';'
    $cleanPath = "$prefix;$systemPath"

    $step = {
        param([string[]]$Arguments)
        $output = & (Join-Path $prefix 'nxlc.exe') @Arguments 2>&1
        [pscustomobject]@{ Exit = $LASTEXITCODE; Output = ($output | Out-String).Trim() }
    }

    Step 'compiler launches and reports its version' {
        $saved = $env:Path
        $env:Path = $cleanPath
        try {
            $result = & $step @('--version')
            if ($result.Exit -ne 0) { throw "exit $($result.Exit): $($result.Output)" }
            if ($result.Output -notmatch '^nxlc \d') { throw "unexpected version output: $($result.Output)" }
            $result.Output
        } finally { $env:Path = $saved }
    }

    Step 'doctor reports accurate diagnostics' {
        $saved = $env:Path
        $env:Path = $cleanPath
        try {
            $result = & $step @('doctor')
            if ($result.Exit -ne 0) { throw "doctor failed: $($result.Output)" }
            if ($result.Output -notmatch 'nxlc compiler') { throw "doctor did not describe the compiler" }
            'compiler, node and project checks reported'
        } finally { $env:Path = $saved }
    }

    Step 'bundled example runs' {
        $saved = $env:Path
        $env:Path = $cleanPath
        try {
            $result = & $step @('run', (Join-Path $extract 'examples\main.nxl'))
            if ($result.Exit -ne 0) { throw "example failed: $($result.Output)" }
            "output: $($result.Output)"
        } finally { $env:Path = $saved }
    }

    Step 'invalid source fails with a non-zero exit' {
        $saved = $env:Path
        $env:Path = $cleanPath
        try {
            $bad = Join-Path $scratch 'broken.nxl'
            Set-Content -LiteralPath $bad -Value 'fn ((( broken' -Encoding ascii
            $result = & $step @('check', $bad)
            if ($result.Exit -eq 0) { throw 'invalid source was accepted' }
            "rejected with exit $($result.Exit)"
        } finally { $env:Path = $saved }
    }

    Step 'project creation works in a clean directory' {
        $saved = $env:Path
        $env:Path = $cleanPath
        $work = Join-Path $scratch 'workspace'
        New-Item -ItemType Directory -Path $work -Force | Out-Null
        try {
            Push-Location $work
            try {
                $created = & $step @('new', 'console', 'hello-app')
                if ($created.Exit -ne 0) { throw "creation failed: $($created.Output)" }
                Push-Location (Join-Path $work 'hello-app')
                try {
                    $ran = & $step @('run')
                    if ($ran.Exit -ne 0) { throw "generated project did not run: $($ran.Output)" }
                    if ($ran.Output -notmatch 'Hello, NXL!') { throw "unexpected output: $($ran.Output)" }
                    "created and ran: $($ran.Output)"
                } finally { Pop-Location }
            } finally { Pop-Location }
        } finally { $env:Path = $saved }
    }

    Step 'paths containing spaces work' {
        $saved = $env:Path
        $env:Path = $cleanPath
        $spaced = Join-Path $scratch 'dir with spaces'
        New-Item -ItemType Directory -Path $spaced -Force | Out-Null
        try {
            $file = Join-Path $spaced 'spaced program.nxl'
            Set-Content -LiteralPath $file -Value 'print("spaces work");' -Encoding ascii
            $result = & $step @('run', $file)
            if ($result.Exit -ne 0) { throw "failed: $($result.Output)" }
            $result.Output
        } finally { $env:Path = $saved }
    }

    Step 'missing dependency is diagnosed' {
        $saved = $env:Path
        $env:Path = $cleanPath
        try {
            $missing = Join-Path $scratch 'not-here.nxl'
            $result = & $step @('run', $missing)
            if ($result.Exit -eq 0) { throw 'a missing file was reported as success' }
            "diagnosed: $(($result.Output -split "`n")[0])"
        } finally { $env:Path = $saved }
    }

    Step 'reinstall over an existing installation is safe' {
        $saved = $env:Path
        $env:Path = $cleanPath
        try {
            $output = & powershell -NoProfile -ExecutionPolicy Bypass -File $installer -InstallDir $prefix 2>&1
            if ($LASTEXITCODE -ne 0) { throw ($output | Out-String) }
            'reinstalled over the existing copy'
        } finally { $env:Path = $saved }
    }

    Step 'uninstall removes the installation' {
        $saved = $env:Path
        $env:Path = $cleanPath
        try {
            if ($RealInstall) {
                "skipped: -RealInstall was requested, leaving $prefix in place"
            } else {
                $output = & powershell -NoProfile -ExecutionPolicy Bypass -File $installer -InstallDir $prefix -Uninstall 2>&1
                if ($LASTEXITCODE -ne 0) { throw ($output | Out-String) }
                if (Test-Path -LiteralPath (Join-Path $prefix 'nxlc.exe')) { throw 'nxlc.exe still present after uninstall' }
                'removed cleanly'
            }
        } finally { $env:Path = $saved }
    }
} finally {
    Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
}

$overall.Stop()
$failed = @($results | Where-Object { -not $_.Ok })
Write-Host ''
Write-Host '--- summary ---'
Write-Host "  steps    : $($results.Count)"
Write-Host "  passed   : $($results.Count - $failed.Count)"
Write-Host "  failed   : $($failed.Count)"
Write-Host "  total    : $([math]::Round($overall.Elapsed.TotalSeconds, 2))s"
foreach ($item in $failed) { Write-Host "  FAILED   : $($item.Step) - $($item.Note)" }
if ($failed.Count -gt 0) { exit 1 }
exit 0