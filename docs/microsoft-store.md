# Task list: publish Caffeine-Win on the Microsoft Store

Status date: 2026-11-12. Owner work, checked off as done. Prefilled state reflects what already landed this session (icon embed, installer fixes, Inno Setup 7.1.1, size claims corrected to ~290 KB).

Not done, not committed: anything below with an empty box.

## Store identity (reserved 2026-11-12)

| Field | Value |
|---|---|
| Identity Name | `Sonarchalunfalnapurnewmas.CaffeineWin` |
| Publisher | `CN=70B0DB70-8EB7-417C-BD78-E45B21717ADC` |
| PublisherDisplayName | Abdul Karim Mia |
| Package Family Name | `Sonarchalunfalnapurnewmas.CaffeineWin_8m8t2472f45ny` |
| Package SID | `S-1-15-2-3565153955-1657702745-1400512398-536880357-3862558246-3579109328-2708474728` |
| Store ID | `9PLZW998J3RW` |
| Reserved title | Caffeine Win |

## 0. Decisions first (blocks the rest)

- [ ] Store title: **Caffeine Win** — see "Naming decision" below. Do not submit as "Caffeine" or "Caffeine for Windows".
- [x] Reserve "Caffeine Win" in Partner Center (existing account — AirZip is on the same account). Steps in "Reserving the name" below.
- [x] Publisher CN recorded (`CN=70B0DB70-8EB7-417C-BD78-E45B21717ADC`, account-level); identity `Name` auto-generated as `Sonarchalunfalnapurnewmas.CaffeineWin` on the first submission — recorded above.
- [ ] Decide architectures to ship: x64 required, ARM64 recommended (Windows on ARM emulates x64), x86 optional. `tools/pack-msix.ps1` handles all three; the choice is only what CI builds and submits.
- [x] Decide where the MSIX build runs: `tools/pack-msix.ps1 -Arch <x64|x86|arm64>` (cargo build + makeappx). The portable/Inno builds stay as the non-Store path.

## Naming decision (research, 2026-11-12)

Evidence, then the call:

- Store already has exact-name competitors in this category: `Caffeine : Keep Your Computer Awake` (9mtpkh51l2lr), `Caffeine 1` (9p7ffb9tf8r3, a repack of the old F15-keypress app), plus `Caffeinator`, `The Caffeine`, `Caffeine Pro`, `Windows Caffeinated`.
- "Caffeine for Windows" is the phrase the web uses for Zhorn Software's classic tray app (freeware since ~2004, CNET listing, `zhornsoftware.co.uk/caffeine`) — the very "old Caffeine" the app's FAQ contrasts against. Using it invite confusion and impersonation complaints.
- Store policy 10.1.1: "Your product title or name must be unique and must not contain marketing or descriptive text, including extraneous use of keywords." → "for Windows" is descriptive text; and a title the same as another product is rejected unless you published that product.
- `Caffeine Win` (search: `CaffeineWin` returns nothing on the Store) is free, matches the repo name `caffeine-win`, GitHub slug and website brand, and reads short on a tile. Tile `DisplayName` can still be `Caffeine`; full title `Caffeine Win`.
- Search terms in Partner Center (max 7, may not use other product titles): `keep awake`, `stay awake`, `screen`, `sleep`, `timer`, `tray`, `power`.

### Reserving the name (Partner Center, ~5 minutes)

1. Sign in at https://partner.microsoft.com/dashboard with the **same Microsoft account that owns AirZip** (one account, many products).
2. Left nav → **Apps and games** → the **New product** / **Create new app** button.
3. The "create your app by reserving a name" page asks for the **product name**. Enter exactly `Caffeine Win`.
4. Run the availability check. If it shows available, tick the ownership/rights checkbox, then **Reserve product name**. The name is locked to your account immediately.
5. The product dashboard page is created (no Store ID yet — that appears with the first submission). Write the reserved title into this doc's checkboxes.
6. Publisher CN: account-level, same as AirZip's `CN=70B0DB70-8EB7-417C-BD78-E45B21717ADC`; the identity `Name` (e.g. `Sonarchalunfalnapurnewmas.CaffeineWin`) is auto-generated on the first submission's Product identity step — copy it from there into `msix/AppxManifest.xml`.
7. If `Caffeine Win` is somehow taken, fallbacks in order: `CaffeineWin`, `Caffeine Clock` (their actual differentiator — the animated countdown tray icon), then re-scan this list before reserving anything else.
8. Timing: a reserved name is held while you prepare the first submission — MS docs state roughly three months for an unreleased reserved name. Reserve when Phase 1–2 work is underway, not months early.



## 1. App change: autostart must stop using the registry

