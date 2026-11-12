#Requires -Version 5
<#
Regenerates every committed icon asset from a single source of truth: web/favicon.svg.
The exe icon, installer icon, shortcuts, README and website then all share one logo
(the same coffee-cup + countdown-ring mark as the runtime tray icon in src/icon.rs).

Run from the repository root:
    powershell -ExecutionPolicy Bypass -File tools\make-icons.ps1

Outputs:
    assets\caffeine-win.svg         reference vector (copy of web/favicon.svg)
    assets\caffeine-win.ico         16/24/32/48/64/128/256 px, PNG-compressed
    assets\caffeine-win-256.png     README / social
    web\icon-192.png                web app manifest
    web\icon-512.png                web app manifest
    web\apple-touch-icon.png        iOS home screen
#>
param(
    [int[]]$IcoSizes = @(16, 24, 32, 48, 64, 128, 256),
    [int]$ManifestSmall = 192,
    [int]$ManifestLarge = 512,
    [int]$AppleTouchSize = 180,
    [int]$ReadmeSize = 256,
    # 6x the 64-unit viewBox: oversampled raster, downscaled per size.
    [int]$Density = 384
)

$ErrorActionPreference = 'Stop'
$Repo = Split-Path -Parent $PSScriptRoot
$Assets = Join-Path $Repo 'assets'
$Web = Join-Path $Repo 'web'
$Magick = (Get-Command magick -ErrorAction Stop).Source
New-Item -ItemType Directory -Force -Path $Assets | Out-Null

# The favicon is the master artwork; assets keeps a copy beside the generated files.
$Svg = Join-Path $Web 'favicon.svg'
Copy-Item -Path $Svg -Destination (Join-Path $Assets 'caffeine-win.svg') -Force

function Invoke-Magick {
    param([string[]]$Arguments)
    & $Magick @Arguments
    if ($LASTEXITCODE -ne 0) { throw "magick $($Arguments -join ' ') exited with $LASTEXITCODE" }
}

# Oversample once, then let magick resize down to each output size.
$Common = @('-density', $Density, '-background', 'none', $Svg)

function Save-Png {
    param([int]$Size, [string]$Path)
    Invoke-Magick ($Common + @('-resize', "${Size}x${Size}", '-strip', '-define', 'png:compression-level=9', $Path))
}

function Save-Ico {
    param([string]$Path, [hashtable]$Entries)
    # PNG-compressed frames. ImageMagick's ICO writer emits 32-bit BMP frames
    # instead (~4 bytes per pixel: the 256 frame alone was 262 KB). PNG frames
    # are supported by Windows Vista and later and are what rc.exe embeds.
    $sizes = @($Entries.Keys | Sort-Object { [int]$_ })
    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter($ms)
    $bw.Write([UInt16]0)                                   # reserved
    $bw.Write([UInt16]1)                                   # type: icon
    $bw.Write([UInt16]$sizes.Count)
    $offset = 6 + 16 * $sizes.Count
    foreach ($size in $sizes) {
        $bytes = $Entries[$size]
        $dim = if ([int]$size -ge 256) { 0 } else { [int]$size }   # 256 encodes as 0
        $bw.Write([Byte]$dim)                               # width
        $bw.Write([Byte]$dim)                               # height
        $bw.Write([Byte]0)                                  # palette size
        $bw.Write([Byte]0)                                  # reserved
        $bw.Write([UInt16]1)                                # colour planes
        $bw.Write([UInt16]32)                               # bits per pixel
        $bw.Write([UInt32]$bytes.Length)                    # bytes in resource
        $bw.Write([UInt32]$offset)                          # image offset
        $offset += $bytes.Length
    }
    foreach ($size in $sizes) { $bw.Write($Entries[$size]) }
    $bw.Flush()
    [System.IO.File]::WriteAllBytes($Path, $ms.ToArray())
    $bw.Close()
    $ms.Close()
}

$ico = @{}
foreach ($size in $IcoSizes) {
    $part = Join-Path $env:TEMP ("caffeine-win-{0}.png" -f $size)
    Save-Png -Size $size -Path $part
    $ico[$size] = [System.IO.File]::ReadAllBytes($part)
    Remove-Item $part
}
Save-Ico -Path (Join-Path $Assets 'caffeine-win.ico') -Entries $ico
"assets\caffeine-win.ico: $((Get-Item (Join-Path $Assets 'caffeine-win.ico')).Length) bytes ($($IcoSizes -join '/') px)"

Save-Png -Size $ReadmeSize -Path (Join-Path $Assets ("caffeine-win-{0}.png" -f $ReadmeSize))
Save-Png -Size $ManifestSmall -Path (Join-Path $Web "icon-$ManifestSmall.png")
Save-Png -Size $ManifestLarge -Path (Join-Path $Web "icon-$ManifestLarge.png")
Save-Png -Size $AppleTouchSize -Path (Join-Path $Web 'apple-touch-icon.png')
"done: assets\caffeine-win-256.png, web\icon-192.png, web\icon-512.png, web\apple-touch-icon.png"
