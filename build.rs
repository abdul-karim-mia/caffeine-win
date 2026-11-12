use std::path::PathBuf;

/// Windows Kit binaries, newest kit first.
const RC_CANDIDATES: &[&str] = &[
    r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64",
    r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64",
    r"C:\Program Files\Windows Kits\10\bin\10.0.26100.0\x64",
    r"C:\Program Files\Windows Kits\10\bin\10.0.19041.0\x64",
];

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=assets/caffeine-win.ico");
    println!("cargo:rerun-if-changed=build.rs");

    let mut rc = rc_candidates()
        .into_iter()
        .find(|dir| dir.join("rc.exe").try_exists().unwrap_or(false));
    if rc.is_none() {
        // Fall back to PATH.
        let status = std::process::Command::new("rc.exe")
            .arg("/?")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            rc = Some(PathBuf::from("."));
        }
    }

    let Some(rc_dir) = rc else {
        // No SDK tools: ship without the manifest, icon or version info. The exe
        // still runs; Windows then themes shell-drawn UI as an unversioned process.
        println!("cargo:warning=rc.exe not found; manifest, icon and version info were not embedded");
        return;
    };
    if rc_dir != PathBuf::from(".") && !rc_dir.join("rc.exe").exists() {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let manifest = std::path::Path::new("app.manifest");
    let icon = std::path::Path::new("assets/caffeine-win.ico");
    if !manifest.exists() {
        return;
    }

    // One .rc: RT_MANIFEST id 1 (as before), RT_ICON id 1 for the exe icon, and
    // VERSIONINFO so Explorer shows name/version instead of blank properties.
    // Resource ids are per-type, so icon and manifest can both use id 1.
    let version = version_quad();
    let rc_text = format!(
        "1 RT_MANIFEST \"app.manifest\"\n\
         1 ICON \"assets/caffeine-win.ico\"\n\
         1 VERSIONINFO\n\
         FILEVERSION {major},{minor},{patch},0\n\
         PRODUCTVERSION {major},{minor},{patch},0\n\
         FILEOS 0x4\n\
         FILETYPE 0x1\n\
         FILESUBTYPE 0x0\n\
         BEGIN\n\
         \x20 BLOCK \"StringFileInfo\"\n\
         \x20 BEGIN\n\
         \x20 \x20 BLOCK \"040904b0\"\n\
         \x20 \x20 BEGIN\n\
         \x20 \x20 \x20 VALUE \"CompanyName\", \"Abdul Karim Mia\"\n\
         \x20 \x20 \x20 VALUE \"FileDescription\", \"Keep your PC and screen awake from the system tray\"\n\
         \x20 \x20 \x20 VALUE \"FileVersion\", \"{major}.{minor}.{patch}.0\"\n\
         \x20 \x20 \x20 VALUE \"InternalName\", \"caffeine-win\"\n\
         \x20 \x20 \x20 VALUE \"LegalCopyright\", \"MIT License. Copyright (c) Abdul Karim Mia.\"\n\
         \x20 \x20 \x20 VALUE \"OriginalFilename\", \"caffeine-win.exe\"\n\
         \x20 \x20 \x20 VALUE \"ProductName\", \"Caffeine for Windows\"\n\
         \x20 \x20 END\n\
         \x20 END\n\
         \x20 BLOCK \"VarFileInfo\"\n\
         \x20 BEGIN\n\
         \x20 \x20 VALUE \"Translation\", 0x0409, 0x04b0\n\
         \x20 END\n\
         END\n",
        major = version.0,
        minor = version.1,
        patch = version.2,
    );
    let rc_file = format!("{out_dir}\\manifest.rc");
    let res_file = format!("{out_dir}\\manifest.res");
    if !icon.exists() {
        // No icon asset: still embed the manifest and version info, the exe just
        // keeps the default Windows icon. Regenerate with tools/make-icons.ps1.
        let rc_without_icon = rc_text
            .lines()
            .filter(|line| !line.contains(" ICON "))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&rc_file, rc_without_icon).expect("write manifest.rc");
    } else {
        std::fs::write(&rc_file, rc_text).expect("write manifest.rc");
    }
    let rc_exe = rc_dir.join("rc.exe");
    let status = std::process::Command::new(&rc_exe)
        .arg("/nologo")
        .arg(format!("/fo{res_file}"))
        .arg(&rc_file)
        .current_dir(std::env::current_dir().expect("cwd"))
        .status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={res_file}"),
        _ => println!("cargo:warning=rc.exe failed; manifest, icon and version info were not embedded"),
    }
}

/// (major, minor, patch) from Cargo's package metadata, so the resource version
/// never drifts from Cargo.toml.
fn version_quad() -> (u64, u64, u64) {
    let parse = |name: &str, fallback: u64| -> u64 {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(fallback)
    };
    (
        parse("CARGO_PKG_VERSION_MAJOR", 0),
        parse("CARGO_PKG_VERSION_MINOR", 1),
        parse("CARGO_PKG_VERSION_PATCH", 0),
    )
}

fn rc_candidates() -> Vec<PathBuf> {
    RC_CANDIDATES.iter().map(PathBuf::from).collect()
}
