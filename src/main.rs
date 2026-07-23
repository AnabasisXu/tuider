//! Tuider — slim terminal reader; plugins from .so directory.

#[cfg(feature = "ai")]
mod ai;
mod app;
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
use std::path::{Path, PathBuf};
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
    let mut cli_html = false;
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

    // D-CFG-03: list configured wordlists (-W; -L kept as alias)
    if raw.iter().any(|a| a == "-W" || a == "-L") {
        let wls = config::list_wordlists();
        if wls.is_empty() {
            println!("No wordlists configured. Add 'wordlists:' section to tuider.yml");
        } else {
            println!("Available wordlists:");
            for wl in &wls {
                println!("  {wl}");
            }
        }
        return Ok(());
    }


    let mut i = 0;
    while i < raw.len() {
        let a = &raw[i];
        match a.as_str() {
            "-r" | "--recursive" => recursive = true,
            "-l" | "--list" | "--print" | "--lite" => list_mode = true,
            "--html" => cli_html = true,
            "--db" => {}
            // plugin flags (+ their values): leave for plugin open
            "-u" | "--url" | "-hn" | "--hn" | "-g" | "--group" | "-s" | "--search" | "-n"
            | "--limit" | "--code" | "-m" | "-w" => {
                if matches!(
                    a.as_str(),
                    "-u" | "--url"
                        | "-g"
                        | "--group"
                        | "-s"
                        | "--search"
                        | "-n"
                        | "--limit"
                        | "-m"
                        | "-w"
                ) && i + 1 < raw.len()
                    && !raw[i + 1].starts_with('-')
                {
                    i += 1;
                }
            }
            s if s.starts_with('-') => {
                eprintln!("tuider: unknown flag `{s}`");
                return Err(ExitCode::from(2));
            }
            s => {
                let p = PathBuf::from(s);
                // gap §11: existing path / .mdx → path; else word (handled later)
                if is_path_token(s) {
                    paths.push(p);
                }
            }
        }
        i += 1;
    }

    let words = extract_cli_words(&raw);

    // Try each loaded plugin that claims these args
    for plug in &registry.plugins {
        if !config::plugin_enabled(&plug.id) {
            continue;
        }
        if plug.handles_args(&raw) || plugin_catalog::claims(plug.id.as_str(), &raw) {
            match plug.open_from_args(&raw) {
                Ok(mut src) => {
                    // --db: export only (plugin runs export in open); never TUI
                    if raw.iter().any(|a| a == "--db") {
                        return Ok(());
                    }
                    // D-CLI-01..05,07: dict claimed + word tokens → CLI lookup, no TUI
                    if plug.id == "dict" && !words.is_empty() {
                        return print_dict_cli(&mut src, &words, list_mode, cli_html);
                    }
                    if list_mode || !io::stdout().is_terminal() {
                        let search = raw.windows(2).find_map(|w| {
                            if matches!(w[0].as_str(), "-s" | "--search") {
                                Some(w[1].as_str())
                            } else {
                                None
                            }
                        });
                        if search.is_some() {
                            let name = src.entries().first().cloned().unwrap_or_default();
                            println!("{name}");
                            let res = src.load(0, 100);
                            for line in res.lines {
                                let plain: String =
                                    line.spans.iter().map(|s| s.content.as_ref()).collect();
                                println!("{plain}");
                            }
                        } else {
                            for e in src.entries() {
                                println!("{e}");
                            }
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

    // words without dict claim → hint -g
    if !words.is_empty() && paths.is_empty() {
        eprintln!(
            "tuider: word(s) {:?} need a dictionary (-g <group> or .mdx path)",
            words
        );
        return Err(ExitCode::from(2));
    }

    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }
    open_files(paths, recursive, list_mode)
}

/// gap §11 frozen: non-flag, non-existing path, not .mdx → word token(s).
fn is_path_token(s: &str) -> bool {
    if s.ends_with(".mdx") || s.ends_with(".MDX") {
        return true;
    }
    Path::new(s).exists()
}

fn parse_cli_words(raw: &str) -> Vec<String> {
    if raw.contains(',') {
        raw.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        let t = raw.trim();
        if t.is_empty() {
            Vec::new()
        } else {
            vec![t.to_owned()]
        }
    }
}

fn extract_cli_words(raw: &[String]) -> Vec<String> {
    let mut words = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "-g" | "--group" | "-s" | "--search" | "-n" | "--limit" | "-w" | "-u" | "--url"
            | "-m" => {
                i += 1;
                if i < raw.len() && !raw[i].starts_with('-') {
                    i += 1;
                }
            }
            s if s.starts_with('-') => i += 1,
            s => {
                if !is_path_token(s) {
                    words.extend(parse_cli_words(s));
                }
                i += 1;
            }
        }
    }
    words
}

fn merge_lookup_hits(hits: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (title, frag) in hits {
        if let Some(last) = out.last_mut() {
            if last.0 == title {
                last.1.push('\n');
                last.1.push_str(&frag);
                continue;
            }
        }
        out.push((title, frag));
    }
    out
}

fn strip_html_light(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn print_dict_cli(
    src: &mut dyn ContentSource,
    words: &[String],
    lite: bool,
    html: bool,
) -> Result<(), ExitCode> {
    // D-CLI-07: no ANSI in this path (plain strip; color optional skipped)
    let multi = words.len() > 1;
    for (wi, word) in words.iter().enumerate() {
        if multi && !html {
            if wi > 0 {
                println!();
            }
            println!("######## {word} ########");
        }
        let hits = merge_lookup_hits(src.lookup_word(word));
        if hits.is_empty() {
            println!("Not found: {word}");
            continue;
        }
        for (dict, body) in hits {
            if html {
                if multi {
                    println!("<!-- {word} / {dict} -->");
                }
                println!("{body}");
            } else if lite {
                // D-CLI-03: headword + dict only
                println!("{dict}: {word}");
            } else {
                println!("=== {dict} ===");
                let plain = strip_html_light(&body);
                let mut out = io::stdout().lock();
                let _ = writeln!(out, "{plain}");
            }
        }
    }
    Ok(())
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
    tuider [OPTIONS] [PATH...] [WORD...]
    tuider -g <group> <word>          CLI lookup (no TUI)
    tuider pkg list|install|remove …

CORE OPTIONS:
    -r, --recursive   scan dirs recursively
    -l, --print       print without TUI; with words: dict+headword only
    -g <group>        dict group (tuider.yml)
    -s, --search <w>  jump to headword; comma list = temp wordlist
    -n, --limit <N>   limit dict count (sorted by name)
    -w <name>         filter sidebar to wordlist
    -W                list configured wordlists
    --html            CLI: raw HTML definitions
    --db              export each .mdx to sibling .db (no TUI)
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