Decision (owner, 2026-11-12): **auto-on at install** for the Store build — no consent dialog, no new dependency, zero WinRT code.

- [x] Manifest ships `<StartupTask TaskId="CaffeineAutostart" Enabled="true" />` in `msix/AppxManifest.xml`.
- [x] `src/autostart.rs`: `is_packaged()` (uses `GetCurrentPackageFullName`; APPMODEL_ERROR_NO_PACKAGE means unpackaged) — the one new branch, unit-tested (`test_is_packaged_is_false_outside_msix`).
- [x] `src/main.rs`: packaged build shows "Start with Windows (at startup)" ticked and ignores the toggle command; unpackaged build keeps the existing registry Run-key behaviour.
- [x] `cargo test` green (14 tests). MSIX repacked with the new exe: `dist\caffeine-win-x64.msix` (854,035 bytes, Identity + StartupTask verified inside the package).

Known: the InfoTip/tooltip text and the balloon say "Caffeine" — leave as is (short brand), no Store policy touch.

Inside MSIX, `HKCU` writes are virtualised per-package, so `src/autostart.rs`'s Run-key write would silently fail in the Store build. Replace with a StartupTask.

- [ ] `src/autostart.rs`: keep the registry path for the non-MSIX build, add `startup_task_enabled()`, `request_startup_task()`, `disable_startup_task()` behind WinRT calls (`Windows.ApplicationModel.StartupTask`). Both paths return a bool so `src/main.rs` stays unchanged in shape.
- [ ] `src/menu.rs` / `src/main.rs`: the "Start with Windows" item calls one dispatcher: if the process has package identity (`GetCurrentPackageFullName`), use the StartupTask; otherwise the existing registry path.
- [ ] Unit tests for the dispatcher branch (non-packaged path testable; packaged path needs an `if` guard only).
- [ ] `cargo test` green.

## 2. MSIX package scaffolding

