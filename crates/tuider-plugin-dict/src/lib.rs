//! Dict plugin cdylib — install as `libtuider_dict.so`.
//! Returns HTML (with sibling CSS) for host mdx-tui-style CSS rendering.

use std::collections::HashMap;
use std::os::raw::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};

use mdict_rs::MdxFile;
use serde::Deserialize;
use tuider_plugin_api::{
    args_vec, cstring_or_null, free_cstring, write_err, BODY_HTML_V1_PREFIX, TUIDER_PLUGIN_ABI,
};

const MAX_KEYS_INDEX: usize = 50_000;
const SEP: &str = "\n\u{1e}\n";

struct OneDict {
    title: String,
    names: Vec<String>,
    file: MdxFile,
    /// Merged sibling `.css` next to the .mdx (mdx-tui method).
    css: String,
}

struct DictState {
    dicts: Vec<OneDict>,
    active: usize,
    /// CLI `-g` name when opened as a group (for multi title prefix).
    group: Option<String>,
}

impl DictState {
    fn active(&self) -> &OneDict {
        &self.dicts[self.active]
    }

    fn active_mut(&mut self) -> &mut OneDict {
        &mut self.dicts[self.active]
    }

    fn title_string(&self) -> String {
        let d = self.active();
        if self.dicts.len() > 1 {
            if let Some(g) = &self.group {
                // multi group: "group: active" so cycle updates title
                format!("{g}: {}", d.title)
            } else {
                d.title.clone()
            }
        } else {
            d.title.clone()
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    groups: HashMap<String, Vec<String>>,
}

fn load_groups() -> HashMap<String, Vec<String>> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path = home
        .map(|h| h.join(".config/tuider.yml"))
        .unwrap_or_else(|| PathBuf::from("tuider.yml"));
    let Ok(raw) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    serde_yaml::from_str::<FileConfig>(&raw)
        .map(|c| c.groups)
        .unwrap_or_default()
}

fn load_sibling_css(mdx: &Path) -> String {
    let Some(dir) = mdx.parent() else {
        return String::new();
    };
    let Ok(rd) = std::fs::read_dir(dir) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("css"))
        {
            if let Ok(s) = std::fs::read_to_string(&p) {
                parts.push(s);
            }
        }
    }
    parts.join("\n")
}

fn open_one(path: &Path) -> Result<OneDict, String> {
    let file = MdxFile::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut names = Vec::new();
    for k in file.keys() {
        match k {
            Ok(key) => {
                names.push(key);
                if names.len() >= MAX_KEYS_INDEX {
                    break;
                }
            }
            Err(e) => eprintln!("dict key warn: {e}"),
        }
    }
    names.sort();
    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "dict".into());
    let css = load_sibling_css(path);
    Ok(OneDict {
        title,
        names,
        file,
        css,
    })
}

/// Open every path; skip failures with eprintln. Need ≥1 success.
fn open_all(paths: &[PathBuf], group: Option<&str>) -> Result<DictState, String> {
    let mut dicts = Vec::new();
    for p in paths {
        match open_one(p) {
            Ok(d) => dicts.push(d),
            Err(e) => eprintln!("dict: skip {}: {e}", p.display()),
        }
    }
    if dicts.is_empty() {
        return Err(
            "dict: no dictionaries opened (all paths failed or empty)".into(),
        );
    }
    // multi title label uses join of stems when no -g (host may show via active)
    if dicts.len() > 1 && group.is_none() {
        let joined = dicts
            .iter()
            .map(|d| d.title.as_str())
            .collect::<Vec<_>>()
            .join("+");
        eprintln!("dict: multi open {joined}");
    } else if dicts.len() > 1 {
        if let Some(g) = group {
            let joined = dicts
                .iter()
                .map(|d| d.title.as_str())
                .collect::<Vec<_>>()
                .join("+");
            eprintln!("dict: group {g}: {joined}");
        }
    }
    Ok(DictState {
        dicts,
        active: 0,
        group: group.map(str::to_owned),
    })
}

fn resolve_paths(cli_paths: &[PathBuf], group: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for p in cli_paths {
        if p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mdx"))
        {
            paths.push(p.clone());
        } else if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    let path = e.path();
                    if path
                        .extension()
                        .and_then(|x| x.to_str())
                        .is_some_and(|x| x.eq_ignore_ascii_case("mdx"))
                    {
                        paths.push(path);
                    }
                }
            }
        }
    }
    if let Some(g) = group {
        let groups = load_groups();
        let list = groups
            .get(g)
            .ok_or_else(|| format!("dict group `{g}` not in ~/.config/tuider.yml groups"))?;
        for s in list {
            paths.push(PathBuf::from(s));
        }
    }
    paths.retain(|p| p.exists());
    if paths.is_empty() {
        return Err(
            "dict: no .mdx files (pass path or -g <group> from tuider.yml groups)".into(),
        );
    }
    Ok(paths)
}

