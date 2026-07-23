//! Load `tuider.yml` / `.tuider.yml` (cwd → parents → `~/.config/tuider.yml`).
//!
//! Plugin-specific keys live under `plugins:`; core only parses what it needs
//! (AI providers, paths). Uncompiled plugins simply never read their section.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)] // groups/plugins reserved for dict/url/hn/code readers
pub struct FileConfig {
    #[serde(default)]
    pub wordlists: HashMap<String, String>,
    #[serde(default)]
    pub groups: HashMap<String, Vec<String>>,
    pub ai: Option<AiSection>,
    pub plugins: Option<PluginsSection>,
    pub docs_root: Option<String>,
    pub plugins_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AiSection {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub providers: Option<Vec<ProviderEntry>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderEntry {
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct PluginsSection {
    pub url: Option<UrlPluginCfg>,
    pub hn: Option<HnPluginCfg>,
    pub dict: Option<DictPluginCfg>,
    pub code: Option<CodePluginCfg>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct UrlPluginCfg {
    pub enabled: Option<bool>,
    #[serde(default = "default_true")]
    pub cache: bool,

}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct HnPluginCfg {
    pub enabled: Option<bool>,
    pub default_limit: Option<usize>,
    pub comments: Option<usize>,

}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct DictPluginCfg {
    pub enabled: Option<bool>,
    pub default_group: Option<String>,

}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct CodePluginCfg {
    pub enabled: Option<bool>,
    #[serde(default)]
    pub recursive: bool,

}

fn default_true() -> bool {
    true
}

/// Whether a plugin may load at runtime.
/// - missing `plugins` section → all compiled plugins allowed
/// - missing plugin key → allowed
/// - `enabled: false` → blocked even if Cargo feature is on
pub fn plugin_enabled(id: &str) -> bool {
    let Some((_, cfg)) = load() else {
        return true;
    };
    let Some(plugins) = cfg.plugins else {
        return true;
    };
    let en = match id {
        "url" => plugins.url.and_then(|u| u.enabled),
        "hn" => plugins.hn.and_then(|h| h.enabled),
        "dict" => plugins.dict.and_then(|d| d.enabled),
        "code" => plugins.code.and_then(|c| c.enabled),
        _ => None,
    };
    en.unwrap_or(true)
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AiProvider {
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

/// Search order: `./tuider.yml`, `./.tuider.yml`, parents, then `~/.config/tuider.yml`.
pub fn config_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd;
        for _ in 0..8 {
            paths.push(dir.join("tuider.yml"));
            paths.push(dir.join(".tuider.yml"));
            paths.push(dir.join("tuider.yaml"));
            if !dir.pop() {
                break;
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".config/tuider.yml"));
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        paths.push(PathBuf::from(xdg).join("tuider.yml"));
    }
    paths
}

pub fn load() -> Option<(PathBuf, FileConfig)> {
    for p in config_search_paths() {
        if p.is_file() {
            match load_path(&p) {
                Ok(cfg) => return Some((p, cfg)),
                Err(e) => {
                    eprintln!("tuider: config {}: {e}", p.display());
                }
            }
        }
    }
    None
}

pub fn load_path(path: &Path) -> Result<FileConfig, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_yaml::from_str(&text).map_err(|e| format!("YAML: {e}"))
}

/// Names from `wordlists:` in the first loadable tuider.yml.
///
/// Dual-read: if tuider.yml has no wordlists, also try `~/.config/mdx-tui.yml`
/// (same key shape) so mdx-tui configs keep working.
pub fn list_wordlists() -> Vec<String> {
    let mut keys: Vec<String> = load()
        .map(|(_, c)| c.wordlists.keys().cloned().collect())
        .unwrap_or_default();
    if keys.is_empty() {
        if let Some(extra) = load_legacy_mdx_tui_wordlists() {
            keys.extend(extra.keys().cloned());
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

/// Load a named wordlist file (one word per line). Relative paths resolve
/// against the config file directory.
#[allow(dead_code)] // host API; dict plugin currently loads wordlists itself
pub fn load_wordlist(name: &str) -> Option<std::collections::HashSet<String>> {
    let (cfg_path, map) = if let Some((p, c)) = load() {
        if c.wordlists.contains_key(name) {
            (p, c.wordlists)
        } else if let Some(legacy) = load_legacy_mdx_tui_wordlists() {
            (dirs_config().join("mdx-tui.yml"), legacy)
        } else {
            return None;
        }
    } else if let Some(legacy) = load_legacy_mdx_tui_wordlists() {
        (dirs_config().join("mdx-tui.yml"), legacy)
    } else {
        return None;
    };
    let rel = map.get(name)?;
    let full = if Path::new(rel).is_relative() {
        cfg_path.parent()?.join(rel)
    } else {
        PathBuf::from(rel)
    };
    let text = fs::read_to_string(full).ok()?;
    let set: std::collections::HashSet<String> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(str::to_owned)
        .collect();
    if set.is_empty() {
        None
    } else {
        Some(set)
    }
}

fn dirs_config() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        PathBuf::from(".config")
    }
}

fn load_legacy_mdx_tui_wordlists() -> Option<HashMap<String, String>> {
    let p = dirs_config().join("mdx-tui.yml");
    let cfg = load_path(&p).ok()?;
    if cfg.wordlists.is_empty() {
        None
    } else {
        Some(cfg.wordlists)
    }
}

/// Resolve AI providers from config file + env fallback.
#[allow(dead_code)] // used from ai.rs when feature=ai
pub fn ai_providers(file: Option<&FileConfig>) -> Vec<AiProvider> {
    let mut out = Vec::new();
    if let Some(cfg) = file {
        if let Some(ai) = &cfg.ai {
            out.extend(ai_section_to_providers(ai));
        }
    }
    if out.is_empty() {
        if let Some(p) = env_provider() {
            out.push(p);
        }
    }
    out
}

#[allow(dead_code)]
fn env_provider() -> Option<AiProvider> {
    let api_key = std::env::var("TUIDER_AI_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .or_else(|_| std::env::var("AI_API_KEY"))
        .ok()?;
    if api_key.trim().is_empty() {
        return None;
    }
    let base_url = std::env::var("TUIDER_AI_BASE_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1".into());
    let model = std::env::var("TUIDER_AI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
    Some(AiProvider {
        name: "env".into(),
        api_key,
        base_url: base_url.trim_end_matches('/').to_string(),
        model,
    })
}


fn ai_section_to_providers(ai: &AiSection) -> Vec<AiProvider> {
    let default_key = ai
        .api_key
        .clone()
        .or_else(|| std::env::var("TUIDER_AI_KEY").ok())
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .or_else(|| std::env::var("AI_API_KEY").ok())
        .unwrap_or_default();

    if let Some(list) = &ai.providers {
        return list
            .iter()
            .map(|e| AiProvider {
                name: e.name.clone(),
                api_key: e
                    .api_key
                    .clone()
                    .unwrap_or_else(|| default_key.clone()),
                base_url: e.base_url.trim_end_matches('/').to_string(),
                model: e.model.clone(),
            })
            .filter(|p| !p.api_key.is_empty())
            .collect();
    }

    // single-provider shape
    let api_key = default_key;
    if api_key.is_empty() && ai.base_url.is_none() && ai.model.is_none() {
        return Vec::new();
    }
    if api_key.is_empty() {
        return Vec::new();
    }
    let base_url = ai
        .base_url
        .clone()
        .unwrap_or_else(|| "https://api.openai.com/v1".into());
    let model = ai.model.clone().unwrap_or_else(|| "gpt-4o-mini".into());
    let name = ai.provider.clone().unwrap_or_else(|| "default".into());
    vec![AiProvider {
        name,
        api_key,
        base_url: base_url.trim_end_matches('/').to_string(),
        model,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_ai_providers() {
        let y = r#"
ai:
  providers:
    - name: a
      base_url: https://example.com/v1
      model: m
      api_key: k
plugins:
  hn:
    default_limit: 10
"#;
        let cfg: FileConfig = serde_yaml::from_str(y).unwrap();
        let p = ai_section_to_providers(cfg.ai.as_ref().unwrap());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].name, "a");
        assert_eq!(cfg.plugins.unwrap().hn.unwrap().default_limit, Some(10));
    }
}
