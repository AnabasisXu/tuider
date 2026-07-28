//! Dict plugin cdylib — install as `libtuider_dict.so`.
//! Returns HTML (with sibling CSS) for host mdx-tui-style CSS rendering.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::os::raw::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};

use flate2::write::ZlibEncoder;
use flate2::Compression;
use mdict_rs::MdxFile;
use rusqlite::{params, Connection};
use serde::Deserialize;
use tuider_plugin_api::{
    args_vec, cstring_or_null, free_cstring, write_err, BODY_HTML_V1_PREFIX, TUIDER_PLUGIN_ABI,
};

const SEP: &str = "\n\u{1e}\n";

struct OneDict {
    title: String,
    names: Vec<String>,
    file: MdxFile,
    /// Merged sibling `.css` next to the .mdx (mdx-tui method).
    css: String,
    path: PathBuf,
    /// Lazy: lowercased headword + plain definition, parallel to `names`.
    ft_cache: Option<FulltextCache>,
}

/// Built once on first fulltext_search; avoids repeated MDX lookup + strip_tags.
struct FulltextCache {
    head_lowers: Vec<String>,
    body_lowers: Vec<String>,
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
    #[serde(default)]
    wordlists: HashMap<String, String>,
}

fn config_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".config/tuider.yml"))
        .unwrap_or_else(|| PathBuf::from("tuider.yml"))
}

fn load_file_config() -> FileConfig {
    let Ok(raw) = std::fs::read_to_string(config_path()) else {
        return FileConfig::default();
    };
    serde_yaml::from_str(&raw).unwrap_or_default()
}

fn load_groups() -> HashMap<String, Vec<String>> {
    load_file_config().groups
}

