//! `ueforge-deploy`. Framework-level build + install + package
//! CLI for any ueforge-based UE4SS Rust mod in the workspace.
//!
//! Each mod opts in by adding a `[package.metadata.ueforge]`
//! block to its `Cargo.toml`. Run from anywhere in the
//! workspace:
//!
//! ```text
//! cargo run -p ueforge-deploy --release. Install -p tweaks
//! cargo run -p ueforge-deploy --release. Install -p grounded2-mod
//! ```
//!
//! Or with the `cargo deploy` alias defined in
//! `.cargo/config.toml`:
//!
//! ```text
//! cargo deploy install -p tweaks
//! cargo deploy uninstall -p grounded2-mod
//! cargo deploy package -p tweaks            # builds dist zip
//! ```

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use regex::Regex;
use serde::Deserialize;

#[derive(Parser)]
#[command(version, about = "ueforge mod deploy CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build the mod and drop main.dll into the local game install.
    Install(CommonArgs),
    /// Remove the mod from the local game install (keeps mods.txt clean).
    Uninstall(CommonArgs),
    /// Build the mod and produce a dist zip in `dist/` for upload.
    Package(CommonArgs),
}

#[derive(Parser, Clone)]
struct CommonArgs {
    /// Workspace member to deploy.
    #[arg(short = 'p', long)]
    package: String,
    /// Override game-install autodetection.
    #[arg(long)]
    game_path: Option<PathBuf>,
    /// Skip cargo build (use already-built DLL).
    #[arg(long)]
    skip_build: bool,
    /// Accepted and ignored (the `k3sc cargo-lock` wrapper
    /// appends this; harmless here because we always run from
    /// the workspace root).
    #[arg(long, hide = true)]
    manifest_path: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Install(a) => run_install(a),
        Cmd::Uninstall(a) => run_uninstall(a),
        Cmd::Package(a) => run_package(a),
    }
}

// -------- Mod config (read from Cargo.toml metadata via cargo metadata) ----

#[derive(Debug)]
struct ModCfg {
    package_name: String,
    version: String,
    workspace_root: PathBuf,
    target_dir: PathBuf,
    built_dll: PathBuf,
    dist_dir: PathBuf,
    mod_folder_name: String,
    game_name_regex: Regex,
    game_sub_path: PathBuf,
    zip_prefix: String,
    example_settings: Option<PathBuf>,
}

#[derive(Deserialize)]
struct CargoMetadata {
    workspace_root: String,
    packages: Vec<CargoPackage>,
}

#[derive(Deserialize)]
struct CargoPackage {
    name: String,
    version: String,
    metadata: Option<serde_json::Value>,
    #[serde(default)]
    targets: Vec<CargoTarget>,
}

#[derive(Deserialize)]
struct CargoTarget {
    /// The `[lib] name`, which is what cargo writes the artifact
    /// under. Never assume it matches the deployed filename.
    name: String,
    kind: Vec<String>,
}

#[derive(Deserialize)]
struct UespyMetadata {
    mod_folder_name: String,
    game_name_regex: String,
    game_sub_path: String,
    zip_prefix: String,
    target_dir: Option<String>,
    example_settings: Option<String>,
}

