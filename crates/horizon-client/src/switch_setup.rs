//! First-run lookup of the player's own prepared Horizon files and of a
//! copied `client.jar` / `.minecraft` folder, without downloading anything.
//!
//! Horizon cannot ship Minecraft's files. On the Switch the player copies them
//! onto the SD card; this module finds a prepared tree (the output of
//! `horizon-prepare` or the desktop launcher), and otherwise reports what was
//! found so the setup screen can say what is still missing.
use std::path::{Path, PathBuf};

use horizon_core::version::DATA_VERSION;
use horizon_core::VANILLA_VERSION;

/// The prepared-tree manifest the game looks for at startup.
pub const MANIFEST: &str = "horizon-resources.json";
/// Written next to the manifest by `horizon-prepare` (optional, helps confirm a tree).
pub const PREPARE_STAMP: &str = "horizon-prepare.json";
/// Exported block-tag bundle; the client fails to load without it.
pub const BLOCK_TAGS: &str = "block-tags.json";

/// A copied official-launcher (or similarly laid out) game folder.
const MINECRAFT_DIR_NAMES: &[&str] = &["minecraft", ".minecraft"];

/// What a scan of the folders beside the NRO (or the game folder) found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scan {
    /// A prepared Horizon tree the game can start with (`horizon-resources.json`).
    pub prepared: Option<PathBuf>,
    /// Why [`Self::prepared`] was rejected, if a tree was found but is unusable.
    pub prepared_error: Option<String>,
    /// The player's own client jar, if one was found.
    pub client_jar: Option<PathBuf>,
    /// A copied `.minecraft` / `minecraft` folder, if one was found.
    pub minecraft_dir: Option<PathBuf>,
    /// Short notes for the setup screen (paths that were looked at, and so on).
    pub notes: Vec<String>,
}

impl Scan {
    /// The game can leave the setup screen and start.
    pub fn is_ready(&self) -> bool {
        self.prepared.is_some() && self.prepared_error.is_none()
    }
}

/// Folders to search: the NRO / executable directory, the working directory,
/// and any extra game folder the player gave.
pub fn search_bases(exe_dir: &Path, game_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut bases = Vec::new();
    push_unique(&mut bases, exe_dir.to_path_buf());
    if let Ok(cwd) = std::env::current_dir() {
        push_unique(&mut bases, cwd);
    }
    if let Some(dir) = game_dir {
        push_unique(&mut bases, dir.to_path_buf());
    }
    bases
}

fn push_unique(bases: &mut Vec<PathBuf>, path: PathBuf) {
    if path.as_os_str().is_empty() {
        return;
    }
    if bases.iter().any(|b| b == &path) {
        return;
    }
    bases.push(path);
}

