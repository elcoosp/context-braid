pub mod ipc;

pub use ipc::{AgentStatus, BrowserCommand, IpcMessage, ProjectWorkspace, TabContextPayload};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReposConfig {
    pub root_dir: String,
    #[serde(default = "default_ignore")]
    pub ignore: Vec<String>,
}

fn default_ignore() -> Vec<String> {
    vec![
        ".git".into(),
        "node_modules".into(),
        "target".into(),
        ".build".into(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_socket_path")]
    pub socket_path: String,
}

fn default_socket_path() -> String {
    "/tmp/braid.sock".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    #[serde(default = "default_favorite_editor")]
    pub favorite: String,
}

fn default_favorite_editor() -> String {
    "zed".into()
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            favorite: default_favorite_editor(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationsConfig {
    #[serde(default)]
    pub notify_when_focused: bool,
    #[serde(default = "default_min_working_ms")]
    pub min_working_ms: u64,
}

fn default_min_working_ms() -> u64 {
    1_200
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            notify_when_focused: false,
            min_working_ms: default_min_working_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BraidConfig {
    pub repos: ReposConfig,
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub auto_download_zip: bool,
    #[serde(default)]
    pub notifications: NotificationsConfig,
}

impl Default for BraidConfig {
    fn default() -> Self {
        Self {
            repos: ReposConfig {
                root_dir: "~/Documents/Repos".into(),
                ignore: default_ignore(),
            },
            daemon: DaemonConfig {
                socket_path: default_socket_path(),
            },
            editor: EditorConfig::default(),
            auto_download_zip: false,
            notifications: NotificationsConfig::default(),
        }
    }
}

impl BraidConfig {
    /// Returns the configuration file path for `ContextBraid`.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration path cannot be constructed.
    pub fn config_path() -> Result<PathBuf> {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
        Ok(base.join("contextbraid").join("config.toml"))
    }

    /// Loads the configuration from disk, creating a default file if absent.
    ///
    /// # Errors
    ///
    /// Returns an error if reading, serializing, or parsing the configuration file fails.
    pub fn load_or_default() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            let cfg = Self::default();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, toml::to_string_pretty(&cfg)?)?;
            return Ok(cfg);
        }
        let raw = fs::read_to_string(path)?;
        Ok(toml::from_str(&raw)?)
    }

    /// Saves the current configuration to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if serializing or writing the configuration file fails.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    #[must_use]
    pub fn resolved_repos_root(&self) -> PathBuf {
        if let Some(stripped) = self.repos.root_dir.strip_prefix("~/")
            && let Some(home) = dirs::home_dir()
        {
            return home.join(stripped);
        }
        PathBuf::from(&self.repos.root_dir)
    }
}

/// Scans `root` for Git repositories while skipping directory names in `ignore`.
///
/// # Errors
///
/// Returns an error if `root` cannot be read.
pub fn scan_repos(root: &Path, ignore: &[String]) -> Result<Vec<(String, PathBuf)>> {
    let mut repos = Vec::new();
    if !root.exists() {
        return Ok(repos);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || ignore.iter().any(|ig| ig == &name) {
            continue;
        }
        repos.push((name, path));
    }
    repos.sort_by_key(|a| a.0.to_lowercase());
    Ok(repos)
}

fn count_whole_word_matches(text: &str, needle: &str) -> usize {
    let bytes = text.as_bytes();
    let n_len = needle.len();
    if n_len == 0 || bytes.len() < n_len {
        return 0;
    }
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    let mut count = 0;
    let mut pos = 0;
    while let Some(found) = text[pos..].find(needle) {
        let start = pos + found;
        let end = start + n_len;
        let left_ok = start == 0 || !is_word(bytes[start - 1]);
        let right_ok = end == bytes.len() || !is_word(bytes[end]);
        if left_ok && right_ok {
            count += 1;
        }
        pos = start + n_len;
    }
    count
}

fn count_leaf_path_hits(text: &str, repo_lower: &str) -> usize {
    text.split_whitespace()
        .filter(|tok| {
            let clean = tok
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '-' && c != '_');
            clean
                .rsplit_once('/')
                .is_some_and(|(_, leaf)| leaf.trim_matches('.') == repo_lower)
        })
        .count()
}

fn score_repo_candidate(tab: &TabContextPayload, repo_name: &str) -> usize {
    let n = repo_name.to_lowercase();
    if n.is_empty() {
        return 0;
    }

    let title_lower = tab.title.to_lowercase();
    let snippet_lower = tab.context_snippet.to_lowercase();
    let full_hay = format!(
        "{} {} {}",
        title_lower,
        snippet_lower,
        tab.url.to_lowercase()
    );

    let title_hits = count_whole_word_matches(&title_lower, &n);
    let total_hits = count_whole_word_matches(&full_hay, &n);
    if total_hits == 0 {
        return 0;
    }

    let mut score = 0;
    let leaf_hits = count_leaf_path_hits(&full_hay, &n);
    if leaf_hits > 0 {
        score += 120 * leaf_hits;
    }
    if title_hits > 0 {
        score += 90 * title_hits;
    }
    let specificity_weight = if n.contains('-') || n.contains('_') {
        35
    } else if n.len() >= 7 {
        20
    } else {
        6
    };

    score += total_hits * specificity_weight;
    score
}

#[must_use]
pub fn match_tab_to_repo<S: std::hash::BuildHasher>(
    tab: &TabContextPayload,
    repos: &[(String, PathBuf)],
    inherited: &HashMap<(String, i64), String, S>,
) -> Option<String> {
    let mut best_repo: Option<(String, usize)> = None;
    for (name, _) in repos {
        let s = score_repo_candidate(tab, name);
        if s > 0 {
            let is_better = match &best_repo {
                None => true,
                Some((best_name, best_score)) => {
                    s > *best_score || (s == *best_score && name.len() > best_name.len())
                }
            };
            if is_better {
                best_repo = Some((name.clone(), s));
            }
        }
    }

    if let Some((repo, _)) = best_repo {
        return Some(repo);
    }

    if let Some(op) = tab.opener_tab_id
        && let Some(parent) = inherited.get(&(tab.browser.clone(), op))
    {
        return Some(parent.clone());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_leaf_priority_and_whole_word_matching() {
        let repos = vec![
            ("cli".into(), PathBuf::from("/repos/cli")),
            ("guiwright".into(), PathBuf::from("/repos/guiwright")),
        ];
        let tab = TabContextPayload {
            browser: "chrome".into(),
            window_id: 1,
            tab_id: 2,
            opener_tab_id: None,
            incognito: false,
            url: "https://chat.z.ai/c/task-123".into(),
            title: "Build guiwright CLI".into(),
            context_snippet: "Path: /Users/dev/repos/guiwright".into(),
            status: AgentStatus::Completed,
            is_focused: false,
            updated_at_ms: 100,
        };
        assert_eq!(
            match_tab_to_repo(&tab, &repos, &HashMap::new()),
            Some("guiwright".into())
        );
    }
}
