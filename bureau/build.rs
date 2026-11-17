//! Intègre l'icône, le manifeste et les informations de version dans l'exécutable
//! Windows. Le fichier de ressources est généré ici pour y inscrire la version de
//! Cargo.toml (les macros chaînes passent mal d'un compilateur de ressources à l'autre).

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=ressources");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut parts = version.split(['.', '-']).map(|p| p.parse::<u16>().unwrap_or(0));
    let (major, minor, patch) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("ressources");
    // Barres obliques : acceptées par rc.exe comme par windres, sans échappement.
    let path = |f: &str| dir.join(f).to_string_lossy().replace('\\', "/");

    let rc = format!(
        r#"#include <winver.h>

1 ICON "{icon}"
1 24 "{manifest}"

VS_VERSION_INFO VERSIONINFO
 FILEVERSION     {major},{minor},{patch},0
 PRODUCTVERSION  {major},{minor},{patch},0
 FILEFLAGSMASK   0x3fL
 FILEFLAGS       0x0L
 FILEOS          VOS_NT_WINDOWS32
 FILETYPE        VFT_APP
 FILESUBTYPE     VFT2_UNKNOWN
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040C04B0"
    BEGIN
      VALUE "CompanyName",      "Gestion CSV"
      VALUE "FileDescription",  "Gestion CSV - manipulation de fichiers CSV"
      VALUE "FileVersion",      "{version}"
      VALUE "InternalName",     "gestion-csv-bureau"
      VALUE "LegalCopyright",   "Gestion CSV"
      VALUE "OriginalFilename", "GestionCSV.exe"
      VALUE "ProductName",      "Gestion CSV"
      VALUE "ProductVersion",   "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x040C, 1200
  END
END
"#,
        icon = path("gestion-csv.ico"),
        manifest = path("gestion-csv.manifest"),
    );
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("gestion-csv.rc");
    std::fs::write(&out, rc).expect("écriture du fichier de ressources");
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_required()
        .expect("compilation des ressources Windows");
}