/// Look for a prepared tree, a client jar, and a copied Minecraft folder under `bases`.
pub fn scan(bases: &[PathBuf]) -> Scan {
    let mut notes = Vec::new();
    if bases.is_empty() {
        notes.push("No folder to search.".into());
        return Scan {
            prepared: None,
            prepared_error: None,
            client_jar: None,
            minecraft_dir: None,
            notes,
        };
    }

    let mut prepared = None;
    let mut prepared_error = None;
    for dir in find_manifest_dirs(bases) {
        match verify_prepared(&dir) {
            Ok(()) => {
                prepared = Some(dir);
                prepared_error = None;
                break;
            }
            Err(e) => {
                if prepared.is_none() {
                    prepared = Some(dir);
                    prepared_error = Some(e);
                }
            }
        }
    }

    let client_jar = find_client_jar(bases);
    let minecraft_dir = find_minecraft_dir(bases);

    if let Some(path) = &prepared {
        if prepared_error.is_none() {
            notes.push(format!("Found prepared game files in {}.", path.display()));
        }
    }
    if let Some(path) = &client_jar {
        notes.push(format!("Found your client jar at {}.", path.display()));
    }
    if let Some(path) = &minecraft_dir {
        notes.push(format!("Found a Minecraft folder at {}.", path.display()));
    }
    if prepared.is_none() && client_jar.is_none() && minecraft_dir.is_none() {
        notes.push(format!(
            "No prepared files, client.jar, or Minecraft folder in {}.",
            bases
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Scan {
        prepared,
        prepared_error,
        client_jar,
        minecraft_dir,
        notes,
    }
}

/// The first prepared tree that verifies, if any.
pub fn find_prepared(bases: &[PathBuf]) -> Option<PathBuf> {
    let scan = scan(bases);
    if scan.prepared_error.is_some() {
        return None;
    }
    scan.prepared
}

/// Check that `dir` looks like Horizon's prepared output for this build.
pub fn verify_prepared(dir: &Path) -> Result<(), String> {
    let manifest_path = dir.join(MANIFEST);
    if !manifest_path.is_file() {
        return Err(format!("No {MANIFEST} in {}.", dir.display()));
    }
    let text = std::fs::read_to_string(&manifest_path).map_err(|e| {
        format!("Could not read {}: {e}", manifest_path.display())
    })?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        format!("{MANIFEST} is not valid JSON: {e}")
    })?;
    match value.get("data_version").and_then(|v| v.as_i64()) {
        Some(v) if v == i64::from(DATA_VERSION) => {}
        Some(v) => {
            return Err(format!(
                "These files are for data version {v}. Horizon {VANILLA_VERSION} needs data version {DATA_VERSION} (Java Edition {VANILLA_VERSION})."
            ));
        }
        None => {
            return Err(format!(
                "{MANIFEST} has no data_version. Horizon {VANILLA_VERSION} needs data version {DATA_VERSION}."
            ));
        }
    }
    let tags = dir.join(BLOCK_TAGS);
    if !tags.is_file() {
        return Err(format!(
            "This folder has {MANIFEST} but not {BLOCK_TAGS}. Copy the full prepared folder from a computer (Horizon's launcher, or horizon-prepare --prepare), not only the manifest."
        ));
    }
    Ok(())
}

/// Short status for the first-run screen, from a completed scan.
pub fn status_text(scan: &Scan) -> String {
    if let Some(err) = &scan.prepared_error {
        return err.clone();
    }
    if let Some(path) = &scan.prepared {
        return format!(
            "Ready. Prepared game files are in {}.",
            path.display()
        );
    }
    if scan.client_jar.is_some() || scan.minecraft_dir.is_some() {
        return format!(
            "Found your own Minecraft files, but Horizon still needs a prepared folder. \
             On a computer, run Horizon's launcher or `horizon-prepare --prepare` with your \
             Java Edition {VANILLA_VERSION}, then copy the prepared folder to \
             switch/horizon/resources/ (it must contain {MANIFEST} and {BLOCK_TAGS}). \
             Horizon does not download these files. Press Scan again when they are in place."
        );
    }
    format!(
        "Copy your own files onto the SD card, then press Scan again.\n\n\
         Preferred: a prepared Horizon folder at switch/horizon/resources/ \
         (run Horizon's launcher or `horizon-prepare --prepare` on a computer with \
         Java Edition {VANILLA_VERSION}, then copy that output here).\n\n\
         Also recognized: client.jar, or a copy of your .minecraft folder, next to \
         horizon.nro. Those are your game files, but the prepared folder is still \
         required to start. Horizon never downloads Minecraft's files."
    )
}

fn find_manifest_dirs(bases: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for base in bases {
        for dir in candidate_prepared_dirs(base) {
            if dir.join(MANIFEST).is_file() {
                push_unique(&mut found, dir);
            }
        }
        walk_manifests(base, 0, 5, &mut found);
    }
    found
}

fn candidate_prepared_dirs(base: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![
        base.to_path_buf(),
        base.join("resources"),
        base.join("prepared"),
        base.join(".local").join("horizon-client").join(VANILLA_VERSION),
        base.join("horizon-client").join(VANILLA_VERSION),
        base.join("minecraft").join("resources"),
        base.join(".minecraft").join("resources"),
    ];
    for name in MINECRAFT_DIR_NAMES {
        dirs.push(base.join(name));
        dirs.push(base.join(name).join(".local").join("horizon-client").join(VANILLA_VERSION));
    }
    dirs
}

fn walk_manifests(dir: &Path, depth: u32, max_depth: u32, found: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }
    let manifest = dir.join(MANIFEST);
    if manifest.is_file() {
        push_unique(found, dir.to_path_buf());
    }
    if depth == max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skip = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| {
                matches!(
                    n,
                    "saves"
                        | "logs"
                        | "crash-reports"
                        | "screenshots"
                        | "resourcepacks"
                        | "shaderpacks"
                        | "libraries"
                        | "objects"
                        | "assets"
                        | "data"
                        | "target"
                        | ".git"
                        | "out"
                )
            });
        if skip {
            continue;
        }
        walk_manifests(&path, depth + 1, max_depth, found);
    }
}