fn read_mod_cfg(package: &str) -> Result<ModCfg> {
    let out = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("running `cargo metadata`")?;
    if !out.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let meta: CargoMetadata =
        serde_json::from_slice(&out.stdout).context("parsing cargo metadata JSON")?;
    let pkg = meta
        .packages
        .iter()
        .find(|p| p.name == package)
        .ok_or_else(|| anyhow!("package '{package}' not found in workspace"))?;

    let raw = pkg
        .metadata
        .as_ref()
        .and_then(|v| v.get("ueforge"))
        .ok_or_else(|| {
            anyhow!(
                "package '{package}' is missing its [package.metadata.ueforge] block.\n\n\
                 Add to its Cargo.toml:\n\n\
                 [package.metadata.ueforge]\n\
                 mod_folder_name = \"<Name>\"\n\
                 game_name_regex = \"<regex>\"\n\
                 game_sub_path   = \"<path>\"\n\
                 zip_prefix      = \"<prefix>\""
            )
        })?;
    let u: UespyMetadata =
        serde_json::from_value(raw.clone()).context("parsing [package.metadata.ueforge]")?;

    let workspace_root = PathBuf::from(meta.workspace_root);
    let target_dir = u
        .target_dir
        .map(|p| workspace_root.join(p))
        .unwrap_or_else(|| workspace_root.join("target").join(package));
    // Cargo writes the artifact under the crate's `[lib] name`,
    // which is NOT the deployed filename. UE4SS requires
    // `main.dll` in the mod's dlls folder, but every mod in this
    // workspace naming its lib `main` made them all write one
    // file in a shared target dir, overwriting each other. Cargo
    // then treated the losers as fresh and never rewrote it, so a
    // deploy could ship a different game's mod. That crashed
    // MISERY on 2026-08-26.
    let lib_name = pkg
        .targets
        .iter()
        .find(|t| t.kind.iter().any(|k| k == "cdylib"))
        .map(|t| t.name.replace('-', "_"))
        .unwrap_or_else(|| "main".to_string());
    let built_dll = target_dir
        .join("x86_64-pc-windows-msvc")
        .join("release")
        .join(format!("{lib_name}.dll"));
    let example_settings = u.example_settings.map(|p| workspace_root.join(p));

    let game_name_regex = Regex::new(&u.game_name_regex)
        .context("compiling [package.metadata.ueforge].game_name_regex")?;

    Ok(ModCfg {
        package_name: package.to_string(),
        version: pkg.version.clone(),
        workspace_root: workspace_root.clone(),
        target_dir,
        built_dll,
        dist_dir: workspace_root.join("dist"),
        mod_folder_name: u.mod_folder_name,
        game_name_regex,
        game_sub_path: PathBuf::from(u.game_sub_path.replace('/', "\\")),
        zip_prefix: u.zip_prefix,
        example_settings,
    })
}

// -------- Steam install / game discovery -----------------------------------

fn find_steam_root() -> Result<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::*;
    for (hkey, sub) in &[
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam"),
        (HKEY_CURRENT_USER, r"Software\Valve\Steam"),
    ] {
        if let Ok(key) = RegKey::predef(*hkey).open_subkey(sub)
            && let Ok(path) = key.get_value::<String, _>("InstallPath")
        {
            return Ok(PathBuf::from(path));
        }
    }
    bail!("Steam install not found in registry. Pass --game-path explicitly.")
}

fn parse_library_paths(vdf: &str) -> Vec<PathBuf> {
    let re = Regex::new(r#""path"\s*"([^"]+)""#).unwrap();
    re.captures_iter(vdf)
        .map(|c| PathBuf::from(c[1].replace("\\\\", "\\")))
        .collect()
}

fn find_game_root(cfg: &ModCfg) -> Result<PathBuf> {
    let steam = find_steam_root()?;
    let vdf_path = steam.join("steamapps").join("libraryfolders.vdf");
    if !vdf_path.is_file() {
        bail!(
            "Steam library config not found at {}",
            vdf_path.display()
        );
    }
    let vdf = fs::read_to_string(&vdf_path)
        .with_context(|| format!("reading {}", vdf_path.display()))?;
    let libraries = parse_library_paths(&vdf);
    for lib in libraries {
        let common = lib.join("steamapps").join("common");
        let Ok(entries) = fs::read_dir(&common) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            if !cfg.game_name_regex.is_match(&name) {
                continue;
            }
            let candidate = entry.path();
            if candidate.join(&cfg.game_sub_path).is_dir() {
                return Ok(candidate);
            }
        }
    }
    bail!(
        "Game install matching '{}' with subpath '{}' not found in any Steam library. \
         Pass --game-path explicitly.",
        cfg.game_name_regex.as_str(),
        cfg.game_sub_path.display()
    )
}

fn resolve_game_root(cfg: &ModCfg, override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        if !p.exists() {
            bail!("--game-path does not exist: {}", p.display());
        }
        return Ok(p.to_path_buf());
    }
    find_game_root(cfg)
}

