#Requires -Version 5
<#
Builds every MSIX package for the Microsoft Store: x64, x86 and ARM64.

Run from the repository root:
    powershell -ExecutionPolicy Bypass -File tools\build-msix.ps1

Each arch delegates to tools\pack-msix.ps1 (cargo build + makeappx pack +
placeholder restore). An arch that cannot build here - missing ARM64 linker,
missing ruststd, missing Windows Kits - is reported and skipped, so x64 always
still succeeds. Output lands in dist\ as caffeine-win-<arch>.msix.
#>
param(
    # Which architectures to build. Default: everything this machine can produce.
    [string[]]$Arch = @('x64', 'x86', 'arm64')
)

$ErrorActionPreference = 'Stop'
$Repo = Split-Path -Parent $PSScriptRoot

$makeAppx = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\makeappx.exe'
if (-not (Test-Path $makeAppx)) {
    throw "Windows Kits makeappx.exe not found at $makeAppx; adjust the version path for your machine"
}

$targets = @{
    'x64'   = 'x86_64-pc-windows-msvc'
    'x86'   = 'i686-pc-windows-msvc'
    'arm64' = 'aarch64-pc-windows-msvc'
}

$ok = @()
$failed = @()
foreach ($arch in $Arch) {
    $target = $targets[$arch]
    $linkerNote = ''
    if ($arch -eq 'arm64') {
        # link.exe from the MSVC ARM64 build tools; rustc probes the VS install,
        # but the files must exist or the final link fails.
        $armLink = Get-ChildItem "C:\Program Files (x86)\Microsoft Visual Studio\*\*\VC\Tools\MSVC\*\bin\Hostx64\arm64\link.exe" -ErrorAction SilentlyContinue
        if (-not $armLink) {
            $failed += "$arch : skipped, ARM64 build tools missing (install Microsoft.VisualStudio.Component.VC.Tools.ARM64)"
            continue
        }
    }
    try {
        & powershell -ExecutionPolicy Bypass -File "$Repo\tools\pack-msix.ps1" -Arch $arch | ForEach-Object { $_ }
        $pkg = "$Repo\dist\caffeine-win-$arch.msix"
        if (Test-Path $pkg) { $ok += $arch } else { $failed += "$arch : pack reported success but $pkg is missing" }
    }
    catch {
        $failed += "$arch : $($_.Exception.Message)"
    }
}

""
"--- MSIX build summary ---"
foreach ($arch in $ok) {
    $pkg = Join-Path $Repo "dist\caffeine-win-$arch.msix"
    "OK      $pkg ($((Get-Item $pkg).Length) bytes)"
}
foreach ($line in $failed) { "SKIPPED $line" }
if ($failed.Count -gt 0) { "Note: any arch above can be produced by CI - the release workflow packs and uploads it as a run artifact." }