fn find_client_jar(bases: &[PathBuf]) -> Option<PathBuf> {
    for base in bases {
        for path in candidate_jars(base) {
            if is_probably_jar(&path) {
                return Some(path);
            }
        }
    }
    None
}

fn candidate_jars(base: &Path) -> Vec<PathBuf> {
    let mut jars = vec![
        base.join("client.jar"),
        base.join(format!("{VANILLA_VERSION}.jar")),
        base.join("resources").join("client.jar"),
    ];
    for name in MINECRAFT_DIR_NAMES {
        let root = base.join(name);
        jars.push(root.join("client.jar"));
        jars.push(
            root.join("versions")
                .join(VANILLA_VERSION)
                .join(format!("{VANILLA_VERSION}.jar")),
        );
        jars.push(
            root.join("versions")
                .join(VANILLA_VERSION)
                .join("client.jar"),
        );
    }
    jars.push(
        base.join("versions")
            .join(VANILLA_VERSION)
            .join(format!("{VANILLA_VERSION}.jar")),
    );
    jars
}

fn find_minecraft_dir(bases: &[PathBuf]) -> Option<PathBuf> {
    for base in bases {
        for name in MINECRAFT_DIR_NAMES {
            let dir = base.join(name);
            if looks_like_minecraft(&dir) {
                return Some(dir);
            }
        }
        if looks_like_minecraft(base) && base != Path::new("") {
            // A copied .minecraft placed as the NRO folder itself is unusual;
            // still accept a versions/ tree sitting beside the NRO.
            if base.join("versions").is_dir() {
                return Some(base.to_path_buf());
            }
        }
    }
    None
}

fn looks_like_minecraft(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    dir.join("versions").is_dir()
        || dir.join("assets").is_dir()
        || dir.join("launcher_profiles.json").is_file()
}

/// A readable ZIP/JAR (local file header), so a 0-byte placeholder is not "found".
fn is_probably_jar(path: &Path) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    use std::io::Read;
    let mut magic = [0u8; 4];
    if file.read(&mut magic).ok() != Some(4) {
        return false;
    }
    magic == *b"PK\x03\x04" || magic == *b"PK\x05\x06" || magic == *b"PK\x07\x08"
}

/// Copy a verified prepared tree into `dest` when the two paths differ.
/// When they are the same folder, this is a no-op.
pub fn install_prepared(src: &Path, dest: &Path) -> Result<PathBuf, String> {
    verify_prepared(src)?;
    let src_abs = std::path::absolute(src).unwrap_or_else(|_| src.to_path_buf());
    let dest_abs = std::path::absolute(dest).unwrap_or_else(|_| dest.to_path_buf());
    if src_abs == dest_abs {
        return Ok(dest_abs);
    }
    // Already installed.
    if dest_abs.join(MANIFEST).is_file() && verify_prepared(&dest_abs).is_ok() {
        return Ok(dest_abs);
    }
    copy_tree(src, dest).map_err(|e| format!("Could not copy prepared files: {e}"))?;
    verify_prepared(dest)?;
    Ok(dest_abs)
}

