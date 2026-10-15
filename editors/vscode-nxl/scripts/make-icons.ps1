# Regenerates the extension icons from the repository's favicon.ico.
#
# The checked-in favicon is a 769x751 image with a non-standard ICO header; it
# decodes fine, but it is not a square, correctly sized icon, so it is cropped
# to a square and rendered at the sizes VS Code and the marketplace expect.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/make-icons.ps1

[CmdletBinding()]
param(
    [string]$Source,
    [string]$OutDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$extensionDir = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
# editors/vscode-nxl -> repository root
$repoRoot = Split-Path -Parent (Split-Path -Parent $extensionDir)
if (-not $Source) { $Source = Join-Path $repoRoot 'favicon.ico' }
if (-not $OutDir) { $OutDir = Join-Path $extensionDir 'icons' }

if (-not (Test-Path -LiteralPath $Source)) { throw "source icon not found: $Source" }
Add-Type -AssemblyName System.Drawing

$icon = New-Object System.Drawing.Icon($Source)
$bitmap = $icon.ToBitmap()
if ($bitmap.Width -eq 0 -or $bitmap.Height -eq 0) {
    throw "could not decode $Source as an icon"
}

# Crop to a square so the logo is not distorted.
$edge = [Math]::Min($bitmap.Width, $bitmap.Height)
$square = New-Object System.Drawing.Bitmap -ArgumentList $edge, $edge, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$graphics = [System.Drawing.Graphics]::FromImage($square)
$graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$graphics.Clear([System.Drawing.Color]::Transparent)
$graphics.DrawImage($bitmap, (New-Object System.Drawing.Rectangle -ArgumentList 0, 0, $edge, $edge), (New-Object System.Drawing.Rectangle -ArgumentList 0, 0, $edge, $edge), [System.Drawing.GraphicsUnit]::Pixel)
$graphics.Dispose()

New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
foreach ($size in @(128, 64, 32)) {
    $bmp = New-Object System.Drawing.Bitmap -ArgumentList $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($square, 0, 0, $size, $size)
    $g.Dispose()
    $target = Join-Path $OutDir "nxl-$size.png"
    $bmp.Save($target, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host "wrote $target"
}

# The SVG used for the file icon embeds the 64px PNG so it scales cleanly
# without a build step.
$png64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes((Join-Path $OutDir 'nxl-64.png')))
$svg = @"
<?xml version="1.0" encoding="UTF-8"?>
<!--
  NXL file icon, generated from the repository's favicon.ico by
  scripts/make-icons.ps1. The logo is embedded so it scales cleanly and needs
  no build step to render.
-->
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
     width="64" height="64" viewBox="0 0 64 64" role="img" aria-label="NXL">
  <image x="0" y="0" width="64" height="64" xlink:href="data:image/png;base64,$png64"/>
</svg>
"@
$svgPath = Join-Path $OutDir 'nxl.svg'
[System.IO.File]::WriteAllText($svgPath, $svg)
Write-Host "wrote $svgPath"

$square.Dispose(); $bitmap.Dispose(); $icon.Dispose()