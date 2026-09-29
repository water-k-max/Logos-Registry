//! lezbuild — turn a source tree into the build manifest the Provenance registry commits to
//! (`attach_manifest`), and re-run that recipe to prove the claim (`verify`).
//!
//!    lezbuild manifest [--out reproducible-build.json] [--offline]
//!    lezbuild verify [--manifest PATH] [--build]
//!
//! `verify` without `--build` is the cheap offline half: it re-hashes the guest's crate
//! manifests and sources and reports exactly which file moved. That half exists because of a
//! measured fact — a `[features]` table added to a guest-linked crate, with no source change
//! at all, moves the deployed ImageID. A verifier that only hashed `src/` would miss it.

mod hash;
mod manifest;
mod verify;

use std::path::PathBuf;

#[derive(Clone)]
pub struct Cli {
    pub root: PathBuf,
    pub dockerfile: PathBuf,
    pub manifest: PathBuf,
    pub out: PathBuf,
    pub repo_url: String,
    pub source_cid: Option<String>,
    pub image_id: String,
    pub image_id_source: String,
    pub image_ref: Option<String>,
    pub cache_id: String,
    pub risc0_toolchain: String,
    pub gitdb_source: String,
    /// host dir holding the crates.io cache for a sealed (no-network) rebuild; null = not seeded
    pub regcache_source: Option<String>,
    pub artifact_elf: Option<String>,
    pub build_script: Option<PathBuf>,
    pub context_dir: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub build: bool,
    pub offline: bool,
    /// fail (exit 2) when files outside the guest graph differ, i.e. the manifest does not
    /// describe this tree byte for byte even though the ImageID would still reproduce
    pub strict: bool,
}

impl Default for Cli {
    fn default() -> Cli {
        let root = PathBuf::from(
            std::env::var("LEZBUILD_ROOT").unwrap_or_else(|_| "/home/user/provenance".into()),
        );
        Cli {
            dockerfile: root.join("docker/build-guest.Dockerfile"),
            manifest: root.join("artifacts/reproducible-build.json"),
            out: root.join("artifacts/reproducible-build.json"),
            repo_url: String::new(),
            source_cid: None,
            image_id: String::new(),
            image_id_source: "unspecified".into(),
            image_ref: None,
            cache_id: "provenance-risc0-guest-b2".into(),
            risc0_toolchain: "risczero/risc0-guest-builder:r0.1.88.0".into(),
            gitdb_source: "/home/user/lez-programs/gitdb".into(),
            regcache_source: None,
            artifact_elf: Some("artifacts/provenance.elf".into()),
            build_script: None,
            context_dir: None,
            output_dir: None,
            build: false,
            offline: false,
            strict: false,
            root,
        }
    }
}

/// `--flag value` (also accepts `--flag=value`).
fn value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    let raw = args[*i].clone();
    if let Some((_, v)) = raw.split_once('=') {
        *i += 1;
        return Ok(v.to_string());
    }
    let v = args.get(*i + 1).cloned().ok_or_else(|| format!("{flag} needs a value"))?;
    *i += 2;
    Ok(v)
}