fn ue4ss_installed(cfg: &ModCfg, game_root: &Path) -> bool {
    game_root
        .join(&cfg.game_sub_path)
        .join("ue4ss")
        .join("UE4SS.dll")
        .is_file()
}

// -------- mods.txt management ---------------------------------------------

fn update_mods_txt(cfg: &ModCfg, mods_dir: &Path) -> Result<()> {
    let mods_txt = mods_dir.join("mods.txt");
    let entry = format!("{} : 1", cfg.mod_folder_name);
    if !mods_txt.is_file() {
        println!("    creating {} with entry '{entry}'", mods_txt.display());
        fs::write(&mods_txt, &entry)?;
        return Ok(());
    }
    let content = fs::read_to_string(&mods_txt)?;
    let already = content.lines().any(|l| {
        l.trim_start()
            .starts_with(&format!("{}", cfg.mod_folder_name))
            && l.contains(':')
    });
    if already {
        println!("    mods.txt already registers {}", cfg.mod_folder_name);
        return Ok(());
    }
    println!("    appending '{entry}' to mods.txt");
    let appended = if content.ends_with('\n') {
        format!("{content}{entry}\n")
    } else {
        format!("{content}\n{entry}\n")
    };
    fs::write(&mods_txt, appended)?;
    Ok(())
}

fn strip_mods_txt_entry(cfg: &ModCfg, mods_dir: &Path) -> Result<()> {
    let mods_txt = mods_dir.join("mods.txt");
    if !mods_txt.is_file() {
        return Ok(());
    }
    let content = fs::read_to_string(&mods_txt)?;
    let prefix = format!("{}", cfg.mod_folder_name);
    let kept: Vec<&str> = content
        .lines()
        .filter(|l| !(l.trim_start().starts_with(&prefix) && l.contains(':')))
        .collect();
    if kept.len() != content.lines().count() {
        println!("    removing {} from mods.txt", cfg.mod_folder_name);
        fs::write(&mods_txt, kept.join("\n") + "\n")?;
    }
    Ok(())
}

// -------- Build ------------------------------------------------------------

fn cargo_build(cfg: &ModCfg) -> Result<()> {
    println!(
        "==> cargo build -p {} --release --target-dir {}",
        cfg.package_name,
        cfg.target_dir.display()
    );
    let status = Command::new("cargo")
        .current_dir(&cfg.workspace_root)
        .args([
            "build",
            "-p",
            &cfg.package_name,
            "--release",
            "--target-dir",
        ])
        .arg(&cfg.target_dir)
        .status()
        .context("running cargo build")?;
    if !status.success() {
        bail!("cargo build failed (exit {:?})", status.code());
    }
    if !cfg.built_dll.is_file() {
        bail!("main.dll not produced at {}", cfg.built_dll.display());
    }
    Ok(())
}

// -------- Modes ------------------------------------------------------------

fn run_install(args: CommonArgs) -> Result<()> {
    let cfg = read_mod_cfg(&args.package)?;
    if !args.skip_build {
        cargo_build(&cfg)?;
    } else if !cfg.built_dll.is_file() {
        bail!(
            "--skip-build set but no DLL at {}",
            cfg.built_dll.display()
        );
    }
    let game_root = resolve_game_root(&cfg, args.game_path.as_deref())?;
    if !ue4ss_installed(&cfg, &game_root) {
        bail!(
            "UE4SS is not installed at {}\\{}\\ue4ss\\\nInstall it first, then re-run.",
            game_root.display(),
            cfg.game_sub_path.display()
        );
    }
    let mods_dir = game_root.join(&cfg.game_sub_path).join("ue4ss").join("Mods");
    let mod_dir = mods_dir.join(&cfg.mod_folder_name);
    let dlls_dir = mod_dir.join("dlls");
    fs::create_dir_all(&dlls_dir)?;

    println!("==> deploying to {}", mod_dir.display());
    // Side-file deploy: if main.dll already exists, write to
    // main-new.dll (never locked, even if the game is running and
    // UE4SS has main.dll mapped). The mod's on_shutdown does the
    // swap on the next Ctrl+R. First-ever deploy: write main.dll
    // directly. See ueforge/docs/lifecycle.md "side-file pattern".
    let main_dst = dlls_dir.join("main.dll");
    let dst = if main_dst.is_file() {
        let new_dst = dlls_dir.join("main-new.dll");
        println!("    main.dll exists; writing main-new.dll (Ctrl+R in-game to swap)");
        new_dst
    } else {
        println!("    first deploy; writing main.dll directly");
        main_dst
    };
    fs::copy(&cfg.built_dll, &dst).map_err(|e| {
        anyhow!(
            "failed to copy {}: {e}.\nIf the game is running, ensure UE4SS is loaded with this mod (so main.dll exists already and we'd write the side-file path).",
            dst.display()
        )
    })?;
    println!("    wrote {}", dst.file_name().unwrap().to_string_lossy());

    if let Some(example) = &cfg.example_settings
        && example.is_file()
    {
        let settings_dst = dlls_dir.join("settings.json");
        if settings_dst.is_file() {
            println!("    settings.json already present, leaving user edits");
        } else {
            fs::copy(example, &settings_dst)?;
            println!("    seeded settings.json from example");
        }
    }

    update_mods_txt(&cfg, &mods_dir)?;
    println!("==> done. Launch the game to load the mod.");
    Ok(())
}