fn copy_tree(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_tree(&from, &to)?;
        } else if ty.is_file() {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "horizon-switch-setup-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_prepared(dir: &Path, data_version: i32) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join(MANIFEST),
            format!("{{\"data_version\":{data_version}}}"),
        )
        .unwrap();
        fs::write(dir.join(BLOCK_TAGS), "{\"minecraft:blocks_motion_in_heightmap\":\"\"}").unwrap();
    }

    #[test]
    fn a_prepared_resources_folder_is_ready() {
        let root = temp("ready");
        let resources = root.join("resources");
        write_prepared(&resources, DATA_VERSION);
        let scan = scan(&[root.clone()]);
        assert_eq!(scan.prepared.as_ref(), Some(&resources));
        assert!(scan.prepared_error.is_none());
        assert!(scan.is_ready());
        verify_prepared(&resources).unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_manifest_alone_is_not_enough() {
        let root = temp("manifest-only");
        fs::write(
            root.join(MANIFEST),
            format!("{{\"data_version\":{DATA_VERSION}}}"),
        )
        .unwrap();
        let err = verify_prepared(&root).unwrap_err();
        assert!(err.contains(BLOCK_TAGS), "{err}");
        let scan = scan(&[root.clone()]);
        assert!(!scan.is_ready());
        assert!(scan.prepared_error.is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_wrong_data_version_is_rejected() {
        let root = temp("version");
        write_prepared(&root, 1);
        let err = verify_prepared(&root).unwrap_err();
        assert!(err.contains("data version 1"), "{err}");
        assert!(err.contains(&DATA_VERSION.to_string()), "{err}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_client_jar_without_prepared_files_is_not_ready() {
        let root = temp("jar-only");
        // Minimal ZIP local-file header so the detector accepts it.
        fs::write(root.join("client.jar"), b"PK\x03\x04placeholder").unwrap();
        let scan = scan(&[root.clone()]);
        assert_eq!(
            scan.client_jar.as_ref(),
            Some(&root.join("client.jar"))
        );
        assert!(scan.prepared.is_none());
        assert!(!scan.is_ready());
        let text = status_text(&scan);
        assert!(text.contains("prepared folder"), "{text}");
        assert!(text.contains("does not download") || text.contains("still needs"), "{text}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_copied_minecraft_folder_is_detected() {
        let root = temp("mcdir");
        let mc = root.join(".minecraft");
        fs::create_dir_all(mc.join("versions").join(VANILLA_VERSION)).unwrap();
        fs::create_dir_all(mc.join("assets")).unwrap();
        let scan = scan(&[root.clone()]);
        assert_eq!(scan.minecraft_dir.as_ref(), Some(&mc));
        assert!(!scan.is_ready());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn official_version_jar_under_minecraft_is_found() {
        let root = temp("version-jar");
        let jar = root
            .join("minecraft")
            .join("versions")
            .join(VANILLA_VERSION)
            .join(format!("{VANILLA_VERSION}.jar"));
        fs::create_dir_all(jar.parent().unwrap()).unwrap();
        fs::write(&jar, b"PK\x03\x04").unwrap();
        let scan = scan(&[root.clone()]);
        assert_eq!(scan.client_jar.as_ref(), Some(&jar));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_prepared_walks_a_nested_local_folder() {
        let root = temp("nested");
        let dest = root
            .join("copy")
            .join(".local")
            .join("horizon-client")
            .join(VANILLA_VERSION);
        write_prepared(&dest, DATA_VERSION);
        assert_eq!(find_prepared(&[root.clone()]).as_ref(), Some(&dest));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_copies_into_the_destination() {
        let root = temp("install");
        let src = root.join("src");
        let dest = root.join("resources");
        write_prepared(&src, DATA_VERSION);
        let installed = install_prepared(&src, &dest).unwrap();
        assert_eq!(installed, std::path::absolute(&dest).unwrap());
        verify_prepared(&dest).unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_bases_are_not_ready() {
        let scan = scan(&[]);
        assert!(!scan.is_ready());
        assert!(status_text(&scan).contains("prepared"));
    }
}
