use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{json, Map, Value};

use crate::hash::{self, hex, sha256_bytes, sha256_file, tree_sha256};
use crate::Cli;

/// Facts extracted from the build recipe itself, so the manifest cannot drift from it.
#[derive(Debug, Clone)]
pub struct Recipe {
    pub image: String,
    pub build_command: Vec<String>,
    pub workdir: String,
    pub env: Map<String, Value>,
    /// `ARG` defaults declared in the Dockerfile. `FROM` is expanded with them, so the recorded
    /// image reference is the tag the build actually pulls — not a placeholder.
    pub args: Vec<(String, String)>,
    pub rustflags: String,
    pub guest_manifest: String,
    pub output_elf: String,
    /// `(context path, in-container path)` for every `cp -a /src/<x>/. <dst>/` in the recipe:
    /// the caches the build copies in because it is not allowed to fetch them. Parsed rather
    /// than hardcoded, so a new seed cannot be added to the Dockerfile without being recorded.
    pub seeds: Vec<(String, String)>,
    /// `(image path, staged-to path)` for every `cp -a /opt/prov-seed/<x>/. <dst>/`: the same
    /// caches delivered inside the builder image instead of the build context. A third delivery
    /// channel, so it has to be as visible as the other two — and the reason it exists (cache
    /// mounts shadow image content at `/root/.cargo/...`) is recorded in the recipe's own comment.
    pub image_seeds: Vec<(String, String)>,
}

/// Strip one pair of surrounding quotes (not all of them: the guest rustflags end in `\"`,
/// and trimming greedily ate that escaped quote) and turn `\"` back into `"`.
fn unescape(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(t);
    t.replace("\\\"", "\"")
}

/// Expand `${NAME}` and `$NAME` using the recipe's own `ARG` defaults.
fn expand(s: &str, args: &[(String, String)]) -> String {
    let mut out = s.to_string();
    for (name, value) in args {
        out = out.replace(&format!("${{{name}}}"), value).replace(&format!("${name}"), value);
    }
    out
}

pub fn parse_recipe(dockerfile: &str) -> Result<Recipe, String> {
    let mut image = String::new();
    let mut workdir = "/src".to_string();
    let mut env = Map::new();
    let mut build_command: Vec<String> = Vec::new();
    let mut args: Vec<(String, String)> = Vec::new();
    let mut seeds: Vec<(String, String)> = Vec::new();
    let mut image_seeds: Vec<(String, String)> = Vec::new();
    let mut rustflags = String::new();
    let mut guest_manifest = String::new();
    let mut output_elf = String::new();

    for raw in dockerfile.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("ARG ") {
            if let Some((name, default)) = rest.split_once('=') {
                args.push((name.trim().to_string(), unescape(default)));
            }
        } else if let Some(rest) = line.strip_prefix("FROM ") {
            // first FROM is the build stage; ignore the `scratch` export stage
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if image.is_empty() && !parts.is_empty() && parts[0] != "scratch" {
                image = expand(parts[0], &args);
            }
        } else if let Some(rest) = line.strip_prefix("WORKDIR ") {
            workdir = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("ENV ") {
            let mut it = rest.splitn(2, '=');
            if let (Some(k), Some(v)) = (it.next(), it.next()) {
                env.insert(k.trim().to_string(), json!(unescape(v)));
            }
        } else if let Some((name, value)) = shell_assignment(line) {
            if name == "guest_rustflags" {
                rustflags = value.clone();
            }
            // shell `VAR="..."` lines are definitions the recipe itself uses via ${VAR};
            // recording them lets the build command be resolved instead of left ambiguous
            args.push((name, value));
        } else if let Some(rest) = line.strip_prefix("cp -a /src/") {
            // `<ctx>/. <container-dir>/` — a cache the recipe refuses to fetch over the network
            let mut it = rest.split_whitespace();
            if let (Some(src), Some(dst)) = (it.next(), it.next()) {
                seeds.push((
                    src.trim_end_matches("/.").to_string(),
                    dst.trim_end_matches('/').to_string(),
                ));
            }
        } else if let Some(rest) = line.strip_prefix("cp -a /opt/prov-seed/") {
            // `<name>/. /src/<name>/` — the same cache carried by the builder image, staged into
            // the context path because a cache mount shadows anything at /root/.cargo/*
            let mut it = rest.split_whitespace();
            if let (Some(src), Some(dst)) = (it.next(), it.next()) {
                image_seeds.push((
                    format!("/opt/prov-seed/{}", src.trim_end_matches("/.")),
                    dst.trim_end_matches('/').to_string(),
                ));
            }
        } else if line.starts_with("cargo +risc0 build") {
            build_command = line
                .split_whitespace()
                .map(|s| expand(unescape(s).as_str(), &args))
                .collect();
            if let Some(i) = build_command.iter().position(|a| a == "--manifest-path") {
                guest_manifest = build_command.get(i + 1).cloned().unwrap_or_default();
            }
        } else if line.starts_with("cp \"${CARGO_TARGET_DIR}/") {
            match line.split_whitespace().nth(1) {
                Some(token) => output_elf = token.trim_matches('"').to_string(),
                None => return Err(format!("cannot read the cp target out of {line:?}")),
            }
        }
    }

    if image.is_empty() || build_command.is_empty() || guest_manifest.is_empty() {
        return Err(format!(
            "Dockerfile parse incomplete: image={image:?} cmd={build_command:?} manifest={guest_manifest:?}"
        ));
    }
    Ok(Recipe {
        image,
        build_command,
        workdir,
        env,
        args,
        seeds,
        image_seeds,
        rustflags,
        guest_manifest,
        output_elf,
    })
}