fn run_uninstall(args: CommonArgs) -> Result<()> {
    let cfg = read_mod_cfg(&args.package)?;
    let game_root = resolve_game_root(&cfg, args.game_path.as_deref())?;
    let mods_dir = game_root.join(&cfg.game_sub_path).join("ue4ss").join("Mods");
    let mod_dir = mods_dir.join(&cfg.mod_folder_name);
    if !mod_dir.is_dir() {
        println!(
            "==> no {} folder at {}; nothing to remove",
            cfg.mod_folder_name,
            mod_dir.display()
        );
        return Ok(());
    }
    println!("==> removing {}", mod_dir.display());
    fs::remove_dir_all(&mod_dir)?;
    strip_mods_txt_entry(&cfg, &mods_dir)?;
    println!("==> done");
    Ok(())
}

fn run_package(args: CommonArgs) -> Result<()> {
    let cfg = read_mod_cfg(&args.package)?;
    if !args.skip_build {
        cargo_build(&cfg)?;
    }
    fs::create_dir_all(&cfg.dist_dir)?;
    let zip_path = cfg.dist_dir.join(format!(
        "{}-v{}.zip",
        cfg.zip_prefix, cfg.version
    ));
    if zip_path.exists() {
        fs::remove_file(&zip_path)?;
    }
    println!("==> writing {}", zip_path.display());
    write_dist_zip(&cfg, &zip_path)?;
    println!(
        "==> done ({} bytes)",
        fs::metadata(&zip_path)?.len()
    );
    Ok(())
}

fn write_dist_zip(cfg: &ModCfg, zip_path: &Path) -> Result<()> {
    use zip::write::SimpleFileOptions;

    let f = fs::File::create(zip_path)?;
    let mut w = zip::ZipWriter::new(f);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // Stage layout: <game_sub_path>\ue4ss\Mods\<ModFolderName>\dlls\main.dll
    let stage_rel = cfg
        .game_sub_path
        .join("ue4ss")
        .join("Mods")
        .join(&cfg.mod_folder_name)
        .join("dlls");

    add_file(
        &mut w,
        &stage_rel.join("main.dll"),
        &cfg.built_dll,
        opts,
    )?;
    println!("    + {}/main.dll", cfg.mod_folder_name);

    if let Some(example) = &cfg.example_settings
        && example.is_file()
    {
        add_file(&mut w, &stage_rel.join("settings.json"), example, opts)?;
        println!("    + {}/settings.json", cfg.mod_folder_name);
    }

    w.finish()?;
    Ok(())
}

fn add_file(
    w: &mut zip::ZipWriter<fs::File>,
    rel: &Path,
    src: &Path,
    opts: zip::write::SimpleFileOptions,
) -> Result<()> {
    // Zip entries use forward slashes regardless of host OS.
    let name = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/");
    w.start_file(name, opts)?;
    let mut f = fs::File::open(src)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    w.write_all(&buf)?;
    Ok(())
}