fn load_wordlist_set(name: &str) -> Result<HashSet<String>, String> {
    let cfg = load_file_config();
    let path = cfg
        .wordlists
        .get(name)
        .ok_or_else(|| format!("dict: wordlist `{name}` not in tuider.yml wordlists"))?;
    let full = if Path::new(path).is_relative() {
        config_path()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    } else {
        PathBuf::from(path)
    };
    let text = std::fs::read_to_string(&full)
        .map_err(|e| format!("dict: wordlist `{name}` read {}: {e}", full.display()))?;
    let set: HashSet<String> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(str::to_owned)
        .collect();
    if set.is_empty() {
        return Err(format!("dict: wordlist `{name}` empty"));
    }
    Ok(set)
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
    // ponytail: full key list in RAM; was 50k cap (missed late keys like "make")
    let mut names = Vec::new();
    for k in file.keys() {
        match k {
            Ok(key) => names.push(key),
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
        path: path.to_path_buf(),
        ft_cache: None,
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
        return Err("dict: no dictionaries opened (all paths failed or empty)".into());
    }
    if dicts.len() > 1 {
        let joined = dicts
            .iter()
            .map(|d| d.title.as_str())
            .collect::<Vec<_>>()
            .join("+");
        if let Some(g) = group {
            eprintln!("dict: group {g}: {joined}");
        } else {
            eprintln!("dict: multi open {joined}");
        }
    }
    Ok(DictState {
        dicts,
        active: 0,
        group: group.map(str::to_owned),
    })
}

/// Prefer exact (ci) → prefix → contains. Move hit to front; multi-dict switch active.
fn apply_search(state: &mut DictState, q: &str) -> Result<(), String> {
    let q = q.trim();
    if q.is_empty() {
        return Err("dict: -s needs a non-empty word".into());
    }
    let order: Vec<usize> = std::iter::once(state.active)
        .chain((0..state.dicts.len()).filter(|&i| i != state.active))
        .collect();
    for di in order {
        if let Some(ki) = pick_key_index(&state.dicts[di].names, q) {
            state.active = di;
            let key = state.dicts[di].names.remove(ki);
            state.dicts[di].names.insert(0, key.clone());
            state.dicts[di].ft_cache = None; // names reordered
            eprintln!("dict: -s `{q}` → {} / {}", state.dicts[di].title, key);
            return Ok(());
        }
    }
    Err(format!("dict: -s `{q}` not found in loaded dictionaries"))
}

fn pick_key_index(names: &[String], q: &str) -> Option<usize> {
    let ql = q.to_lowercase();
    names
        .iter()
        .position(|n| n.eq_ignore_ascii_case(q))
        .or_else(|| names.iter().position(|n| n.to_lowercase().starts_with(&ql)))
        .or_else(|| names.iter().position(|n| n.to_lowercase().contains(&ql)))
}

fn parse_comma_words(raw: &str) -> Vec<String> {
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

fn apply_wordlist_filter(state: &mut DictState, wl: &HashSet<String>) {
    for d in &mut state.dicts {
        d.names.retain(|n| wl.contains(n));
        d.ft_cache = None; // names order/set changed
    }
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
        // non-path word tokens ignored here
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

fn cycle_active(state: &mut DictState) -> c_int {
    if state.dicts.len() <= 1 {
        return 0;
    }
    state.active = (state.active + 1) % state.dicts.len();
    1
}

fn zlib_compress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data).map_err(|e| e.to_string())?;
    enc.finish().map_err(|e| e.to_string())
}

/// One .mdx → sibling .db (meta + mdx zlib blobs). // ponytail: sync full export
fn export_dict_to_db(dict: &OneDict) -> Result<PathBuf, String> {
    let db_path = dict.path.with_extension("db");
    if db_path.exists() {
        std::fs::remove_file(&db_path).map_err(|e| e.to_string())?;
    }
    let mut conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    {
        tx.execute_batch(
            "DROP TABLE IF EXISTS meta;
             DROP TABLE IF EXISTS mdx;
             CREATE TABLE meta (key TEXT NOT NULL, value TEXT NOT NULL);
             CREATE TABLE mdx (entry TEXT NOT NULL, paraphrase BLOB NOT NULL);",
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params!["title", dict.title.as_str()],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params!["zip", "1"],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params!["word_count", dict.names.len().to_string()],
        )
        .map_err(|e| e.to_string())?;
        let mut stmt = tx
            .prepare("INSERT INTO mdx (entry, paraphrase) VALUES (?1, ?2)")
            .map_err(|e| e.to_string())?;
        for rec in dict.file.entries() {
            let rec = rec.map_err(|e| e.to_string())?;
            if rec.text.trim().is_empty() {
                continue;
            }
            let compressed = zlib_compress(rec.text.as_bytes())?;
            stmt.execute(params![rec.key, compressed])
                .map_err(|e| e.to_string())?;
        }
        drop(stmt);
        tx.execute_batch("CREATE INDEX mdx_entry_index ON mdx (entry);")
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    let _ = conn.execute_batch("VACUUM");
    Ok(db_path)
}

fn lookup_in_dict(dict: &OneDict, word: &str) -> Option<String> {
    let ql = word.to_lowercase();
    // exact → lower → first pick_key hit
    if let Ok(Some(rec)) = dict.file.lookup(word) {
        return Some(rec.text);
    }
    if ql != word {
        if let Ok(Some(rec)) = dict.file.lookup(&ql) {
            return Some(rec.text);
        }
    }
    if let Some(ki) = pick_key_index(&dict.names, word) {
        let key = &dict.names[ki];
        if let Ok(Some(rec)) = dict.file.lookup(key) {
            return Some(rec.text);
        }
    }
    None
}

/// Format: `TITLE\tline` per line (host merges same title).
fn format_lookup_results(hits: &[(String, String)]) -> String {
    let mut out = String::new();
    for (title, body) in hits {
        for line in body.lines() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(title);
            out.push('\t');
            out.push_str(line);
        }
        if body.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(title);
            out.push('\t');
        }
    }
    out
}

