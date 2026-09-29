use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

use crate::hash::{self, hex, sha256_bytes, tree_sha256};
use crate::manifest;
use crate::Cli;

#[derive(Debug)]
pub struct Drift {
    pub field: String,
    pub expected: String,
    pub actual: String,
    /// `true` = this can move the ImageID (guest-graph crate, its manifest, Cargo.lock).
    /// `false` = informational (some other file in the tree moved): the build is still
    /// reproducible, but this manifest no longer describes the tree byte for byte.
    pub image_id_relevant: bool,
}

fn load(manifest_path: &PathBuf) -> Result<Value, String> {
    let text =
        fs::read_to_string(manifest_path).map_err(|e| format!("cannot read manifest: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("manifest is not valid JSON: {e}"))
}

fn get_str(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for p in path {
        cur = cur.get(p).unwrap_or(&Value::Null);
    }
    cur.as_str().unwrap_or("").to_string()
}

/// Everything a verifier can check without running a build: does the tree on disk still
/// match, byte for byte and manifest for manifest, what the build was claimed to come from.
pub fn check_offline(cli: &Cli) -> Result<(Value, Vec<Drift>), String> {
    let m = load(&cli.manifest)?;
    let mut drift = Vec::new();

    let guest_manifest = get_str(&m, &["build", "guest_manifest"]);
    let crates = hash::guest_local_graph(cli.root.as_path(), &guest_manifest)?;
    let recorded = m["guest_graph"]["crate_manifests"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    for entry in &recorded {
        let rel = entry["manifest_path"].as_str().unwrap_or("");
        let Some(found) = crates.iter().find(|c| c.rel == rel) else {
            drift.push(Drift {
                field: format!("{rel}: present in manifest, absent from tree"),
                expected: "exists".into(),
                actual: "missing".into(),
                image_id_relevant: true,
            });
            continue;
        };
        let want_m = entry["manifest_sha256"].as_str().unwrap_or("");
        if want_m != hex(&found.manifest_sha) {
            drift.push(Drift {
                field: rel.to_string(),
                expected: want_m.into(),
                actual: hex(&found.manifest_sha),
                image_id_relevant: true,
            });
        }
        let crate_dir = rel.strip_suffix("/Cargo.toml").unwrap_or(rel);
        let want_s = entry["src_tree_sha256"].as_str().unwrap_or("");
        if want_s != hex(&found.src_sha) {
            drift.push(Drift {
                field: format!("{crate_dir} src tree"),
                expected: want_s.into(),
                actual: hex(&found.src_sha),
                image_id_relevant: true,
            });
            // name the individual files so the fix is not a guess
            let files = entry["src_files"].as_array().cloned().unwrap_or_default();
            for f in &files {
                let p = f["path"].as_str().unwrap_or("");
                let want = f["sha256"].as_str().unwrap_or("");
                let have = found
                    .src_files
                    .iter()
                    .find(|(rp, _)| rp == p)
                    .map(|(_, s)| hex(s))
                    .unwrap_or_else(|| "<gone>".to_string());
                if want != have {
                    drift.push(Drift {
                        field: format!("{crate_dir}/{p}"),
                        expected: want.into(),
                        actual: have,
                        image_id_relevant: true,
                    });
                }
            }
            let unlisted: Vec<String> = found
                .src_files
                .iter()
                .map(|(p, _)| p.clone())
                .filter(|p| {
                    !files.iter().any(|f| f["path"].as_str() == Some(p.as_str()))
                })
                .collect();
            for p in unlisted {
                drift.push(Drift {
                    field: format!("{crate_dir}/{p}"),
                    expected: "<not in manifest>".into(),
                    actual: "added".into(),
                    image_id_relevant: true,
                });
            }
        }
    }

    let lock = fs::read(cli.root.join("Cargo.lock")).map_err(|e| e.to_string())?;
    let want_lock = get_str(&m, &["guest_graph", "lockfile_sha256"]);
    if want_lock != hex(&sha256_bytes(&lock)) {
        drift.push(Drift {
            field: "Cargo.lock".into(),
            expected: want_lock,
            actual: hex(&sha256_bytes(&lock)),
            image_id_relevant: true,
        });
    }

    let lock_text = String::from_utf8_lossy(&lock).to_string();
    let (count, fp) = manifest::dep_set_fingerprint(&lock_text);
    let want_dep = get_str(&m, &["guest_graph", "dep_set_sha256"]);
    if want_dep != hex(&fp) {
        drift.push(Drift {
            field: "dependency set (Cargo.lock package list)".into(),
            expected: format!("{want_dep} / {} crates", m["guest_graph"]["external_crate_count"]),
            actual: format!("{} / {count} crates", hex(&fp)),
            image_id_relevant: true,
        });
    }

    // The whole tree is a *stronger* claim than the ImageID: a doc edit moves it but cannot
    // move the guest. Report it, name the files, but do not let it fail a reproducibility check.
    let tree = hash::file_hashes(&cli.root).map_err(|e| e.to_string())?;
    let want_tree = get_str(&m, &["source", "tree_sha256"]);
    if want_tree != hex(&tree_sha256(&tree)) {
        let recorded = m["source"]["files"].as_array().cloned().unwrap_or_default();
        if recorded.is_empty() {
            drift.push(Drift {
                field: "whole source tree (no per-file record in this manifest)".into(),
                expected: want_tree,
                actual: hex(&tree_sha256(&tree)),
                image_id_relevant: false,
            });
        } else {
            for (p, sha) in &tree {
                let want = recorded
                    .iter()
                    .find(|f| f["path"].as_str() == Some(p.as_str()))
                    .map(|f| f["sha256"].as_str().unwrap_or("").to_string())
                    .unwrap_or_else(|| "<not in manifest>".to_string());
                if want != hex(sha) {
                    let added = want == "<not in manifest>";
                    drift.push(Drift {
                        field: p.clone(),
                        expected: want,
                        actual: if added { "added".into() } else { hex(sha) },
                        image_id_relevant: false,
                    });
                }
            }
            for f in &recorded {
                let p = f["path"].as_str().unwrap_or("");
                if !tree.iter().any(|(tp, _)| tp == p) {
                    drift.push(Drift {
                        field: p.to_string(),
                        expected: f["sha256"].as_str().unwrap_or("").to_string(),
                        actual: "deleted".into(),
                        image_id_relevant: false,
                    });
                }
            }
        }
    }

    Ok((m, drift))
}

/// Run the recorded recipe end to end and compare the produced ImageID with the expected one.
pub fn build_and_compare(cli: &Cli, m: &Value) -> Result<Option<String>, String> {
    let script = cli
        .build_script
        .clone()
        .or_else(|| {
            let p = cli.root.join("scripts/build_guest.sh");
            p.is_file().then_some(p)
        })
        .ok_or("no build script: pass --build-script or add scripts/build_guest.sh")?;

    let ctx = cli.context_dir.clone().unwrap_or_else(|| PathBuf::from("/home/user/prov-build"));
    let out = cli
        .output_dir
        .clone()
        .unwrap_or_else(|| cli.root.join("artifacts/verify"));

    println!("running {script:?} ctx={ctx:?} out={out:?} (docker build, minutes)");
    let status = Command::new("bash")
        .arg(&script)
        .arg(&ctx)
        .arg(&out)
        .status()
        .map_err(|e| format!("cannot run build script: {e}"))?;
    if !status.success() {
        return Err(format!("build script exited with {status}"));
    }

    let log = fs::read_dir(&out)
        .map_err(|e| format!("no output dir {out:?}: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.file_name().map(|n| n == "build.log").unwrap_or(false));
    let image_id = match log {
        Some(l) => {
            let text = fs::read_to_string(&l).unwrap_or_default();
            text.lines()
                .rev()
                .find_map(|line| line.strip_prefix("IMAGEID ").map(|s| s.trim().to_string()))
        }
        None => None,
    };

    let want = get_str(m, &["expect", "image_id"]);
    match &image_id {
        Some(got) => {
            if want.is_empty() {
                println!("built ImageID  {got}\nmanifest records no expected ImageID — cannot compare");
            } else if got == &want {
                println!("ImageID MATCH: {got} == manifest expect.image_id");
            } else {
                println!("ImageID MISMATCH: built {got} != expected {want}");
            }
        }
        None => println!("build finished but no `IMAGEID <hex>` line found in build.log"),
    }
    Ok(image_id)
}