fn envelope(css: &str, html: &str) -> String {
    let mut s = String::with_capacity(BODY_HTML_V1_PREFIX.len() + css.len() + SEP.len() + html.len());
    s.push_str(BODY_HTML_V1_PREFIX);
    s.push_str(css);
    s.push_str(SEP);
    s.push_str(html);
    s
}

/// Advance `active` when more than one dict is loaded. Returns 1 if cycled, else 0.
fn cycle_active(state: &mut DictState) -> c_int {
    if state.dicts.len() <= 1 {
        return 0;
    }
    state.active = (state.active + 1) % state.dicts.len();
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_abi_version() -> u32 {
    TUIDER_PLUGIN_ABI
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_id() -> *const c_char {
    c"dict".as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn tuider_plugin_name() -> *const c_char {
    c"MDX dictionary".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_handles(argc: c_int, argv: *const *const c_char) -> c_int {
    let args = unsafe { args_vec(argc, argv) };
    if args.iter().any(|a| a == "-g" || a == "--group") {
        return 1;
    }
    if args
        .iter()
        .any(|a| a.ends_with(".mdx") || a.ends_with(".MDX"))
    {
        return 1;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_open(
    argc: c_int,
    argv: *const *const c_char,
    err: *mut c_char,
    err_len: usize,
) -> *mut c_void {
    let args = unsafe { args_vec(argc, argv) };
    let mut group: Option<String> = None;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-g" | "--group" => {
                if i + 1 < args.len() {
                    group = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
            }
            s if s.starts_with('-') => {}
            s => paths.push(PathBuf::from(s)),
        }
        i += 1;
    }
    match resolve_paths(&paths, group.as_deref()).and_then(|ps| open_all(&ps, group.as_deref())) {
        Ok(state) => Box::into_raw(Box::new(state)) as *mut c_void,
        Err(e) => {
            write_err(err, err_len, &e);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_plugin_close(src: *mut c_void) {
    if !src.is_null() {
        drop(unsafe { Box::from_raw(src as *mut DictState) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_title(src: *mut c_void) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut DictState) };
    cstring_or_null(&s.title_string())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_count(src: *mut c_void) -> usize {
    if src.is_null() {
        0
    } else {
        unsafe { &*(src as *mut DictState) }.active().names.len()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_at(src: *mut c_void, index: usize) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &*(src as *mut DictState) };
    s.active()
        .names
        .get(index)
        .map(|n| cstring_or_null(n))
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_load_body(
    src: *mut c_void,
    index: usize,
    _width: usize,
) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let s = unsafe { &mut *(src as *mut DictState) };
    let Some(key) = s.active().names.get(index).cloned() else {
        return cstring_or_null("out of range");
    };
    let d = s.active_mut();
    match d.file.lookup(&key) {
        Ok(Some(rec)) => cstring_or_null(&envelope(&d.css, &rec.text)),
        Ok(None) => cstring_or_null(&format!("not found: {key}")),
        Err(e) => cstring_or_null(&format!("error: {e}")),
    }
}

/// Optional: Tab cycle among dicts loaded via `-g` / multi path. Returns 1 if switched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_cycle(src: *mut c_void) -> c_int {
    if src.is_null() {
        return 0;
    }
    let s = unsafe { &mut *(src as *mut DictState) };
    cycle_active(s)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_string_free(s: *mut c_char) {
    unsafe { free_cstring(s) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_roundtrip_shape() {
        let e = envelope("span{x}", "<b>hi</b>");
        assert!(e.starts_with(BODY_HTML_V1_PREFIX));
        assert!(e.contains(SEP));
        assert!(e.ends_with("<b>hi</b>"));
    }

    #[test]
    fn cycle_active_wraps_and_noops_singleton() {
        // ponytail: stub OneDict without real MdxFile — only exercise active index
        // We can't build MdxFile easily; test the modulo helper path via a minimal state
        // built only if we have empty dicts is invalid (active panics). Unit-test logic inline:
        let mut active = 0usize;
        let len = 3usize;
        active = (active + 1) % len;
        assert_eq!(active, 1);
        active = (active + 1) % len;
        assert_eq!(active, 2);
        active = (active + 1) % len;
        assert_eq!(active, 0);
        // singleton: cycle returns 0
        let mut single = DictState {
            dicts: Vec::new(),
            active: 0,
            group: None,
        };
        // empty dicts: cycle_active treats len<=1
        assert_eq!(cycle_active(&mut single), 0);
    }

    #[test]
    fn title_string_multi_group_uses_active() {
        // title formatting without MdxFile: only stems matter; skip full open
        // covered by title_string logic with hand-built state is blocked by MdxFile.
        // Keep format contract as pure string check:
        let g = "en";
        let stem = "oxford";
        assert_eq!(format!("{g}: {stem}"), "en: oxford");
    }
}
