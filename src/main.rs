//! Tuider — slim terminal reader; plugins from .so directory.

#[cfg(feature = "ai")]
mod ai;
mod app;
mod cache;
mod config;
mod loader;
mod html_css;
mod html_render;
mod md;
mod plugin;
mod scan;
mod source;
mod theme;
mod ui;
mod pkg;
mod plugin_catalog;

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use loader::{default_plugins_dir, PluginRegistry};
use plugin::{ai_compiled, ContentSource};
use source::FileTreeSource;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_TIME: &str = env!("TUIDER_BUILD_TIME");

fn main() -> ExitCode {
    let loaded = config::load();
    app::set_file_config(loaded.as_ref().map(|(_, c)| c.clone()));

    let plugins_dir = loaded
        .as_ref()
        .and_then(|(_, c)| c.plugins_dir.clone())
        .map(PathBuf::from)
        .unwrap_or_else(default_plugins_dir);

    let registry = PluginRegistry::load(&plugins_dir);

    match run(&registry, &plugins_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn run(registry: &PluginRegistry, plugins_dir: &std::path::Path) -> Result<(), ExitCode> {
    let mut recursive = false;
    let mut list_mode = false;
    let mut paths: Vec<PathBuf> = Vec::new();
    let raw: Vec<String> = std::env::args().skip(1).collect();

    if raw.first().map(String::as_str) == Some("pkg") {
        return pkg::run(&raw, plugins_dir, registry);
    }

    if raw.iter().any(|a| a == "-h" || a == "--help") {
        print_help(registry, plugins_dir);
        return Ok(());
    }
    if raw.iter().any(|a| a == "-V" || a == "--version") {
        println!("tuider {VERSION} (built {BUILD_TIME})");
        return Ok(());
    }

    let mut i = 0;
    while i < raw.len() {
        let a = &raw[i];
        match a.as_str() {
            "-r" | "--recursive" => recursive = true,
            "-l" | "--list" | "--print" => list_mode = true,
            s if s.starts_with('-') => {
                // unknown core flag → maybe plugin claims it later
                if matches!(s, "-u" | "--url" | "-hn" | "--hn" | "-g" | "--code" | "-n" | "-m") {
                    // handled by plugin open below
                } else {
                    eprintln!("tuider: unknown flag `{s}`");
                    return Err(ExitCode::from(2));
                }
            }
            s => paths.push(PathBuf::from(s)),
        }
        i += 1;
    }

    // Try each loaded plugin that claims these args
    for plug in &registry.plugins {
        if !config::plugin_enabled(&plug.id) {
            continue;
        }
        if plug.handles_args(&raw) || plugin_catalog::claims(plug.id.as_str(), &raw) {
            match plug.open_from_args(&raw) {
                Ok(src) => {
                    if list_mode || !io::stdout().is_terminal() {
                        for e in src.entries() {
                            println!("{e}");
                        }
                        return Ok(());
                    }
                    return run_tui(Box::new(src));
                }
                Err(e) => {
                    eprintln!("tuider: plugin `{}`: {e}", plug.id);
                    return Err(ExitCode::from(1));
                }
            }
        }
    }

    // Explicit plugin flags without .so
    if let Some(need) = plugin_catalog::missing_plugin_hint(&raw, |id| registry.has(id)) {
        eprintln!(
            "tuider: need plugin `{need}` — build and copy .so to:\n  {}\n  (see docs/plugins.md)",
            plugins_dir.display()
        );
        return Err(ExitCode::from(2));
    }

    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }
    open_files(paths, recursive, list_mode)
}
fn open_files(paths: Vec<PathBuf>, recursive: bool, list_mode: bool) -> Result<(), ExitCode> {
    let mut docs = Vec::new();
    for p in &paths {
        if !p.exists() {
            eprintln!("tuider: not found: {}", p.display());
            return Err(ExitCode::from(1));
        }
        docs.extend(scan::scan_docs(p, recursive));
    }
    docs.sort_by(|a, b| a.1.cmp(&b.1));
    docs.dedup_by(|a, b| a.1 == b.1);
    docs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    if list_mode || !io::stdout().is_terminal() {
        return print_docs(&docs).map_err(|e| {
            eprintln!("tuider: {e}");
            ExitCode::from(1)
        });
    }
    if docs.is_empty() {
        eprintln!("tuider: no .md / .txt under given path(s)");
        return Err(ExitCode::from(1));
    }
    run_tui(Box::new(FileTreeSource::new(docs)))
}

fn run_tui(source: Box<dyn ContentSource>) -> Result<(), ExitCode> {
    let mut terminal = ratatui::init();
    let result = app::App::new(source).run(&mut terminal);
    ratatui::restore();
    result.map_err(|e| {
        eprintln!("tuider: {e}");
        ExitCode::from(1)
    })
}

fn print_docs(docs: &[(String, PathBuf)]) -> io::Result<()> {
    if docs.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no documents"));
    }
    if docs.len() != 1 {
        for (_, path) in docs {
            println!("{}", path.display());
        }
        return Ok(());
    }
    let text = std::fs::read_to_string(&docs[0].1)?;
    let mut out = io::stdout().lock();
    out.write_all(text.as_bytes())?;
    if !text.ends_with('\n') {
        out.write_all(b"\n")?;
    }
    Ok(())
}

fn print_help(registry: &PluginRegistry, plugins_dir: &std::path::Path) {
    let ai_line = if ai_compiled() {
        "    ai            compiled (default; Alt+L)\n"
    } else {
        "    ai            not in this build\n"
    };
    let mut plug = String::new();
    if registry.plugins.is_empty() {
        plug.push_str("    (none loaded — copy .so into plugins dir)\n");
    } else {
        for p in &registry.plugins {
            let en = if config::plugin_enabled(&p.id) {
                "on"
            } else {
                "disabled in yml"
            };
            plug.push_str(&format!("    {} — {} [{en}]\n", p.id, p.name));
        }
    }
    println!(
        "\
tuider {VERSION} — terminal UI reader (dynamic plugins)

USAGE:
    tuider [OPTIONS] [PATH...]
    tuider pkg list|install|remove …

CORE OPTIONS:
    -r, --recursive   scan dirs recursively
    -l, --print       print without TUI
    -h, --help
    -V, --version

CORE:
    md/txt reader, vim /, visual+yank
{ai_line}
PLUGINS DIR:
    {}
    env TUIDER_PLUGINS_DIR overrides

LOADED PLUGINS:
{plug}
PKG:
    tuider pkg list
    tuider pkg install <id|all>
    tuider pkg remove  <id|all>
See docs/plugins.md",
        plugins_dir.display()
    );
}
