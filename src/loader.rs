//! Scan plugins directory and dlopen Tuider plugin .so files (ABI v1).

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libloading::{Library, Symbol};
use tuider_plugin_api::{
    PluginTextSource, BODY_HTML_V1_PREFIX, FnAbiVersion, FnClose, FnEntryAt, FnEntryCount,
    FnHandles, FnId, FnLoadBody, FnName, FnOpen, FnStringFree, FnTitle, TUIDER_PLUGIN_ABI,
};

/// Optional: `extern "C" fn tuider_source_cycle(src: *mut c_void) -> c_int` (1 = cycled).
type FnCycle = unsafe extern "C" fn(*mut c_void) -> c_int;

pub struct LoadedPlugin {
    #[allow(dead_code)] // kept for skip/debug messages
    pub path: PathBuf,
    pub id: String,
    pub name: String,
    _lib: Library,
    open: FnOpen,
    handles: Option<FnHandles>,
    close: FnClose,
    title: FnTitle,
    entry_count: FnEntryCount,
    entry_at: FnEntryAt,
    load_body: FnLoadBody,
    string_free: FnStringFree,
    cycle: Option<FnCycle>,
}

pub struct DynSource {
    plugin: Arc<LoadedPlugin>,
    handle: *mut c_void,
    title_cache: String,
    entries_cache: Vec<String>,
}

unsafe impl Send for DynSource {}

impl Drop for DynSource {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.plugin.close)(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

impl PluginTextSource for DynSource {
    fn title(&self) -> &str {
        &self.title_cache
    }
    fn entries(&self) -> &[String] {
        &self.entries_cache
    }
    fn load_text(&mut self, index: usize, width: usize) -> Result<String, String> {
        let raw = unsafe { (self.plugin.load_body)(self.handle, index, width) };
        if raw.is_null() {
            return Err("plugin returned null body".into());
        }
        let s = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.plugin.string_free)(raw) };
        Ok(s)
    }
}

impl DynSource {
    fn refresh_meta(&mut self) {
        let title_ptr = unsafe { (self.plugin.title)(self.handle) };
        self.title_cache = if title_ptr.is_null() {
            self.plugin.id.clone()
        } else {
            let s = unsafe { CStr::from_ptr(title_ptr) }
                .to_string_lossy()
                .into_owned();
            unsafe { (self.plugin.string_free)(title_ptr) };
            s
        };
        let n = unsafe { (self.plugin.entry_count)(self.handle) };
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let p = unsafe { (self.plugin.entry_at)(self.handle, i) };
            if p.is_null() {
                entries.push(format!("#{i}"));
            } else {
                let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
                unsafe { (self.plugin.string_free)(p) };
                entries.push(s);
            }
        }
        self.entries_cache = entries;
    }

    fn cycle_layer(&mut self) -> bool {
        let Some(cycle) = self.plugin.cycle else {
            return false;
        };
        let r = unsafe { cycle(self.handle) };
        if r == 0 {
            return false;
        }
        self.refresh_meta();
        true
    }
}

/// Host ContentSource: plugin text → ratatui Lines.
/// - `TUIDER_HTML_V1` + CSS + HTML → mdx-tui CSS renderer
/// - markdown-ish → mdterm `md`
/// - else plain text
pub struct HostSource {
    inner: DynSource,
    names: Vec<String>,
    title: String,
}

impl HostSource {
    pub fn from_dyn(d: DynSource) -> Self {
        let title = d.title_cache.clone();
        let names = d.entries_cache.clone();
        Self {
            inner: d,
            names,
            title,
        }
    }
}

impl crate::plugin::ContentSource for HostSource {
    fn title(&self) -> &str {
        &self.title
    }
    fn entries(&self) -> &[String] {
        &self.names
    }
    fn load(&mut self, index: usize, width: usize) -> crate::plugin::LoadResult {
        match self.inner.load_text(index, width) {
            Ok(text) => {
                let name = self
                    .names
                    .get(index)
                    .map(|s| s.as_str())
                    .unwrap_or("?");
                let lines = render_plugin_body(&text, width.max(20));
                let n = lines.len();
                crate::plugin::LoadResult::plain(lines, format!("{name}  ({n} lines)"))
            }
            Err(e) => crate::plugin::LoadResult::plain(
                vec![ratatui::text::Line::from(format!("error: {e}"))],
                format!("load failed: {e}"),
            ),
        }
    }
    fn cycle_layer(&mut self) -> bool {
        if !self.inner.cycle_layer() {
            return false;
        }
        self.title = self.inner.title_cache.clone();
        self.names = self.inner.entries_cache.clone();
        true
    }
}