/// `name="value"` (or `name=value`), optionally `export`-ed, as written inside the recipe's
/// shell block.
fn shell_assignment(line: &str) -> Option<(String, String)> {
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (name, value) = line.split_once('=')?;
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || !name.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false)
    {
        return None;
    }
    Some((name.to_string(), unescape(value)))
}

pub fn docker_json(args: &[&str]) -> Option<String> {
    let out = Command::new("docker").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn image_digest(recipe_image: &str, override_ref: Option<&str>) -> (String, String) {
    let reference = override_ref.unwrap_or(recipe_image);
    // `r0.1.88.0` may be a tag; ask docker for the immutable digest of what is local
    let id = docker_json(&["image", "inspect", "--format", "{{.Id}}", reference]);
    let repo_digest = docker_json(&[
        "image",
        "inspect",
        "--format",
        "{{range .RepoDigests}}{{.}} {{end}}",
        reference,
    ]);
    let digest = repo_digest
        .as_deref()
        .and_then(|s| s.split_whitespace().next())
        .map(|s| s.to_string())
        .unwrap_or_else(|| id.clone().unwrap_or_else(|| "<unresolved>".to_string()));
    (reference.to_string(), digest)
}

pub fn toolchain_version(recipe_image: &str) -> String {
    docker_json(&[
        "run",
        "--rm",
        "--entrypoint",
        "sh",
        recipe_image,
        "-c",
        "rustc +risc0 --version 2>/dev/null; rustc +risc0 -Vv 2>/dev/null | grep -i '^release'",
    ])
    .unwrap_or_else(|| "<docker unavailable>".to_string())
    .lines()
    .collect::<Vec<_>>()
    .join(" / ")
}

/// The set of external crates cargo will compile, as a fingerprint independent of file order.
pub fn dep_set_fingerprint(lock: &str) -> (usize, [u8; 32]) {
    let parsed: toml::Value = match toml::from_str(lock) {
        Ok(v) => v,
        Err(_) => return (0, [0u8; 32]),
    };
    let mut names: Vec<String> = Vec::new();
    if let Some(toml::Value::Array(packages)) = parsed.get("package") {
        for p in packages {
            let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let version = p.get("version").and_then(|v| v.as_str()).unwrap_or("");
            let source = p.get("source").and_then(|s| s.as_str()).unwrap_or("local");
            names.push(format!("{name}@{version}+{source}"));
        }
    }
    names.sort();
    let joined = names.join("\n");
    (names.len(), sha256_bytes(joined.as_bytes()))
}

pub fn git_commit(root: &Path) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(root).args(["rev-parse", "HEAD"]).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

pub fn generate(cli: &Cli, include_env: bool) -> Result<(Value, [u8; 32]), String> {
    let root = cli.root.as_path();
    let dockerfile = fs::read_to_string(&cli.dockerfile)
        .map_err(|e| format!("cannot read {}: {e}", cli.dockerfile.display()))?;
    let recipe = parse_recipe(&dockerfile)?;

    let local_crates = hash::guest_local_graph(root, &recipe.guest_manifest)?;
    let lock_path = root.join("Cargo.lock");
    let lock = fs::read_to_string(&lock_path)
        .map_err(|e| format!("cannot read {}: {e}", lock_path.display()))?;
    let (dep_count, dep_fp) = dep_set_fingerprint(&lock);
    let tree = hash::file_hashes(root).map_err(|e| e.to_string())?;
    let tree_sha = tree_sha256(&tree);

    let crate_manifests: Vec<Value> = local_crates
        .iter()
        .map(|c| {
            let files: Vec<Value> = c
                .src_files
                .iter()
                .map(|(rel, sha)| json!({ "path": rel, "sha256": hex(sha) }))
                .collect();
            json!({
                "crate": c.name,
                "manifest_path": c.rel,
                "manifest_sha256": hex(&c.manifest_sha),
                "src_tree_sha256": hex(&c.src_sha),
                "src_file_count": c.src_files.len(),
                "src_files": files,
            })
        })
        .collect();

    // every guest-graph manifest under one hash: a verifier that only hashes sources
    // would miss the §0.9 landmine entirely.
    let mut manifest_concat = String::new();
    for c in &local_crates {
        manifest_concat.push_str(&format!("{}\u{0}\u{0}{}\n", c.rel, hex(&c.manifest_sha)));
    }
    let crate_manifests_sha = sha256_bytes(manifest_concat.as_bytes());

    let (image_ref, digest, rust_toolchain) = if include_env {
        let (r, d) = image_digest(&recipe.image, cli.image_ref.as_deref());
        (r, d, toolchain_version(&recipe.image))
    } else {
        // offline mode: record the recipe's own image reference, mark the machine facts pending
        // so nobody mistakes a placeholder for a measurement
        (
            cli.image_ref.clone().unwrap_or(recipe.image.clone()),
            "<pending: docker image inspect>".to_string(),
            "<pending: docker run rustc +risc0 --version>".to_string(),
        )
    };

    let elf_path = cli
        .artifact_elf
        .as_ref()
        .map(|rel| if std::path::Path::new(rel).is_absolute() { std::path::PathBuf::from(rel) } else { root.join(rel) });
    let (elf_sha, elf_size) = match elf_path.as_ref().filter(|p| p.is_file()) {
        Some(p) => (
            Some(hex(&sha256_file(p).map_err(|e| e.to_string())?)),
            Some(fs::metadata(p).map_err(|e| e.to_string())?.len()),
        ),
        _ => (None, None),
    };

    let value = json!({
        "schema": 1,
        "generated_by": format!("lezbuild {}", env!("CARGO_PKG_VERSION")),
        "source": {
            "repo_url": cli.repo_url,
            "commit": git_commit(root),
            "tree_sha256": hex(&tree_sha),
            "file_count": tree.len(),
            // every pruned-free file, not only the guest graph: a verifier can then say *which*
            // file moved when the difference is a README and not a rebuild hazard
            "files": tree
                .iter()
                .map(|(rel, sha)| json!({ "path": rel, "sha256": hex(sha) }))
                .collect::<Vec<_>>(),
            "bundle_cid": cli.source_cid,
        },
        "build": {
            "image": image_ref,
            "image_digest": digest,
            "command": recipe.build_command,
            "workdir": recipe.workdir,
            "env": recipe.env,
            // verbatim from the recipe: the separator is `$(printf '\037')`, so this is the
            // *recipe text*, not a literal CARGO_ENCODED_RUSTFLAGS value
            "guest_rustflags_recipe": recipe.rustflags,
            "recipe_variables": recipe
                .args
                .iter()
                .map(|(k, v)| json!({ "name": k, "default": v }))
                .collect::<Vec<_>>(),
            "guest_manifest": recipe.guest_manifest,
            "container_cache_id": cli.cache_id,
            "rust_toolchain": rust_toolchain,
            "risc0_toolchain": cli.risc0_toolchain,
            "offline_seed": {
                // parsed from the recipe, not asserted: each entry is a cache the build copies
                // in from the build context because it must not fetch it
                "caches_copied_from_context": recipe
                    .seeds
                    .iter()
                    .map(|(ctx, dst)| json!({ "context_path": ctx, "in_container": dst }))
                    .collect::<Vec<_>>(),
                // The same caches when the builder image carries them instead of the context, so a
                // verifier needs only the pinned image. Recorded separately on purpose: they reach
                // /root/.cargo/* only via /src, because a cache mount shadows image content there.
                "caches_baked_in_builder_image": recipe
                    .image_seeds
                    .iter()
                    .map(|(img, dst)| json!({ "in_builder_image": img, "staged_to": dst }))
                    .collect::<Vec<_>>(),
                "host_sources": {
                    "gitdb": cli.gitdb_source,
                    "regcache": cli.regcache_source,
                },
            },
        },
        "guest_graph": {
            "local_crate_count": local_crates.len(),
            "crate_manifests": crate_manifests,
            "crate_manifests_sha256": hex(&crate_manifests_sha),
            "lockfile_sha256": hex(&sha256_bytes(lock.as_bytes())),
            "external_crate_count": dep_count,
            "dep_set_sha256": hex(&dep_fp),
        },
        "expect": {
            "guest_artifact": recipe.output_elf,
            "elf_sha256": elf_sha,
            "elf_size_bytes": elf_size,
            "image_id": cli.image_id,
            "image_id_source": cli.image_id_source,
        },
    });

    let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
    Ok((value, sha256_bytes(&bytes)))
}
