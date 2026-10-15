# Regenerates every NXL brand image from the repository's favicon.ico.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/make-brand-assets.ps1
#
# Produces:
#   .github/assets/nexora-preview.png                    1280x640  GitHub social preview
#   editors/vscode-nxl/media/gallery-banner.png          1280x600  marketplace banner
#   editors/vscode-nxl/icons/nxl-32.png                    32x32   .nxl file icon
#   editors/vscode-nxl/icons/nxl-64.png                    64x64   .nxl file icon
#   editors/vscode-nxl/icons/nxl-128.png                 128x128  extension icon
#   editors/vscode-nxl/icons/nxl.svg                              scalable file icon
#   dist-brand/nxl-logo.png                               512x512  square logo
#
# One script, so the file icons and the banners can never drift apart.
#
# Two problems with the checked-in favicon.ico have to be handled here:
#
#   1. Its header is non-standard: it declares a 769x771 image whose payload is
#      larger than the file. It decodes as 769x751, i.e. the frame is truncated
#      and the rounded corners are cut off mid-curve.
#   2. Its background is opaque white, not transparency, so every derived image
#      inherits white corners (and a white halo once it is scaled down, because
#      the resampler blends white into the dark edge).
#
# So the source is repaired before it is used: the near-white region reachable
# from the image border is flood filled and turned into transparency, with a
# soft alpha ramp across the anti-aliased boundary. The interior white "/" glyph
# is not connected to the border, so it is untouched. Scaling a repaired source
# blends dark pixels with transparent ones and never with white.

[CmdletBinding()]
param(
    [string]$Source
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not $Source) { $Source = Join-Path $repoRoot 'favicon.ico' }
if (-not (Test-Path -LiteralPath $Source)) { throw "source icon not found: $Source" }

Add-Type -AssemblyName System.Drawing

# Palette sampled from the logo.
$navy = [System.Drawing.Color]::FromArgb(17, 21, 28)
$cyan = [System.Drawing.Color]::FromArgb(51, 255, 255)
$white = [System.Drawing.Color]::White
$muted = [System.Drawing.Color]::FromArgb(139, 148, 167)

# Alpha ramp applied across the flood-filled background boundary.
$backgroundSeed = 200  # luminance at or above this counts as background
$alphaStart = 245       # luminance at or above this is fully transparent
$alphaEnd = 175         # luminance at or below this stays fully opaque

# ── Decode the source ──────────────────────────────────────────────────────────
$icon = New-Object System.Drawing.Icon($Source)
$decoded = $icon.ToBitmap()
if ($decoded.Width -le 0 -or $decoded.Height -le 0) { throw "could not decode $Source as an icon" }

# Centre-crop to a square so the tile is not stretched.
$edge = [Math]::Min($decoded.Width, $decoded.Height)
$offsetX = [int](($decoded.Width - $edge) / 2)
$offsetY = [int](($decoded.Height - $edge) / 2)
$logo = New-Object System.Drawing.Bitmap -ArgumentList $edge, $edge, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$gc0 = [System.Drawing.Graphics]::FromImage($logo)
$gc0.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$gc0.Clear([System.Drawing.Color]::Transparent)
$gc0.DrawImage(
    $decoded,
    (New-Object System.Drawing.Rectangle -ArgumentList 0, 0, $edge, $edge),
    (New-Object System.Drawing.Rectangle -ArgumentList $offsetX, $offsetY, $edge, $edge),
    [System.Drawing.GraphicsUnit]::Pixel
)
$gc0.Dispose()
$decoded.Dispose()
$icon.Dispose()

# ── Repair: flood fill the border-connected near-white into transparency ───────
$rect = New-Object System.Drawing.Rectangle -ArgumentList 0, 0, $edge, $edge
$locked = $logo.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadWrite, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$stride = $locked.Stride
$pixels = New-Object byte[] ($stride * $edge)
[System.Runtime.InteropServices.Marshal]::Copy($locked.Scan0, $pixels, 0, $pixels.Length)

# GDI+ 32bppARGB is BGRA in memory and premultiplied; alpha is 255 everywhere
# here, so the channels can be read directly.
function Test-Background([int]$Index) {
    $lum = (0.299 * $pixels[$Index] + 0.587 * $pixels[$Index + 1] + 0.114 * $pixels[$Index + 2])
    return $lum -ge $backgroundSeed
}