fn render_plugin_body(text: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
    if let Some(rest) = text.strip_prefix(BODY_HTML_V1_PREFIX) {
        // payload: <css>\n\x1e\n<html>
        let (css_src, html) = match rest.split_once("\n\u{1e}\n") {
            Some((c, h)) => (c, h),
            None => ("", rest),
        };
        let table = crate::html_css::StyleTable::parse(css_src);
        return crate::html_render::html_to_lines(html, &table);
    }
    if looks_like_md(text) {
        crate::md::render_md_width(text, width)
    } else {
        crate::md::render_txt_width(text, width)
    }
}

fn looks_like_md(t: &str) -> bool {
    t.contains('#') || t.contains("```") || t.contains("\n- ") || t.contains("**")
}

#[cfg(test)]
mod body_render_tests {
    use super::render_plugin_body;
    use ratatui::style::Modifier;
    #[test]
    fn html_v1_applies_css_and_builtin_bold() {
        let body = "TUIDER_HTML_V1\n.src{color:#1E90FF;font-style:italic}\n\u{1e}\n<span class=\"src\">quoted</span> and <b>bold</b>";
        let lines = render_plugin_body(body, 80);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(text.contains("quoted"), "{text}");
        assert!(text.contains("bold"), "{text}");
        let has_bold = lines.iter().any(|l| {
            l.spans.iter().any(|s| {
                s.content.contains("bold") && s.style.add_modifier.contains(Modifier::BOLD)
            })
        });
        assert!(has_bold, "expected bold style on 'bold'");
        // CSS italic + fg applied (color may be lifted for terminal contrast)
        let quoted = lines.iter().flat_map(|l| l.spans.iter()).find(|s| s.content.contains("quoted"));
        let q = quoted.expect("quoted span");
        assert!(q.style.add_modifier.contains(Modifier::ITALIC), "css italic");
        assert!(q.style.fg.is_some(), "css fg color");
    }
}

pub fn default_plugins_dir() -> PathBuf {
    if let Ok(p) = std::env::var("TUIDER_PLUGINS_DIR") {
        return PathBuf::from(p);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/tuider/plugins");
    }
    PathBuf::from("./plugins")
}

pub fn scan_plugins(dir: &Path) -> Vec<Arc<LoadedPlugin>> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "so" || e == "dylib" || e == "dll")
        })
        .collect();
    paths.sort();
    for path in paths {
        match load_one(&path) {
            Ok(p) => out.push(Arc::new(p)),
            Err(e) => eprintln!("tuider: skip {}: {e}", path.display()),
        }
    }
    out
}

