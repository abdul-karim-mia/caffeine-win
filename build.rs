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
        // No SDK tools: ship without a manifest. The exe still runs; Windows
        // then themes shell-drawn UI as an unversioned process.
        println!("cargo:warning=rc.exe not found; app.manifest was not embedded");
        return;
    };
    if rc_dir != PathBuf::from(".") && !rc_dir.join("rc.exe").exists() {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let manifest = std::path::Path::new("app.manifest");
    if !manifest.exists() {
        return;
    }

    // Embed as RT_MANIFEST id 1: rc.exe compiles the syntax, the linker picks
    // the .res up through the link-arg below.
    let rc_file = format!("{out_dir}\\manifest.rc");
    let res_file = format!("{out_dir}\\manifest.res");
    std::fs::write(&rc_file, "1 RT_MANIFEST app.manifest\n").expect("write manifest.rc");
    let rc_exe = rc_dir.join("rc.exe");
    let status = std::process::Command::new(&rc_exe)
        .arg("/nologo")
        .arg(format!("/fo{res_file}"))
        .arg(&rc_file)
        .current_dir(std::env::current_dir().expect("cwd"))
        .status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={res_file}"),
        _ => println!("cargo:warning=rc.exe failed; app.manifest was not embedded"),
    }
}

fn rc_candidates() -> Vec<PathBuf> {
    RC_CANDIDATES.iter().map(PathBuf::from).collect()
}
