use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_bytes(input: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(input);
    h.finalize().into()
}

pub fn sha256_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().into())
}

/// Directory names that are build output, VCS state, or vendored caches — never source.
/// `gitdb`/`regcache` are the sealed-build seeds the recipe copies into the container; they are
/// multi-hundred-MB caches, so they must never be part of the source tree hash.
pub const PRUNE: &[&str] = &[
    "target",
    ".git",
    "artifacts",
    "gitdb",
    "regcache",
    ".cargo",
    "node_modules",
];

pub fn pruned(name: &str) -> bool {
    PRUNE.contains(&name)
}

/// Deterministic (relative-path, sha256) list over a subtree. Paths are forward-slash
/// relative to `root` so the same tree hashes identically on any machine.
pub fn file_hashes(root: &Path) -> std::io::Result<Vec<(String, [u8; 32])>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if pruned(&name) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, sha256_file(&path)?));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Hash over sorted (path, content-hash) pairs: changes if any file changes OR any
/// file is added/removed/renamed.
pub fn tree_sha256(entries: &[(String, [u8; 32])]) -> [u8; 32] {
    let mut h = Sha256::new();
    for (rel, sha) in entries {
        h.update(rel.as_bytes());
        h.update([0u8]);
        h.update(sha);
    }
    h.finalize().into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCrate {
    /// the crate's Cargo.toml, relative to the workspace root, e.g. "provenance_core/Cargo.toml"
    pub rel: String,
    pub name: String,
    pub manifest_sha: [u8; 32],
    pub src_sha: [u8; 32],
    /// source files relative to the *crate* dir, e.g. "src/lib.rs"
    pub src_files: Vec<(String, [u8; 32])>,
}

/// Walk the *guest's* dependency graph and return every local (path-dependency) crate
/// it transitively pulls in. This is the set whose manifests can move the ImageID.
pub fn guest_local_graph(root: &Path, guest_manifest_rel: &str) -> Result<Vec<LocalCrate>, String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(guest_manifest_rel.to_string());
    let mut crates = Vec::new();

    while let Some(rel_manifest) = queue.pop_front() {
        if !seen.insert(rel_manifest.clone()) {
            continue;
        }
        let manifest_path = root.join(&rel_manifest);
        let text = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("cannot read {}: {e}", manifest_path.display()))?;
        let parsed: toml::Value = toml::from_str(&text)
            .map_err(|e| format!("cannot parse {}: {e}", manifest_path.display()))?;
        let name = parsed
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .ok_or_else(|| format!("{} has no package.name", manifest_path.display()))?
            .to_string();

        // path dependencies, in any of the dependency tables cargo resolves for the guest
        let crate_dir = manifest_path.parent().unwrap_or(root);
        for dep_rel in path_deps(&parsed) {
            let dep_manifest = normalize(crate_dir.join(dep_rel).join("Cargo.toml"), root);
            queue.push_back(dep_manifest);
        }

        let src_root = crate_dir.join("src");
        let src_files = if src_root.is_dir() {
            // keep the "src/" prefix so a drift report can name the file unambiguously
            file_hashes(&src_root)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|(p, sha)| (format!("src/{p}"), sha))
                .collect()
        } else {
            Vec::new()
        };
        crates.push(LocalCrate {
            rel: normalize(manifest_path, root),
            name,
            manifest_sha: sha256_bytes(text.as_bytes()),
            src_sha: tree_sha256(&src_files),
            src_files,
        });
    }

    crates.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(crates)
}

fn path_deps(parsed: &toml::Value) -> Vec<String> {
    let mut out = Vec::new();
    for table in ["dependencies", "dev-dependencies", "build-dependencies", "target"] {
        if let Some(deps) = parsed.get(table) {
            collect_path_deps(deps, &mut out);
        }
    }
    out
}

fn collect_path_deps(value: &toml::Value, out: &mut Vec<String>) {
    match value {
        toml::Value::Table(map) => {
            for (_, v) in map {
                match v {
                    toml::Value::String(_) => {}
                    toml::Value::Table(t) => {
                        if let Some(p) = t.get("path").and_then(|x| x.as_str()) {
                            out.push(p.to_string());
                        }
                        // features/optional sub-tables are not dependency maps; only recurse
                        // where cargo puts dependency specs.
                        for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
                            if let Some(inner) = t.get(key) {
                                collect_path_deps(inner, out);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        toml::Value::Array(items) => {
            for item in items {
                collect_path_deps(item, out);
            }
        }
        _ => {}
    }
}

fn normalize(path: PathBuf, root: &Path) -> String {
    let rel = path::clean_relative(&path, root);
    rel.replace('\\', "/")
}

mod path {
    use std::path::{Component, Path, PathBuf};

    /// Lexical normalisation (resolves `..`) so `../provenance_core/Cargo.toml` becomes a
    /// root-relative path. Never touches the filesystem.
    pub fn clean_relative(path: &Path, root: &Path) -> String {
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        let mut rebuilt = PathBuf::new();
        for c in abs.components() {
            match c {
                Component::ParentDir => {
                    rebuilt.pop();
                }
                Component::CurDir => {}
                other => rebuilt.push(other.as_os_str()),
            }
        }
        rebuilt
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| rebuilt.to_string_lossy().to_string())
    }
}