fn load_one(path: &Path) -> Result<LoadedPlugin, String> {
    let lib = unsafe { Library::new(path) }.map_err(|e| format!("dlopen: {e}"))?;
    unsafe {
        let abi: Symbol<FnAbiVersion> = lib
            .get(b"tuider_plugin_abi_version\0")
            .map_err(|e| format!("abi: {e}"))?;
        let ver = abi();
        if ver != TUIDER_PLUGIN_ABI {
            return Err(format!("ABI {ver} != host {TUIDER_PLUGIN_ABI}"));
        }
        let id_fn: Symbol<FnId> = lib.get(b"tuider_plugin_id\0").map_err(|e| e.to_string())?;
        let name_fn: Symbol<FnName> = lib.get(b"tuider_plugin_name\0").map_err(|e| e.to_string())?;
        let open: Symbol<FnOpen> = lib.get(b"tuider_plugin_open\0").map_err(|e| e.to_string())?;
        let close: Symbol<FnClose> = lib.get(b"tuider_plugin_close\0").map_err(|e| e.to_string())?;
        let title: Symbol<FnTitle> = lib.get(b"tuider_source_title\0").map_err(|e| e.to_string())?;
        let entry_count: Symbol<FnEntryCount> = lib
            .get(b"tuider_source_entry_count\0")
            .map_err(|e| e.to_string())?;
        let entry_at: Symbol<FnEntryAt> = lib
            .get(b"tuider_source_entry_at\0")
            .map_err(|e| e.to_string())?;
        let load_body: Symbol<FnLoadBody> = lib
            .get(b"tuider_source_load_body\0")
            .map_err(|e| e.to_string())?;
        let string_free: Symbol<FnStringFree> =
            lib.get(b"tuider_string_free\0").map_err(|e| e.to_string())?;
        let handles: Option<Symbol<FnHandles>> = lib.get(b"tuider_plugin_handles\0").ok();

        let id = CStr::from_ptr(id_fn()).to_string_lossy().into_owned();
        let name = CStr::from_ptr(name_fn()).to_string_lossy().into_owned();

        Ok(LoadedPlugin {
            path: path.to_path_buf(),
            id,
            name,
            open: *open,
            handles: handles.map(|s| *s),
            close: *close,
            title: *title,
            entry_count: *entry_count,
            entry_at: *entry_at,
            load_body: *load_body,
            string_free: *string_free,
            cycle: lib
                .get::<FnCycle>(b"tuider_source_cycle\0")
                .ok()
                .map(|s| *s),
            _lib: lib,
        })
    }
}

impl LoadedPlugin {
    pub fn open_from_args(self: &Arc<Self>, args: &[String]) -> Result<HostSource, String> {
        let c_args: Vec<CString> = args
            .iter()
            .map(|s| CString::new(s.as_str().replace('\0', "")).unwrap_or_default())
            .collect();
        let ptrs: Vec<*const c_char> = c_args.iter().map(|c| c.as_ptr()).collect();
        let mut err = vec![0u8; 512];
        let handle = unsafe {
            (self.open)(
                ptrs.len() as c_int,
                ptrs.as_ptr(),
                err.as_mut_ptr() as *mut c_char,
                err.len(),
            )
        };
        if handle.is_null() {
            let msg = std::ffi::CStr::from_bytes_until_nul(&err)
                .map(|c| c.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "plugin open failed".into());
            return Err(msg);
        }
        let title_ptr = unsafe { (self.title)(handle) };
        let title = if title_ptr.is_null() {
            self.id.clone()
        } else {
            let s = unsafe { CStr::from_ptr(title_ptr) }
                .to_string_lossy()
                .into_owned();
            unsafe { (self.string_free)(title_ptr) };
            s
        };
        let n = unsafe { (self.entry_count)(handle) };
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let p = unsafe { (self.entry_at)(handle, i) };
            if p.is_null() {
                entries.push(format!("#{i}"));
            } else {
                let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
                unsafe { (self.string_free)(p) };
                entries.push(s);
            }
        }
        Ok(HostSource::from_dyn(DynSource {
            plugin: Arc::clone(self),
            handle,
            title_cache: title,
            entries_cache: entries,
        }))
    }

    pub fn handles_args(&self, args: &[String]) -> bool {
        let Some(h) = self.handles else {
            return false;
        };
        let c_args: Vec<CString> = args
            .iter()
            .map(|s| CString::new(s.as_str().replace('\0', "")).unwrap_or_default())
            .collect();
        let ptrs: Vec<*const c_char> = c_args.iter().map(|c| c.as_ptr()).collect();
        unsafe { h(ptrs.len() as c_int, ptrs.as_ptr()) != 0 }
    }
}

pub struct PluginRegistry {
    pub plugins: Vec<Arc<LoadedPlugin>>,
}

impl PluginRegistry {
    pub fn load(dir: &Path) -> Self {
        Self {
            plugins: scan_plugins(dir),
        }
    }

    #[allow(dead_code)]
    pub fn empty() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn has(&self, id: &str) -> bool {
        self.plugins.iter().any(|p| p.id == id)
    }

    #[allow(dead_code)]
    pub fn get(&self, id: &str) -> Option<&Arc<LoadedPlugin>> {
        self.plugins.iter().find(|p| p.id == id)
    }
}