$visited = New-Object 'System.Collections.Generic.HashSet[int]'
$queue = New-Object 'System.Collections.Generic.Queue[int]'

function Add-Seed([int]$X, [int]$Y) {
    $index = ($Y * $stride) + ($X * 4)
    if (Test-Background $index) {
        if ($visited.Add($index)) { $queue.Enqueue($index) }
    }
}

for ($x = 0; $x -lt $edge; $x++) { Add-Seed $x 0; Add-Seed $x ($edge - 1) }
for ($y = 0; $y -lt $edge; $y++) { Add-Seed 0 $y; Add-Seed ($edge - 1) $y }

while ($queue.Count -gt 0) {
    $index = $queue.Dequeue()
    $x = $index % $stride
    $y = [int]($index / $stride)
    if ($x -gt 0) { $n = $index - 4; if ((Test-Background $n) -and $visited.Add($n)) { $queue.Enqueue($n) } }
    if ($x -lt ($edge - 1)) { $n = $index + 4; if ((Test-Background $n) -and $visited.Add($n)) { $queue.Enqueue($n) } }
    if ($y -gt 0) { $n = $index - $stride; if ((Test-Background $n) -and $visited.Add($n)) { $queue.Enqueue($n) } }
    if ($y -lt ($edge - 1)) { $n = $index + $stride; if ((Test-Background $n) -and $visited.Add($n)) { $queue.Enqueue($n) } }
}

$cleared = 0
foreach ($index in $visited) {
    $lum = (0.299 * $pixels[$index] + 0.587 * $pixels[$index + 1] + 0.114 * $pixels[$index + 2])
    if ($lum -ge $alphaStart) {
        $pixels[$index + 3] = 0
        $cleared++
        continue
    }
    $ramp = ($alphaStart - $lum) / ($alphaStart - $alphaEnd)
    if ($ramp -lt 0) { $ramp = 0 }
    if ($ramp -gt 1) { $ramp = 1 }
    $pixels[$index + 3] = [byte][int]($ramp * 255)
}

[System.Runtime.InteropServices.Marshal]::Copy($pixels, 0, $locked.Scan0, $pixels.Length)
$logo.UnlockBits($locked)
Write-Host "repaired source: made $cleared border-connected background pixels transparent (of $($visited.Count) reached)"

function New-Tile {
    param([int]$Size)

    $tile = New-Object System.Drawing.Bitmap -ArgumentList $Size, $Size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($tile)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($logo, 0, 0, $Size, $Size)
    $g.Dispose()
    return $tile
}

function New-Canvas {
    param([int]$Width, [int]$Height)

    $canvas = New-Object System.Drawing.Bitmap -ArgumentList $Width, $Height, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($canvas)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::ClearTypeGridFit
    $g.Clear($navy)
    return @($canvas, $g)
}

function Get-Font {
    param([float]$Size, [System.Drawing.FontStyle]$Style)

    foreach ($family in @('Segoe UI', 'Segoe UI Semibold', 'Arial', 'Helvetica')) {
        try {
            return New-Object System.Drawing.Font -ArgumentList $family, $Size, $Style, ([System.Drawing.GraphicsUnit]::Pixel)
        } catch {
            continue
        }
    }
    return New-Object System.Drawing.Font -ArgumentList 'Arial', $Size, $Style, ([System.Drawing.GraphicsUnit]::Pixel)
}

$tile256 = New-Tile -Size 256

# ── Editor icons ───────────────────────────────────────────────────────────────
$iconDir = Join-Path $repoRoot 'editors\vscode-nxl\icons'
New-Item -ItemType Directory -Path $iconDir -Force | Out-Null
foreach ($size in @(128, 64, 32)) {
    $tile = New-Tile -Size $size
    $target = Join-Path $iconDir "nxl-$size.png"
    $tile.Save($target, [System.Drawing.Imaging.ImageFormat]::Png)
    $tile.Dispose()
    Write-Host "wrote $target"
}

# The SVG embeds the 64px tile so it scales cleanly with no build step.
$png64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes((Join-Path $iconDir 'nxl-64.png')))
$svg = @"
<?xml version="1.0" encoding="UTF-8"?>
<!--
  NXL file icon, generated from the repository's favicon.ico by
  scripts/make-brand-assets.ps1. The logo is embedded so it scales cleanly and
  needs no build step to render.