fn strip_tags_light(html: &str) -> String {
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

fn src_state<'a>(src: *mut c_void) -> Option<&'a mut DictState> {
    if src.is_null() {
        None
    } else {
        Some(unsafe { &mut *(src as *mut DictState) })
    }
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
    let mut search: Option<String> = None;
    let mut wordlist_name: Option<String> = None;
    let mut max_dicts: Option<usize> = None;
    let mut export_db = false;
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
            "-s" | "--search" => {
                if i + 1 < args.len() {
                    search = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
            }
            "-w" => {
                if i + 1 < args.len() {
                    wordlist_name = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
            }
            "-n" | "--limit" => {
                if i + 1 < args.len() {
                    max_dicts = args[i + 1].parse().ok();
                    i += 2;
                    continue;
                }
            }
            "--db" => {
                export_db = true;
            }
            "--html" | "-l" | "--list" | "--print" | "-L" | "-W" | "-r" | "--recursive" | "-V"
            | "--version" | "-h" | "--help" => {}

            s if s.starts_with('-') => {}
            s => {
                // only treat as path candidate; words are ignored by resolve_paths
                paths.push(PathBuf::from(s));
            }
        }
        i += 1;
    }

    let opened = resolve_paths(&paths, group.as_deref())
        .and_then(|ps| open_all(&ps, group.as_deref()))
        .and_then(|mut st| {
            if let Some(n) = max_dicts {
                st.dicts.sort_by(|a, b| a.title.cmp(&b.title));
                st.dicts.truncate(n.max(1));
                if st.dicts.is_empty() {
                    return Err("dict: -n left zero dictionaries".into());
                }
                st.active = 0;
            }
            let mut wl: Option<HashSet<String>> = None;
            if let Some(name) = wordlist_name.as_deref() {
                let set = load_wordlist_set(name)?;
                eprintln!("dict: wordlist `{name}`: {} words", set.len());
                wl = Some(set);
            }
            // -s "a,b" → temporary wordlist filter (not jump)
            if let Some(raw) = search.as_deref() {
                let parts = parse_comma_words(raw);
                if parts.len() > 1 {
                    let mut set: HashSet<String> = parts.into_iter().collect();
                    if let Some(existing) = wl.take() {
                        set.extend(existing);
                    }
                    eprintln!("dict: inline wordlist (-s): {} words", set.len());
                    wl = Some(set);
                    search = None; // consumed as filter
                }
            }
            if let Some(set) = &wl {
                apply_wordlist_filter(&mut st, set);
            }
            if let Some(q) = search.as_deref() {
                apply_search(&mut st, q)?;
            }
            if export_db {
                eprintln!(
                    "dict: exporting {} dictionar{}…",
                    st.dicts.len(),
                    if st.dicts.len() == 1 { "y" } else { "ies" }
                );
                for d in &st.dicts {
                    match export_dict_to_db(d) {
                        Ok(p) => eprintln!("dict: exported {}", p.display()),
                        Err(e) => eprintln!("dict: export {}: {e}", d.path.display()),
                    }
                }
            }
            Ok(st)
        });

    match opened {
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
    let Some(s) = src_state(src) else {
        return std::ptr::null_mut();
    };
    cstring_or_null(&s.title_string())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_count(src: *mut c_void) -> usize {
    src_state(src).map(|s| s.active().names.len()).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_entry_at(src: *mut c_void, index: usize) -> *mut c_char {
    let Some(s) = src_state(src) else {
        return std::ptr::null_mut();
    };
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
    let Some(s) = src_state(src) else {
        return std::ptr::null_mut();
    };
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_source_cycle(src: *mut c_void) -> c_int {
    let Some(s) = src_state(src) else {
        return 0;
    };
    cycle_active(s)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_string_free(s: *mut c_char) {
    unsafe { free_cstring(s) };
}

// ── Optional dict ABI (weak-bound by host; no ABI bump) ───────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_dict_lookup(src: *mut c_void, word: *const c_char) -> *mut c_char {
    let Some(s) = src_state(src) else {
        return std::ptr::null_mut();
    };
    if word.is_null() {
        return cstring_or_null("");
    }
    let w = unsafe { std::ffi::CStr::from_ptr(word) }
        .to_string_lossy()
        .into_owned();
    let mut hits = Vec::new();
    for d in &s.dicts {
        if let Some(html) = lookup_in_dict(d, &w) {
            hits.push((d.title.clone(), html));
        }
    }
    cstring_or_null(&format_lookup_results(&hits))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_dict_search(
    src: *mut c_void,
    prefix: *const c_char,
    limit: usize,
) -> *mut c_char {
    let Some(s) = src_state(src) else {
        return std::ptr::null_mut();
    };
    if prefix.is_null() {
        return cstring_or_null("");
    }
    let p = unsafe { std::ffi::CStr::from_ptr(prefix) }
        .to_string_lossy()
        .to_lowercase();
    let lim = if limit == 0 { 50 } else { limit };
    let mut out = Vec::new();
    // active first, then others; dedupe
    let order: Vec<usize> = std::iter::once(s.active)
        .chain((0..s.dicts.len()).filter(|&i| i != s.active))
        .collect();
    let mut seen = HashSet::new();
    for di in order {
        for n in &s.dicts[di].names {
            if n.to_lowercase().starts_with(&p) && seen.insert(n.clone()) {
                out.push(n.clone());
                if out.len() >= lim {
                    return cstring_or_null(&out.join("\n"));
                }
            }
        }
    }
    cstring_or_null(&out.join("\n"))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_dict_reverse(
    src: *mut c_void,
    query: *const c_char,
    limit: usize,
) -> *mut c_char {
    let Some(s) = src_state(src) else {
        return std::ptr::null_mut();
    };
    if query.is_null() {
        return cstring_or_null("");
    }
    let q = unsafe { std::ffi::CStr::from_ptr(query) }
        .to_string_lossy()
        .to_lowercase();
    if q.is_empty() {
        return cstring_or_null("");
    }
    let lim = if limit == 0 { 20 } else { limit };
    let mut out = Vec::new();
    // ponytail: O(n) scan all defs; stop at lim hits — reverse index if huge dicts
    for d in &s.dicts {
        for name in &d.names {
            if out.len() >= lim {
                break;
            }
            let Ok(Some(rec)) = d.file.lookup(name) else {
                continue;
            };
            let plain = strip_tags_light(&rec.text).to_lowercase();
            if plain.contains(&q) {
                out.push(name.clone());
            }
        }
        if out.len() >= lim {
            break;
        }
    }
    cstring_or_null(&out.join("\n"))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_dict_fulltext_search(
    src: *mut c_void,
    query: *const c_char,
    limit: usize,
) -> *mut c_char {
    let Some(s) = src_state(src) else {
        return std::ptr::null_mut();
    };
    if query.is_null() {
        return cstring_or_null("");
    }
    let q = unsafe { std::ffi::CStr::from_ptr(query) }
        .to_string_lossy()
        .to_lowercase();
    let tokens: Vec<&str> = q
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return cstring_or_null("");
    }
    let lim = if limit == 0 { 500 } else { limit };
    ensure_fulltext_caches(&mut s.dicts);
    let out = search_fulltext_cached(&s.dicts, &tokens, lim);
    cstring_or_null(&out.join("\n"))
}

fn ensure_fulltext_caches(dicts: &mut [OneDict]) {
    for d in dicts {
        if d.ft_cache.is_some() {
            continue;
        }
        let mut head_lowers = Vec::with_capacity(d.names.len());
        let mut body_lowers = Vec::with_capacity(d.names.len());
        for name in &d.names {
            head_lowers.push(name.to_lowercase());
            let body = match d.file.lookup(name) {
                Ok(Some(rec)) => strip_tags_light(&rec.text).to_lowercase(),
                _ => String::new(),
            };
            body_lowers.push(body);
        }
        d.ft_cache = Some(FulltextCache {
            head_lowers,
            body_lowers,
        });
    }
}

/// Headword hits first, then definition-only; token AND on lowercased text.
fn search_fulltext_cached(dicts: &[OneDict], tokens: &[&str], lim: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for d in dicts {
        let Some(cache) = &d.ft_cache else {
            continue;
        };
        append_fulltext_hits(
            &d.names,
            &cache.head_lowers,
            &cache.body_lowers,
            tokens,
            lim,
            &mut out,
            &mut seen,
        );
        if out.len() >= lim {
            break;
        }
    }
    out
}

fn append_fulltext_hits(
    names: &[String],
    head_lowers: &[String],
    body_lowers: &[String],
    tokens: &[&str],
    lim: usize,
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    // 1) all tokens in headword
    for (i, hw) in head_lowers.iter().enumerate() {
        if out.len() >= lim {
            return;
        }
        if tokens.iter().all(|t| hw.contains(t)) && seen.insert(names[i].clone()) {
            out.push(names[i].clone());
        }
    }
    // 2) each token in head OR body (same as host corpus)
    for (i, body) in body_lowers.iter().enumerate() {
        if out.len() >= lim {
            return;
        }
        if seen.contains(&names[i]) {
            continue;
        }
        let hw = head_lowers.get(i).map(|s| s.as_str()).unwrap_or("");
        if tokens
            .iter()
            .all(|t| hw.contains(t) || body.contains(t))
        {
            seen.insert(names[i].clone());
            out.push(names[i].clone());
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_dict_list(src: *mut c_void) -> *mut c_char {
    let Some(s) = src_state(src) else {
        return std::ptr::null_mut();
    };
    let titles: Vec<&str> = s.dicts.iter().map(|d| d.title.as_str()).collect();
    cstring_or_null(&titles.join("\n"))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuider_dict_select(src: *mut c_void, index: usize) -> c_int {
    let Some(s) = src_state(src) else {
        return 0;
    };
    if index >= s.dicts.len() {
        return 0;
    }
    s.active = index;
    1
}

// re-export for unit tests of pure helpers
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
    fn cycle_active_empty_noops() {
        let mut single = DictState {
            dicts: Vec::new(),
            active: 0,
            group: None,
        };
        assert_eq!(cycle_active(&mut single), 0);
    }

    #[test]
    fn pick_key_prefers_exact_then_prefix() {
        let names = vec![
            "maker".into(),
            "Make".into(),
            "remake".into(),
            "makefile".into(),
        ];
        assert_eq!(pick_key_index(&names, "make"), Some(1));
        assert_eq!(pick_key_index(&names, "makef"), Some(3));
        assert_eq!(pick_key_index(&names, "rem"), Some(2));
        assert_eq!(pick_key_index(&names, "zzz"), None);
    }

    #[test]
    fn parse_comma_words_batch() {
        assert_eq!(parse_comma_words("take,make"), vec!["take", "make"]);
        assert_eq!(parse_comma_words("  hi  "), vec!["hi"]);
        assert!(parse_comma_words("").is_empty());
    }

    #[test]
    fn format_lookup_multiline_tabs() {
        let s = format_lookup_results(&[("D1".into(), "a\nb".into())]);
        assert_eq!(s, "D1\ta\nD1\tb");
    }

    #[test]
    fn strip_tags_basic() {
        assert_eq!(strip_tags_light("<p>Hi &amp; <b>x</b></p>"), "Hi & x");
    }

    #[test]
    fn wordlist_filter_retains() {
        let mut st = DictState {
            dicts: vec![],
            active: 0,
            group: None,
        };
        // empty dicts: filter is no-op
        let wl: HashSet<String> = ["a".into()].into_iter().collect();
        apply_wordlist_filter(&mut st, &wl);
        assert!(st.dicts.is_empty());
    }

    #[test]
    fn fulltext_token_and_prefers_headword() {
        let names = vec!["apple".into(), "banana".into(), "cherry pie".into()];
        let heads = vec!["apple".into(), "banana".into(), "cherry pie".into()];
        let bodies = vec![
            "a fruit red".into(),
            "yellow fruit banana peel".into(),
            "dessert with cream".into(),
        ];
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        append_fulltext_hits(
            &names,
            &heads,
            &bodies,
            &["fruit"],
            10,
            &mut out,
            &mut seen,
        );
        assert_eq!(out, vec!["apple", "banana"]);

        out.clear();
        seen.clear();
        append_fulltext_hits(
            &names,
            &heads,
            &bodies,
            &["cherry", "pie"],
            10,
            &mut out,
            &mut seen,
        );
        assert_eq!(out, vec!["cherry pie"]);

        out.clear();
        seen.clear();
        append_fulltext_hits(
            &names,
            &heads,
            &bodies,
            &["banana", "peel"],
            10,
            &mut out,
            &mut seen,
        );
        // head has banana, body has peel → cross-field OR
        assert_eq!(out, vec!["banana"]);

        out.clear();
        seen.clear();
        append_fulltext_hits(
            &names,
            &heads,
            &bodies,
            &["cherry", "cream"],
            10,
            &mut out,
            &mut seen,
        );
        assert_eq!(out, vec!["cherry pie"]);
    }

    #[test]
    fn real_mdx_fulltext_cache_speeds_second_query() {
        let path = PathBuf::from(
            "/root/dict/英语常用词疑难用法手册/英语常用词疑难用法手册.mdx",
        );
        if !path.exists() {
            return;
        }
        let mut st = open_all(&[path], None).expect("open mdx");
        let n = st.dicts[0].names.len();
        assert!(n > 100, "expected real dict size, got {n}");

        let t0 = std::time::Instant::now();
        ensure_fulltext_caches(&mut st.dicts);
        let build = t0.elapsed();

        let t1 = std::time::Instant::now();
        let hits1 = search_fulltext_cached(&st.dicts, &["make"], 50);
        let q1 = t1.elapsed();

        let t2 = std::time::Instant::now();
        let hits2 = search_fulltext_cached(&st.dicts, &["take", "care"], 50);
        let q2 = t2.elapsed();

        assert!(!hits1.is_empty(), "make should hit");
        assert!(
            st.dicts[0].ft_cache.is_some(),
            "cache must stay after search"
        );
        eprintln!(
            "real mdx n={n} build={build:?} q_make={q1:?} hits={} q_take_care={q2:?} hits={}",
            hits1.len(),
            hits2.len()
        );
        // second query must be pure memory scan — well under a second for this book size
        assert!(
            q1.as_millis() < 500 && q2.as_millis() < 500,
            "cached queries too slow: {q1:?} {q2:?}"
        );
        let _ = hits2;
    }

    #[test]
    fn fulltext_respects_limit() {
        let names: Vec<String> = (0..20).map(|i| format!("w{i}")).collect();
        let heads: Vec<String> = names.iter().map(|s| s.to_lowercase()).collect();
        let bodies: Vec<String> = names.iter().map(|_| "hit me".into()).collect();
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        append_fulltext_hits(&names, &heads, &bodies, &["hit"], 5, &mut out, &mut seen);
        assert_eq!(out.len(), 5);
    }
}
