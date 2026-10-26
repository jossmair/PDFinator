//! Embed the same original identity in Explorer, shortcuts and the Windows window.
use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=assets/pdfinator.ico");
    println!("cargo:rerun-if-env-changed=PDFINATOR_GENERATE_ICONS");
    if env::var("PDFINATOR_GENERATE_ICONS").as_deref() == Ok("1") {
        return;
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if let Err(error) = resource() {
        eprintln!("Icône Windows : {error}");
        std::process::exit(1);
    }
}
fn resource() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("Dossier du projet absent")?);
    if !root.join("assets/pdfinator.ico").is_file() {
        return Err("Générez assets/pdfinator.ico avant de compiler l’application.".into());
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("Dossier de compilation absent")?);
    let icon = root
        .join("assets/pdfinator.ico")
        .to_string_lossy()
        .replace('\\', "/");
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.3.1".into());
    let numeric = format!("{},0", version.replace('.', ","));
    let script = format!(
        r#"#pragma code_page(65001)
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 1
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040c04b0"
  BEGIN
   VALUE "CompanyName", "Ergonatus\0"
   VALUE "FileDescription", "PDFinator\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "InternalName", "pdfinator\0"
   VALUE "OriginalFilename", "pdfinator.exe\0"
   VALUE "ProductName", "PDFinator — Ergonatus\0"
   VALUE "ProductVersion", "{version}\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x040c, 1200
 END
END
"#
    );
    fs::write(out.join("pdfinator.rc"), script)?;
    let mut candidates = Vec::new();
    if let Some(bin) = env::var_os("WindowsSdkVerBinPath") {
        candidates.push(PathBuf::from(bin).join("x64/rc.exe"));
    }
    if let Some(programs) = env::var_os("ProgramFiles(x86)") {
        let bin = PathBuf::from(programs).join("Windows Kits/10/bin");
        if let Ok(directories) = fs::read_dir(bin) {
            let mut versions: Vec<_> = directories
                .flatten()
                .map(|d| d.path().join("x64/rc.exe"))
                .filter(|p| p.is_file())
                .collect();
            versions.sort();
            versions.reverse();
            candidates.extend(versions);
        }
    }
    candidates.push("rc.exe".into());
    let output = out.join("pdfinator.res");
    for compiler in candidates {
        match Command::new(compiler)
            .arg("/nologo")
            .arg("/fo")
            .arg(&output)
            .arg(out.join("pdfinator.rc"))
            .status()
        {
            Ok(status) if status.success() => {
                println!("cargo:rustc-link-arg-bin=pdfinator={}", output.display());
                return Ok(());
            }
            Ok(_) => return Err("La compilation de la ressource Windows a échoué.".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err("rc.exe est absent : installez le Windows SDK des Build Tools C++.".into())
}