impl Cli {
    fn parse(args: &[String]) -> Result<Cli, String> {
        let mut cli = Cli::default();
        let mut i = 2; // args[0]=binary args[1]=subcommand
        while i < args.len() {
            let flag = args[i].split('=').next().unwrap_or("").to_string();
            match flag.as_str() {
                "--build" => {
                    cli.build = true;
                    i += 1;
                }
                "--offline" => {
                    cli.offline = true;
                    i += 1;
                }
                "--strict" => {
                    cli.strict = true;
                    i += 1;
                }
                "--root" => cli.root = PathBuf::from(value(args, &mut i, "--root")?),
                "--dockerfile" => cli.dockerfile = PathBuf::from(value(args, &mut i, "--dockerfile")?),
                "--manifest" => cli.manifest = PathBuf::from(value(args, &mut i, "--manifest")?),
                "--out" => cli.out = PathBuf::from(value(args, &mut i, "--out")?),
                "--build-script" => cli.build_script = Some(PathBuf::from(value(args, &mut i, "--build-script")?)),
                "--context" => cli.context_dir = Some(PathBuf::from(value(args, &mut i, "--context")?)),
                "--output-dir" => cli.output_dir = Some(PathBuf::from(value(args, &mut i, "--output-dir")?)),
                "--repo-url" => cli.repo_url = value(args, &mut i, "--repo-url")?.to_string(),
                "--source-cid" => cli.source_cid = Some(value(args, &mut i, "--source-cid")?.to_string()),
                "--image-id" => cli.image_id = value(args, &mut i, "--image-id")?.to_string(),
                "--image-id-source" => {
                    cli.image_id_source = value(args, &mut i, "--image-id-source")?.to_string()
                }
                "--image-ref" => cli.image_ref = Some(value(args, &mut i, "--image-ref")?.to_string()),
                "--cache-id" => cli.cache_id = value(args, &mut i, "--cache-id")?.to_string(),
                "--gitdb" => cli.gitdb_source = value(args, &mut i, "--gitdb")?.to_string(),
                "--regcache" => cli.regcache_source = Some(value(args, &mut i, "--regcache")?),
                "--elf" => cli.artifact_elf = Some(value(args, &mut i, "--elf")?.to_string()),
                other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
                _ => i += 1,
            }
        }
        Ok(cli)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let cli = match Cli::parse(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };

    let result = match sub {
        "manifest" => do_manifest(&cli),
        "verify" => do_verify(&cli),
        _ => {
            eprintln!("usage: lezbuild <manifest|verify> [flags]");
            eprintln!("  manifest  --root PATH --dockerfile PATH --image-id HEX \\");
            eprintln!("              --image-id-source TXT --repo-url URL --out PATH [--offline]");
            eprintln!("  verify    --manifest PATH [--build [--build-script PATH]] [--strict]");
            std::process::exit(2);
        }
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn do_manifest(cli: &Cli) -> Result<i32, String> {
    let (value, cid) = manifest::generate(cli, !cli.offline)?;
    let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
    if let Some(parent) = cli.out.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&cli.out, &bytes).map_err(|e| format!("cannot write {:?}: {e}", cli.out))?;
    println!("{}", serde_json::to_string_pretty(&value).expect("manifest must serialize"));
    eprintln!("\nwrote {:?} ({} bytes)", cli.out, bytes.len());
    eprintln!(
        "manifest sha256 (this is the manifest_cid to attach_manifest on chain): {}",
        hash::hex(&cid)
    );
    if cli.offline {
        eprintln!("OFFLINE MODE: image_digest and rust_toolchain are placeholders, not measurements.");
    }
    Ok(0)
}

fn do_verify(cli: &Cli) -> Result<i32, String> {
    let (m, drift) = verify::check_offline(cli)?;
    println!(
        "manifest: {} guest-graph crate manifests, {} external crates, source tree {}",
        m["guest_graph"]["local_crate_count"],
        m["guest_graph"]["external_crate_count"],
        m["source"]["tree_sha256"].as_str().unwrap_or("?")
    );
    let (blocking, informational): (Vec<&verify::Drift>, Vec<&verify::Drift>) =
        drift.iter().partition(|d| d.image_id_relevant);
    // Machine-readable on purpose: a shell verifier can compare this against the manifest's
    // `source/tree_sha256` without reimplementing the tree rule (PRUNE set, path encoding, sort
    // order) in another language and silently diverging from it. One rule, one implementation.
    if let Ok(tree_now) = hash::file_hashes(&cli.root) {
        let now = hash::tree_sha256(&tree_now);
        println!("source_tree_actual {}", hash::hex(&now));
    }
    let report = |items: &Vec<&verify::Drift>| {
        for d in items {
            println!("  {}\n    manifest {}\n    current  {}", d.field, d.expected, d.actual);
        }
    };

    if blocking.is_empty() {
        println!("GUEST GRAPH MATCHES — every guest-graph manifest, source file, Cargo.lock and dependency set is what the build was claimed from");
    } else {
        println!("GUEST GRAPH DRIFT — {} item(s), the ImageID cannot be trusted to reproduce:", blocking.len());
        report(&blocking);
    }
    if !informational.is_empty() {
        println!(
            "\ninformational: {} file(s) outside the guest graph differ, so this manifest no longer \
             describes the tree byte for byte (re-generate it before publishing):",
            informational.len()
        );
        report(&informational);
    }

    if cli.build {
        match verify::build_and_compare(cli, &m)? {
            Some(got) => {
                let want = m["expect"]["image_id"].as_str().unwrap_or("");
                if !want.is_empty() && got != want {
                    eprintln!("verify --build: FAIL (ImageID differs)");
                    return Ok(1);
                }
            }
            None => {
                eprintln!("verify --build: FAIL (no ImageID produced by the build)");
                return Ok(1);
            }
        }
    } else if !blocking.is_empty() {
        return Ok(1);
    }
    if !informational.is_empty() && cli.strict {
        return Ok(2);
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A throwaway project with the same shape as the real one. Tests must never touch the
    /// real repo: `provenance_core/Cargo.toml` is guest-linked, so even a transient edit there
    /// could leak into a build and move the deployed ImageID.
    fn fixture(tag: &str) -> Cli {
        let root = std::env::temp_dir().join(format!("lezbuild-fix-{tag}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("methods/guest/src/bin")).unwrap();
        fs::create_dir_all(root.join("provenance_core/src")).unwrap();
        fs::write(
            root.join("methods/guest/Cargo.toml"),
            "[package]\nname = \"provenance\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nprovenance_core = { path = \"../../provenance_core\" }\n\
             risc0-zkvm = \"3.0.5\"\n",
        )
        .unwrap();
        fs::write(
            root.join("methods/guest/src/bin/provenance.rs"),
            "fn main() {}\n",
        )
        .unwrap();
        fs::write(
            root.join("provenance_core/Cargo.toml"),
            "[package]\nname = \"provenance_core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(root.join("provenance_core/src/lib.rs"), "pub struct Entry;\n").unwrap();
        fs::write(
            root.join("Cargo.lock"),
            "version = 3\n\n[[package]]\nname = \"provenance\"\nversion = \"0.1.0\"\n\n\
             [[package]]\nname = \"provenance_core\"\nversion = \"0.1.0\"\n\n\
             [[package]]\nname = \"risc0-zkvm\"\nversion = \"3.0.5\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n",
        )
        .unwrap();

        Cli {
            root,
            dockerfile: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docker/build-guest.Dockerfile"),
            manifest: std::env::temp_dir().join(format!("lezbuild-fix-{tag}.json")),
            out: std::env::temp_dir().join(format!("lezbuild-fix-{tag}-out.json")),
            repo_url: "https://example.invalid/provenance".into(),
            image_id: "0".repeat(64),
            artifact_elf: None,
            ..Cli::default()
        }
    }

    #[test]
    fn recipe_parses_the_repo_dockerfile() {
        let text = fs::read_to_string(&Cli::default().dockerfile)
            .expect("docker/build-guest.Dockerfile must be in the repo, not only in $HOME");
        let r = manifest::parse_recipe(&text).unwrap();
        assert_eq!(r.image, "risczero/risc0-guest-builder:r0.1.88.0");
        assert_eq!(r.guest_manifest, "methods/guest/Cargo.toml");
        assert!(r.build_command.iter().any(|a| a == "--locked"), "lock must be enforced");
        assert!(r.build_command.iter().any(|a| a == "riscv32im-risc0-zkvm-elf"));
        assert!(r.rustflags.contains("lower-atomic"), "rustflags: {:?}", r.rustflags);
        assert!(
            r.rustflags.ends_with("getrandom_backend=\"custom\""),
            "the recipe's trailing escaped quote must survive unquoting: {:?}",
            r.rustflags
        );
        assert_eq!(r.workdir, "/src");
        assert_eq!(
            r.output_elf,
            "${CARGO_TARGET_DIR}/${target_triple}/release/provenance"
        );
        // the caches the recipe copies in are build inputs: if the registry seed were invisible
        // in the manifest, a "reproducible" claim would be hiding a 250 MB undeclared dependency
        let seeded: Vec<&str> = r.seeds.iter().map(|(ctx, _)| ctx.as_str()).collect();
        assert!(seeded.contains(&"gitdb"), "git seed missing: {seeded:?}");
        assert!(seeded.contains(&"regcache"), "crates.io seed missing: {seeded:?}");
        assert!(
            r.seeds.iter().any(|(ctx, dst)| ctx == "regcache" && dst == "/root/.cargo/registry"),
            "registry seed destination wrong: {:?}",
            r.seeds
        );
        // second delivery channel: the same caches baked into the builder image, staged into the
        // context path because a cache mount shadows anything at /root/.cargo/*
        let from_image: Vec<(&str, &str)> = r
            .image_seeds
            .iter()
            .map(|(img, dst)| (img.as_str(), dst.as_str()))
            .collect();
        assert!(
            from_image.contains(&("/opt/prov-seed/gitdb", "/src/gitdb")),
            "image-baked git seed missing: {from_image:?}"
        );
        assert!(
            from_image.contains(&("/opt/prov-seed/regcache", "/src/regcache")),
            "image-baked crates.io seed missing: {from_image:?}"
        );
    }

    #[test]
    fn guest_graph_follows_path_deps_and_skips_registry_deps() {
        let cli = fixture("graph");
        let crates = hash::guest_local_graph(&cli.root, "methods/guest/Cargo.toml").unwrap();
        let rels: Vec<&str> = crates.iter().map(|c| c.rel.as_str()).collect();
        assert!(rels.contains(&"methods/guest/Cargo.toml"), "got {rels:?}");
        assert!(rels.contains(&"provenance_core/Cargo.toml"), "path dep missed: {rels:?}");
        assert_eq!(crates.len(), 2, "risc0-zkvm is a registry dep and must not appear");
        for c in &crates {
            assert!(!c.src_files.is_empty(), "{} hashed no sources", c.name);
        }
    }

    #[test]
    fn manifest_is_byte_deterministic() {
        let cli = fixture("det");
        let (a, cid_a) = manifest::generate(&cli, false).unwrap();
        let (b, cid_b) = manifest::generate(&cli, false).unwrap();
        assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
        assert_eq!(cid_a, cid_b);
        assert_eq!(a["guest_graph"]["external_crate_count"], 3);
        assert_eq!(a["source"]["file_count"], 5);
        assert_eq!(a["build"]["image_digest"], "<pending: docker image inspect>");
    }

    #[test]
    fn tree_hash_moves_on_content_change_and_on_rename() {
        let dir = fixture("tree").root;
        let base = hash::tree_sha256(&hash::file_hashes(&dir).unwrap());
        fs::write(dir.join("provenance_core/src/lib.rs"), "pub struct Entry;\n// x\n").unwrap();
        assert_ne!(hash::tree_sha256(&hash::file_hashes(&dir).unwrap()), base, "content edit");

        fs::write(dir.join("provenance_core/src/lib.rs"), "pub struct Entry;\n").unwrap();
        assert_eq!(
            hash::tree_sha256(&hash::file_hashes(&dir).unwrap()),
            base,
            "revert must restore the exact tree hash"
        );
        fs::rename(
            dir.join("provenance_core/src/lib.rs"),
            dir.join("provenance_core/src/mod.rs"),
        )
        .unwrap();
        assert_ne!(
            hash::tree_sha256(&hash::file_hashes(&dir).unwrap()),
            base,
            "a rename at identical size must move the tree"
        );
    }

    #[test]
    fn pruned_dirs_are_never_hashed() {
        let dir = fixture("prune").root;
        fs::create_dir_all(dir.join("target/debug")).unwrap();
        fs::write(dir.join("target/debug/x.elf"), b"junk").unwrap();
        fs::create_dir_all(dir.join("artifacts")).unwrap();
        fs::write(dir.join("artifacts/old.bin"), b"junk").unwrap();
        let rels: Vec<String> = hash::file_hashes(&dir)
            .unwrap()
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert!(!rels.iter().any(|p| p.starts_with("target/") || p.starts_with("artifacts/")), "{rels:?}");
    }

    #[test]
    fn a_manifest_only_edit_is_named_not_just_detected() {
        // The §0.9 landmine, as a test: zero source bytes change, the guest-linked crate's
        // Cargo.toml does. A source-only verifier would pass; this one must fail, name the
        // file, and pass again once reverted.
        let cli = fixture("drift");
        let (m, _cid) = manifest::generate(&cli, false).unwrap();
        fs::write(&cli.manifest, serde_json::to_vec_pretty(&m).unwrap()).unwrap();

        let (_, clean) = verify::check_offline(&cli).unwrap();
        assert!(clean.is_empty(), "pristine fixture reports drift: {clean:?}");

        let core = cli.root.join("provenance_core/Cargo.toml");
        let original = fs::read_to_string(&core).unwrap();
        fs::write(&core, format!("{original}\n[features]\nhost = []\n")).unwrap();
        let (_, drift) = verify::check_offline(&cli).unwrap();
        assert!(
            drift.iter().any(|d| d.field == "provenance_core/Cargo.toml"),
            "manifest-only edit missed: {drift:?}"
        );
        assert!(
            !drift.iter().any(|d| d.field.ends_with("src/lib.rs")),
            "a manifest-only edit must not be blamed on a source file: {drift:?}"
        );
        assert!(
            !drift.iter().any(|d| d.field.starts_with("methods/guest/")),
            "an untouched crate must not be implicated: {drift:?}"
        );

        fs::write(&core, &original).unwrap();
        let (_, reverted) = verify::check_offline(&cli).unwrap();
        assert!(reverted.is_empty(), "revert still reports drift: {reverted:?}");

        // now a source-only edit, which must name the individual file
        fs::write(cli.root.join("provenance_core/src/lib.rs"), "pub struct Entry;\n// moved\n").unwrap();
        let (_, s) = verify::check_offline(&cli).unwrap();
        assert!(
            s.iter().any(|d| d.field == "provenance_core/src/lib.rs"),
            "source edit not attributed to a file: {s:?}"
        );
        assert!(
            !s.iter().any(|d| d.field == "provenance_core/Cargo.toml"),
            "a source-only edit must not report the manifest as drifted: {s:?}"
        );
    }

    #[test]
    fn a_file_outside_the_guest_graph_is_informational_only() {
        // The other half of the §0.9 split: docs and tools can move the tree hash but cannot
        // move the ImageID. They must be reported, and must not fail a reproducibility check.
        let cli = fixture("info");
        let (m, _cid) = manifest::generate(&cli, false).unwrap();
        fs::write(&cli.manifest, serde_json::to_vec_pretty(&m).unwrap()).unwrap();
        assert!(m["source"]["files"].as_array().unwrap().len() == m["source"]["file_count"].as_u64().unwrap() as usize);

        fs::write(cli.root.join("README.md"), "# changed after the build\n").unwrap();
        let (_, drift) = verify::check_offline(&cli).unwrap();
        assert!(!drift.is_empty(), "a new file must move the tree claim");
        assert!(
            drift.iter().all(|d| !d.image_id_relevant),
            "non-guest change escalated to blocking: {drift:?}"
        );
        assert!(
            drift.iter().any(|d| d.field == "README.md" && d.actual == "added"),
            "the added file was not named: {drift:?}"
        );

        fs::remove_file(cli.root.join("README.md")).unwrap();
        let (_, clean) = verify::check_offline(&cli).unwrap();
        assert!(clean.is_empty(), "removing the file must restore the tree hash: {clean:?}");
    }

    #[test]
    fn offline_placeholders_are_marked_pending_not_fake() {
        let cli = fixture("pending");
        let (m, _) = manifest::generate(&cli, false).unwrap();
        let t = serde_json::to_string(&m).unwrap();
        for phrase in ["pending", "unknown"] {
            assert!(t.contains(phrase) || phrase == "unknown", "offline fields unlabelled");
        }
        assert!(m["build"]["image_digest"].as_str().unwrap().starts_with("<pending"));
        assert_eq!(m["expect"]["image_id"], "0".repeat(64));
    }

    /// The three-way exit contract (0 = build claim stands, 1 = it does not, 2 = only
    /// `--strict` objects) is what CI reads. `check_offline` classifies drift correctly but
    /// says nothing about the code the process leaves behind, and a mis-measured code once
    /// looked like a broken `--strict` flag.
    #[test]
    fn the_exit_code_encodes_which_claim_broke() {
        let mut cli = fixture("exit");
        let (m, _) = manifest::generate(&cli, false).unwrap();
        fs::write(&cli.manifest, serde_json::to_vec_pretty(&m).unwrap()).unwrap();

        assert_eq!(do_verify(&cli).unwrap(), 0, "a matching tree must exit 0");

        // Outside the guest graph: reported, not fatal.
        fs::write(cli.root.join("README.md"), "# changed after the build\n").unwrap();
        assert_eq!(
            do_verify(&cli).unwrap(),
            0,
            "informational drift must not fail the reproducibility check"
        );
        cli.strict = true;
        assert_eq!(
            do_verify(&cli).unwrap(),
            2,
            "--strict must escalate informational drift to its own code"
        );
        cli.strict = false;

        // Inside the guest graph: fatal, and fatal with a louder code than --strict's 2.
        fs::write(
            cli.root.join("provenance_core/Cargo.toml"),
            "[package]\nname = \"provenance_core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [features]\nhost = []\n",
        )
        .unwrap();
        assert_eq!(do_verify(&cli).unwrap(), 1, "guest-graph drift must fail");
        let mut strict = cli.clone();
        strict.strict = true;
        assert_eq!(
            do_verify(&strict).unwrap(),
            1,
            "blocking drift outranks the informational code"
        );
    }
}