-->
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
     width="64" height="64" viewBox="0 0 64 64" role="img" aria-label="NXL">
  <image x="0" y="0" width="64" height="64" xlink:href="data:image/png;base64,$png64"/>
</svg>
"@
$svgPath = Join-Path $iconDir 'nxl.svg'
[System.IO.File]::WriteAllText($svgPath, $svg)
Write-Host "wrote $svgPath"

# ── Square logo ────────────────────────────────────────────────────────────────
$logoPath = Join-Path $repoRoot 'dist-brand\nxl-logo.png'
New-Item -ItemType Directory -Path (Split-Path -Parent $logoPath) -Force | Out-Null
$tile512 = New-Tile -Size 512
$tile512.Save($logoPath, [System.Drawing.Imaging.ImageFormat]::Png)
$tile512.Dispose()
Write-Host "wrote $logoPath"

# ── GitHub social preview (1280x640) ───────────────────────────────────────────
$previewPath = Join-Path $repoRoot '.github\assets\nexora-preview.png'
New-Item -ItemType Directory -Path (Split-Path -Parent $previewPath) -Force | Out-Null
$canvas, $g = New-Canvas -Width 1280 -Height 640
$centre = New-Object System.Drawing.StringFormat
$centre.Alignment = [System.Drawing.StringAlignment]::Center
$whiteBrush = New-Object System.Drawing.SolidBrush -ArgumentList $white
$cyanBrush = New-Object System.Drawing.SolidBrush -ArgumentList $cyan
$mutedBrush = New-Object System.Drawing.SolidBrush -ArgumentList $muted
$g.DrawImage($tile256, 520, 92, 240, 240)
$g.DrawString('NEXORA', (Get-Font -Size 60 -Style ([System.Drawing.FontStyle]::Bold)), $whiteBrush, (New-Object System.Drawing.RectangleF -ArgumentList 0, 356, 1280, 76), $centre)
$g.DrawString('N X L', (Get-Font -Size 46 -Style ([System.Drawing.FontStyle]::Bold)), $cyanBrush, (New-Object System.Drawing.RectangleF -ArgumentList 0, 436, 1280, 60), $centre)
$g.DrawString('A programming language and toolchain', (Get-Font -Size 24 -Style ([System.Drawing.FontStyle]::Regular)), $mutedBrush, (New-Object System.Drawing.RectangleF -ArgumentList 0, 516, 1280, 36), $centre)
$canvas.Save($previewPath, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $canvas.Dispose()
Write-Host "wrote $previewPath"

# ── VS Code marketplace banner (1280x600) ──────────────────────────────────────
$bannerPath = Join-Path $repoRoot 'editors\vscode-nxl\media\gallery-banner.png'
New-Item -ItemType Directory -Path (Split-Path -Parent $bannerPath) -Force | Out-Null
$canvas, $g = New-Canvas -Width 1280 -Height 600
$left = New-Object System.Drawing.StringFormat
$left.Alignment = [System.Drawing.StringAlignment]::Near
$g.DrawImage($tile256, 880, 150, 300, 300)
$g.DrawString('NEXORA', (Get-Font -Size 54 -Style ([System.Drawing.FontStyle]::Bold)), $whiteBrush, (New-Object System.Drawing.RectangleF -ArgumentList 130, 226, 700, 70), $left)
$g.DrawString('N X L', (Get-Font -Size 54 -Style ([System.Drawing.FontStyle]::Bold)), $cyanBrush, (New-Object System.Drawing.RectangleF -ArgumentList 130, 296, 700, 70), $left)
$g.DrawString('Syntax highlighting, a .nxl file icon,', (Get-Font -Size 23 -Style ([System.Drawing.FontStyle]::Regular)), $mutedBrush, (New-Object System.Drawing.RectangleF -ArgumentList 130, 392, 700, 34), $left)
$g.DrawString('and commands that run the compiler.', (Get-Font -Size 23 -Style ([System.Drawing.FontStyle]::Regular)), $mutedBrush, (New-Object System.Drawing.RectangleF -ArgumentList 130, 426, 700, 34), $left)
$canvas.Save($bannerPath, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $canvas.Dispose()
Write-Host "wrote $bannerPath"

$tile256.Dispose(); $logo.Dispose()
Write-Host "done"