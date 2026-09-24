//! Plugin package management: list / install / remove local workspace plugins.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use crate::config;
use crate::loader::PluginRegistry;

use crate::plugin_catalog::{self, CATALOG, CatalogEntry};

pub use plugin_catalog::find;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn so_path(plugins_dir: &Path, entry: &CatalogEntry) -> PathBuf {
    plugins_dir.join(entry.so_name)
}

fn is_installed(plugins_dir: &Path, entry: &CatalogEntry) -> bool {
    so_path(plugins_dir, entry).is_file()
}

fn release_mode() -> bool {
    std::env::var("TUIDER_PKG_RELEASE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn target_so(entry: &CatalogEntry) -> PathBuf {
    let profile = if release_mode() { "release" } else { "debug" };
    workspace_root()
        .join("target")
        .join(profile)
        .join(entry.so_name)
}

pub fn run(args: &[String], plugins_dir: &Path, registry: &PluginRegistry) -> Result<(), ExitCode> {
    // args[0] == "pkg"
    let sub = args.get(1).map(String::as_str).unwrap_or("list");
    match sub {
        "list" | "ls" => {
            list(plugins_dir, registry);
            Ok(())
        }
        "install" | "i" => {
            let who = args.get(2).map(String::as_str).unwrap_or("all");
            install(who, plugins_dir)
        }
        "remove" | "rm" | "uninstall" => {
            let who = args.get(2).map(String::as_str).ok_or_else(|| {
                eprintln!("tuider pkg remove: need <id|all>");
                ExitCode::from(2)
            })?;
            remove(who, plugins_dir)
        }
        "-h" | "--help" | "help" => {
            print_help();
            Ok(())
        }
        other => {
            eprintln!("tuider pkg: unknown subcommand `{other}`");
            print_help();
            Err(ExitCode::from(2))
        }
    }
}

fn print_help() {
    println!(
        "\
tuider pkg — manage local plugins (cargo build + copy .so)

USAGE:
    tuider pkg list
    tuider pkg install <id|all>
    tuider pkg remove  <id|all>

IDS: {}
DIR: set by plugins_dir / TUIDER_PLUGINS_DIR
ENV: TUIDER_PKG_RELEASE=1 → cargo --release

Requires building from the tuider source tree.",
        CATALOG.iter().map(|e| e.id).collect::<Vec<_>>().join(", ")
    );
}

fn list(plugins_dir: &Path, registry: &PluginRegistry) {
    println!("plugins_dir: {}", plugins_dir.display());
    println!();
    println!(
        "{:<8} {:<10} {:<10} {}",
        "ID", "STATUS", "ENABLED", "SUMMARY"
    );
    println!("{}", "-".repeat(56));
    for e in CATALOG {
        let installed = is_installed(plugins_dir, e);
        let loaded = registry.has(e.id);
        let status = if installed {
            if loaded {
                "installed"
            } else {
                "file-only" // so present but failed dlopen
            }
        } else {
            "missing"
        };
        let en = if config::plugin_enabled(e.id) {
            "yes"
        } else {
            "no"
        };
        println!("{:<8} {:<10} {:<10} {}", e.id, status, en, e.summary);
    }
    println!();
    println!("install: tuider pkg install <id|all>");
}

fn install(who: &str, plugins_dir: &Path) -> Result<(), ExitCode> {
    let entries: Vec<&CatalogEntry> = if who == "all" {
        CATALOG.iter().collect()
    } else {
        let e = find(who).ok_or_else(|| {
            eprintln!(
                "tuider pkg install: unknown id `{who}` (want: {})",
                CATALOG.iter().map(|e| e.id).collect::<Vec<_>>().join(", ")
            );
            ExitCode::from(2)
        })?;
        vec![e]
    };

    let root = workspace_root();
    if !root.join("Cargo.toml").is_file() {
        eprintln!("tuider pkg install: requires tuider source tree (Cargo workspace not found)");
        eprintln!("  → build from source: cd <tuider-dir> && tuider pkg install {who}");
        eprintln!("  → or copy the .so/.dll manually to the plugins directory");
        eprintln!("  → plugins dir: {}/", plugins_dir.display());
        return Err(ExitCode::from(1));
    }
    std::fs::create_dir_all(plugins_dir).map_err(|e| {
        eprintln!("tuider pkg: mkdir {}: {e}", plugins_dir.display());
        ExitCode::from(1)
    })?;

    let mut pkgs: Vec<&str> = entries.iter().map(|e| e.crate_name).collect();
    pkgs.sort();
    pkgs.dedup();

    let mut cmd = Command::new("cargo");
    cmd.arg("build").current_dir(&root);
    if release_mode() {
        cmd.arg("--release");
    }
    for p in &pkgs {
        cmd.arg("-p").arg(p);
    }
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    eprintln!("tuider pkg: {:?}", cmd);
    let st = cmd.status().map_err(|e| {
        eprintln!("tuider pkg: failed to run cargo: {e}");
        ExitCode::from(1)
    })?;
    if !st.success() {
        eprintln!("tuider pkg: cargo build failed");
        return Err(ExitCode::from(1));
    }

    for e in entries {
        let src = target_so(e);
        if !src.is_file() {
            eprintln!("tuider pkg: missing build product {}", src.display());
            return Err(ExitCode::from(1));
        }
        let dest = so_path(plugins_dir, e);
        std::fs::copy(&src, &dest).map_err(|err| {
            eprintln!(
                "tuider pkg: copy {} → {}: {err}",
                src.display(),
                dest.display()
            );
            ExitCode::from(1)
        })?;
        println!("installed {} → {}", e.id, dest.display());
    }
    Ok(())
}

fn remove(who: &str, plugins_dir: &Path) -> Result<(), ExitCode> {
    let entries: Vec<&CatalogEntry> = if who == "all" {
        CATALOG.iter().collect()
    } else {
        let e = find(who).ok_or_else(|| {
            eprintln!(
                "tuider pkg remove: unknown id `{who}` (want: {})",
                CATALOG.iter().map(|e| e.id).collect::<Vec<_>>().join(", ")
            );
            ExitCode::from(2)
        })?;
        vec![e]
    };

    let mut any = false;
    for e in entries {
        let p = so_path(plugins_dir, e);
        if p.is_file() {
            std::fs::remove_file(&p).map_err(|err| {
                eprintln!("tuider pkg: remove {}: {err}", p.display());
                ExitCode::from(1)
            })?;
            println!("removed {}", p.display());
            any = true;
        } else {
            println!("skip {} (not installed)", e.id);
        }
    }
    if !any {
        eprintln!("tuider pkg: nothing removed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_unique() {
        let mut ids: Vec<_> = CATALOG.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
    }

    #[test]
    fn find_url() {
        assert_eq!(find("url").unwrap().so_name, "libtuider_url.so");
        assert!(find("nope").is_none());
    }
}
