#Requires -Version 5
<#
Packs one architecture of the Caffeine Win MSIX package for the Microsoft Store.

Usage, from the repository root:
    powershell -ExecutionPolicy Bypass -File tools\pack-msix.ps1 -Arch x64
    powershell -ExecutionPolicy Bypass -File tools\pack-msix.ps1 -Arch x86
    powershell -ExecutionPolicy Bypass -File tools\pack-msix.ps1 -Arch arm64

Builds the exe for the matching Rust target first, copies it into msix\, fills
the manifest placeholders (architecture, version from Cargo.toml), then packs
with the Windows SDK's makeappx.exe into dist\.

Local install check (the Store re-signs on ingestion; sideloading needs a cert):
    signtool sign /fd SHA256 /a /f test.pfx /p pass dist\caffeine-win-x64.msix
    Add-AppxPackage dist\caffeine-win-x64.msix
#>
param(
    [ValidateSet('x64', 'x86', 'arm64')]
    [string]$Arch = 'x64'
)

$ErrorActionPreference = 'Stop'
$Repo = Split-Path -Parent $PSScriptRoot
$MakeAppx = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\makeappx.exe'
if (-not (Test-Path $MakeAppx)) {
    throw "makeappx.exe not found at $MakeAppx; adjust the Windows Kits version path"
}

switch ($Arch) {
    'x64'   { $Target = 'x86_64-pc-windows-msvc' }
    'x86'   { $Target = 'i686-pc-windows-msvc' }
    'arm64' { $Target = 'aarch64-pc-windows-msvc' }
}

# Version from Cargo.toml: MSIX wants MAJOR.MINOR.PATCH.0.
$cargo = Get-Content (Join-Path $Repo 'Cargo.toml') -Raw
if ($cargo -notmatch '(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"') {
    throw 'could not read version from Cargo.toml'
}
$version = "{0}.{1}.{2}.0" -f $Matches[1], $Matches[2], $Matches[3]

Push-Location $Repo
try {
    cargo build --release --target $Target
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $Target" }

    Copy-Item "target\$Target\release\caffeine-win.exe" "$Repo\msix\caffeine-win.exe" -Force

    $manifest = [System.IO.File]::ReadAllText("$Repo\msix\AppxManifest.xml", [System.Text.Encoding]::UTF8)
    $manifest = $manifest.Replace('__ARCH__', $Arch)
    # regex.Replace with a script block: PowerShell's -replace replacement string
    # parses $10 as a capture group. Also anchored to its own indented line so the
    # xml declaration's lowercase version="1.0" is never touched.
    $manifest = [regex]::Replace($manifest, '(?m)^(\s*Version=")[\d.]+(")',
        { param($m) $m.Groups[1].Value + $version + $m.Groups[2].Value })
    [System.IO.File]::WriteAllText("$Repo\msix\AppxManifest.xml", $manifest, (New-Object System.Text.UTF8Encoding($false)))

    New-Item -ItemType Directory -Force -Path "$Repo\dist" | Out-Null
    $out = "$Repo\dist\caffeine-win-$Arch.msix"
    & $MakeAppx pack /d "$Repo\msix" /p $out /o
    if ($LASTEXITCODE -ne 0) { throw "makeappx pack failed for $Arch" }

    # Restore the placeholders so the script is repeatable for the next arch.
    $manifest = [System.IO.File]::ReadAllText("$Repo\msix\AppxManifest.xml", [System.Text.Encoding]::UTF8)
    $manifest = $manifest.Replace("ProcessorArchitecture=""$Arch""", 'ProcessorArchitecture="__ARCH__"')
    [System.IO.File]::WriteAllText("$Repo\msix\AppxManifest.xml", $manifest, (New-Object System.Text.UTF8Encoding($false)))

    "packed $out ($((Get-Item $out).Length) bytes), version $version"
}
finally {
    Pop-Location
}