- [ ] `msix/AppxManifest.xml`: adapt `AirZip\msix\AppxManifest.xml` — Identity `Name` from step 0, `Publisher` from step 0, `Version` from `Cargo.toml` (`MAJOR.MINOR.PATCH.0`), `ProcessorArchitecture="__ARCH__"` placeholder, `TargetDeviceFamily` min `10.0.17763.0`, `MaxVersionTested` current, `runFullTrust` capability, `EntryPoint="Windows.FullTrustApplication"`, `Executable="caffeine-win.exe"`.
- [ ] Add the startup extension to the manifest:

      <uap5:Extension Category="windows.startupTask">
        <uap5:StartupTask TaskId="CaffeineAutostart" Enabled="false" DisplayName="Caffeine for Windows" />
      </uap5:Extension>

  (`uap5` namespace + `IgnorableNamespaces` like AirZip's manifest.)
- [ ] No file-type associations: this app opens nothing.
- [ ] `msix/Assets/`: `Square150x150Logo.png`, `Square44x44Logo.png`, `StoreLogo.png`, plus `Wide310x150Logo.png` / `Square310x310Logo.png` for the tile — rasterise from `assets/caffeine-win-256.png` with the existing `tools/make-icons.ps1` pattern.
- [ ] `tools/pack-msix.ps1`: copy `target\<arch>\release\caffeine-win.exe` into `msix/`, replace `__ARCH__`, run `makeappx pack /d msix /p dist\caffeine-win-<arch>.msix /o`, print size. Windows SDK `makeappx.exe` path: `C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64`.
- [ ] Ship only the exe + manifest + assets. No installer, no unins, no LICENSE/README inside the package.

### Phase 2 note — packed twice, one bug caught

The first x86 pack shipped `ProcessorArchitecture="x64"`: the manifest is stateful and the x64 run had already consumed the `__ARCH__` placeholder. `tools/pack-msix.ps1` now restores the placeholder after each pack, and both packages' manifest was read back from the `.msix` to confirm the arch. Lesson: verify the manifest *inside the package*, not the file on disk.

### Phase 1 note — E0793 fixes

While building with rustup's 1.100 toolchain, `src\main.rs` failed with `reference to field of packed struct is unaligned` (NOTIFYICONDATAW is `#[repr(packed)]`). `set_tooltip`/`show_balloon` took `.len()` of `szTip`/`szInfoTitle`, which references a packed field. Now they use fixed capacities (`TIP_CAP=128`, `INFO_CAP=256`, `TITLE_CAP=64`, matching the windows-sys definition) with a comment saying why. This also unblocks CI: it runs latest stable, and the lint was heading for deny-by-default.

- [x] `x86_64-pc-windows-msvc` → `dist\caffeine-win-x64.msix` (854,812 bytes; arch, Identity and `Enabled="true"` startup verified inside the package)
- [x] `i686-pc-windows-msvc` → `dist\caffeine-win-x86.msix` (852,992 bytes; arch verified inside the package)
- [x] `aarch64-pc-windows-msvc` → `dist\caffeine-win-arm64.msix` (851,151 bytes). Built locally 2026-11-13 after the ARM64 build tools install (`Microsoft.VisualStudio.Component.VC.Tools.ARM64`; `Hostx64\arm64\link.exe` now present). Verified inside the package: `ProcessorArchitecture="arm64"`, identity, `Enabled="true"` startup task; exe shows `ProductName=Caffeine for Windows`, `FileVersion=0.2.4.0`.
- [x] Each exe carries the embedded icon + VERSIONINFO (x64 and x86 verified by `FileVersionInfo` / `ExtractAssociatedIcon` after the switch to the rustup toolchain).

### Toolchain gotcha found this session

`PATH` has `C:\Program Files\Rust stable MSVC 1.99\bin` ahead of `C:\Users\Abdul\.cargo\bin`, so plain `cargo`/`rustc` use a standalone 1.99 install that ships only x64 std. That is why every cross-target build failed with `can't find crate for core` even though `rustup target list --installed` showed both targets. Work used here: `$env:RUSTC = <rustup toolchain>\bin\rustc.exe` plus the rustup `cargo`, after installing the i686/aarch64 std tarballs into the rustup toolchain by hand (its `target add` registers but extracts nothing on this machine). Permanent fix for the machine: reorder PATH or retire the 1.99 install — owner's call.

### Machine fixes that need an elevated shell (this session has no admin)

Run in an **admin** PowerShell, in this order:

```powershell
# 1. Install the ARM64 build tools (link.exe Hostx64\arm64). ~1-2 GB, needs UAC.
& "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vs_installer.exe" modify `
  --installPath "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools" `
  --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --passive --norestart

# 2. Stop the standalone Rust 1.99 install from shadowing rustup (it is in the
#    MACHINE Path ahead of C:\Users\Abdul\.cargo\bin, so a per-user fix cannot win).
$m = [Environment]::GetEnvironmentVariable("Path", "Machine")
$m = ($m -split ";") | Where-Object { $_ -ne "C:\Program Files\Rust stable MSVC 1.99\bin" }
[Environment]::SetEnvironmentVariable("Path", ($m -join ";"), "Machine")
```

### Reproducing the Store packages (any time)

```powershell
# from the repository root, all three at once
powershell -ExecutionPolicy Bypass -File tools\build-msix.ps1

# or one arch at a time
powershell -ExecutionPolicy Bypass -File tools\pack-msix.ps1 -Arch x64|x86|arm64
```

`tools\build-msix.ps1` loops the three arches through `pack-msix.ps1` (cargo build per target → copy exe into `msix\` → fill `__ARCH__` and version from `Cargo.toml` → `makeappx pack` → restore the placeholder), skips an arch it cannot build here (missing ARM64 build tools, missing ruststd) with a one-line reason, and prints a summary of what landed in `dist\`.

Machine prerequisites for all three locally: Windows Kits `makeappx.exe` (checked, Windows Kits 10.0.26100.0 x64 by default in both scripts), MSVC ARM64 build tools for the arm64 package, and a rustup toolchain whose `rustc` builds the cross targets (this machine needs `$env:RUSTC` pointed at the rustup toolchain because a standalone Rust 1.99 shadows it in the machine `PATH` — see above).

### Repo fixes done this session (all verified)

- `src\icon.rs`: the four `E0133` `unsafe_fn` warnings now wrap their FFI calls in inner `unsafe {}` blocks (edition-2024 lint, was headed for deny).
- `Cargo.toml`: removed the `homepage` key Cargo 1.100 flagged as redundant with `repository`.
- `src\main.rs`: the packed-struct `E0793` errors (see Phase 1 note below) — these also unblock CI, which builds on latest stable.
- `tools\pack-msix.ps1`: restores `__ARCH__` after packing, so the pack is repeatable per arch.
- Both `.msix` rebuilt after the fixes: x64 854,810 bytes, x86 852,991 bytes, arch verified inside each package.

## 4. CI

- [x] `.github/workflows/release.yml`: matrix builds stay; added, in order — `Pack MSIX for Microsoft Store`, `Compile Inno Setup Installer` (correct script per arch: `installer.iss` / `installer.arm64.iss`), `Normalize installer name per arch` (`caffeine-win-setup-x64.exe` / `-arm64.exe`), `Upload MSIX as run artifact`, and the zip now compresses only exe + README + LICENSE (dist also holds the installer and MSIX).
- [x] GitHub release per arch: portable exe, zip, and matching `.sha256` for each, **plus the setup installer and its sha256** — portable + installer per architecture, per owner decision 2026-11-13.
- [x] The MSIX deliberately stays **out** of the release assets; it is a Store submission artifact only, uploaded as a run artifact (`caffeine-win-<arch>-msix`).
- [x] Website copy fixed: the "ARM64 installer coming soon" note is gone (now "ARM64 installer ships with the release").
- [ ] Store submission automation: none — first submission is manual via Partner Center UI.

## 5. Local verification before submitting

**Skipped by owner decision (2026-11-13).** GitHub release stays portable + installer per arch; the MSIX is built locally/CI for the Store only, and no local signed-test install is run. Package validity rests on `makeappx` having validated each manifest plus the identity/StartupTask contents verified inside every `.msix`. Known risk: a first-submission rejection if the packaged app misbehaves at runtime (tray icon, StartupTask) — that costs one certification cycle, recoverable. If you want the check later: developer mode on, sign a test copy with a self-signed cert, `Add-AppxPackage`, verify tray + Settings → Apps → Startup, then uninstall.

## 6. Store listing

- [ ] Screenshots 1366×768+ (4–5): tray Off, timer running with clock icon, right-click menu, dark mode, Settings entry. Files go in `store-listing/screenshots/` (mirror AirZip). Owner captures these — a tray icon cannot be screenshotted honestly by automation.
- [x] Description ready: `store-listing\description.md` — title, short description, full description, 7 search terms, categories, notes for certification, all paste-ready.
- [x] `web\privacy.html` (required by Store policy 10.5.1 for Desktop Bridge/Win32) + a "Privacy" link in the site nav. URL for Partner Center: `https://caffeine-win.vercel.app/privacy.html`.
- [x] Category Utilities / System utilities; free price; age rating is the IARC questionnaire at submission.
- [x] Submission notes for the reviewer: written into `store-listing\description.md` (tray utility, runFullTrust for the tray icon, StartupTask instead of the registry Run key, no network).

### Copyright and trademark (Partner Center Properties)

Paste-ready for the "Copyright and trademark" field:

```
© 2026 Abdul Karim Mia. Caffeine Win is an independent open-source utility, not affiliated with or endorsed by Microsoft Corporation. Windows is a trademark of Microsoft Corporation.
```

Matches the `LegalCopyright` string embedded in the exe. Do **not** claim a trademark for "Caffeine": the word is in common use by other Store apps and no mark is registered. If that changes, append: `Caffeine Win is a trademark of Abdul Karim Mia.`

- Upload **only the `.msix` files** — one file per architecture, all in the same submission, sharing the reserved identity already inside each manifest (`Sonarchalunfalnapurnewmas.CaffeineWin`): `dist\caffeine-win-x64.msix`, `-arm64.msix`, `-x86.msix`. The Store re-signs on ingestion; no certificate of yours is needed for the Store path.
- Upload **nothing else from the repo**: not the portable `caffeine-win.exe`, not `caffeine-win-setup.exe`, not the zips or `.sha256` files, not README/LICENSE/icons — the MSIX already carries its own icon from `msix/Assets/`.
- Listing fields (description, screenshots, privacy URL, category, price, search terms) are typed into the Partner Center form, not uploaded as files — except screenshots, which are image files.

## 7. Submit and follow up

- [ ] Upload all arch `.msix` files in **one** submission (one Identity, three packages).
- [ ] Version bump per submission, taken from `Cargo.toml`.
- [ ] On approval: add the Store badge/link to `README.md` and `web/index.html` (AirZip has the pattern).
- [ ] Keep the GitHub exe releases as the portable path; add the Store link to the download section.

## Already done (this session, keep for context)

- Exe icon + VERSIONINFO embedded via `build.rs` (x64 verified: `ProductName`, `0.2.4.0`, valid 32×32 icon).
- `assets/` icons + `tools/make-icons.ps1` regenerator; exe 512,512 → 297,472 bytes after PNG-compressed ICO frames.
- Installers: `SetupIconFile`, `IconFilename`, `DefaultGroupName`, `UninstallDisplayIcon`, version 0.2.4; `arm64compatible` → `arm64` (valid everywhere); both scripts compile under Inno Setup 7.1.1 (per-user install).
- Web: apple-touch PNG, manifest 192/512 icons; all "115 KB" copy updated to 290 KB (`dist/README.md` regenerates at release).
- Signing plan documented (SignPath org already used by AirZip; or Azure Trusted Signing).

## Known limits

- ARM64 resource embed not built locally (no ARM64 std/linker on this machine) — CI proves it; revert the unified `build.rs` path if the ARM64 job reds.
- `dist/` holds pre-icon build outputs; the release workflow refreshes them.
