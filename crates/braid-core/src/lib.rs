pub mod ipc;
pub use ipc::*;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BraidConfig {
    pub repos: ReposConfig,
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub roles: Vec<RoleConfig>,
    pub shortcuts: ShortcutsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReposConfig {
    pub root: String,
    #[serde(default)]
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub socket_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub url_patterns: Vec<String>,
    pub app_name: Option<String>,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutsConfig {
    pub cycle_project_windows: String,
    pub open_dashboard: String,
    pub jump_next_alert: String,
}

impl Default for BraidConfig {
    fn default() -> Self {
        Self {
            repos: ReposConfig {
                root: "~/Documents/Repos".to_string(),
                ignore: vec!["node_modules".into(), "target".into(), ".git".into()],
            },
            daemon: DaemonConfig { socket_path: "/tmp/braid.sock".into() },
            roles: vec![
                RoleConfig {
                    id: "executor".into(),
                    label: "Agent Long (z.ai)".into(),
                    url_patterns: vec!["*://*.z.ai/*".into()],
                    app_name: None,
                    icon: "robot".into(),
                },
                RoleConfig {
                    id: "architect".into(),
                    label: "Prompt Builder (Qwen)".into(),
                    url_patterns: vec!["*://chat.qwen.ai/*".into(), "*://qwen.ai/*".into()],
                    app_name: None,
                    icon: "sparkles".into(),
                },
                RoleConfig {
                    id: "scripter".into(),
                    label: "Live Scripting (Gemini)".into(),
                    url_patterns: vec!["*://://google.com*".into()],
                    app_name: None,
                    icon: "bolt".into(),
                },
            ],
            shortcuts: ShortcutsConfig {
                cycle_project_windows: "Option+Tab".into(),
                open_dashboard: "Option+Space".into(),
                jump_next_alert: "Option+A".into(),
            },
        }
    }
}

impl BraidConfig {
    pub fn load_or_default() -> Result<Self> {
        let p = if let Ok(env_p) = std::env::var("BRAID_CONFIG_PATH") {
            PathBuf::from(env_p)
        } else {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("~/.config"))
                .join("contextbraid/config.toml")
        };
        if !p.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&p).with_context(|| format!("Erreur lecture {:?}", p))?;
        Ok(toml::from_str(&raw)?)
    }

    pub fn resolved_repos_root(&self) -> PathBuf {
        if let Some(s) = self.repos.root.strip_prefix("~/") {
            if let Some(h) = dirs::home_dir() {
                return h.join(s);
            }
        }
        PathBuf::from(&self.repos.root)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    NeedsAttention,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabContextPayload {
    pub browser: String,
    pub window_id: i64,
    pub tab_id: i64,
    pub opener_tab_id: Option<i64>,
    pub incognito: bool,
    pub url: String,
    pub title: String,
    pub context_snippet: String,
    pub status: AgentStatus,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectWorkspace {
    pub repo_name: String,
    pub repo_path: String,
    pub tabs: Vec<TabContextPayload>,
    pub has_alert: bool,
}

fn is_project_dir(path: &Path) -> bool {
    const MARKERS: &[&str] = &[
        ".git",
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "flake.nix",
        "justfile",
        "Makefile",
    ];
    MARKERS.iter().any(|m| path.join(m).exists())
}

pub fn scan_repos(root: &Path, ignore: &[String]) -> Result<Vec<(String, PathBuf)>> {
    let mut repos = Vec::new();
    if !root.exists() {
        return Ok(repos);
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() && is_project_dir(&path) {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if !name.starts_with('.') && !ignore.iter().any(|ig| ig == name) {
                    repos.push((name.to_string(), path));
                }
            }
        }
    }
    repos.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(repos)
}

pub fn count_whole_word_matches(haystack: &str, needle: &str) -> usize {
    let n = needle.to_lowercase();
    if n.is_empty() {
        return 0;
    }
    haystack
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .filter(|token| *token == n)
        .count()
}

fn count_leaf_path_hits(text: &str, repo_lower: &str) -> usize {
    text.split_whitespace()
        .filter(|tok| {
            let clean = tok.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '-' && c != '_');
            clean
                .rsplit_once('/')
                .map(|(_, leaf)| leaf.trim_matches('.') == repo_lower)
                .unwrap_or(false)
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
    let full_hay = format!("{} {} {}", title_lower, snippet_lower, tab.url.to_lowercase());

    let title_hits = count_whole_word_matches(&title_lower, &n);
    let total_hits = count_whole_word_matches(&full_hay, &n);
    if total_hits == 0 {
        return 0;
    }

    let mut score = 0;

    // 1. Chemin explicite "owner/repo" : seul le segment final (feuille) est crédité, jamais le owner
    let leaf_hits = count_leaf_path_hits(&full_hay, &n);
    if leaf_hits > 0 {
        score += 120 * leaf_hits;
    }

    // 2. Correspondance directe dans le titre de l'onglet (ex: titre "Alacer" ou "Glyim")
    if title_hits > 0 {
        score += 90 * title_hits;
    }

    // 3. Poids de spécificité lexicale dans le reste du contexte
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

pub fn match_tab_to_repo(
    tab: &TabContextPayload,
    repos: &[(String, PathBuf)],
    inherited: &HashMap<(String, i64), String>,
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

    if let Some(op) = tab.opener_tab_id {
        if let Some(parent) = inherited.get(&(tab.browser.clone(), op)) {
            return Some(parent.clone());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_repo_leaf_priority_and_whole_word_matching() {
        let dir = tempdir().unwrap();
        let user_dir = dir.path().join("elcoosp");
        let repo_dir = dir.path().join("guiwright");
        let short_dir = dir.path().join("zsx");
        for d in [&user_dir, &repo_dir, &short_dir] {
            fs::create_dir(d).unwrap();
            fs::create_dir(d.join(".git")).unwrap();
        }

        let repos = scan_repos(dir.path(), &[]).unwrap();
        let inherited = HashMap::new();

        let tab = TabContextPayload {
            browser: "chrome".into(),
            window_id: 1,
            tab_id: 10,
            opener_tab_id: None,
            incognito: true,
            url: "https://z.ai".into(),
            title: "Deliver Ship.toml & ZIP for GU... | Z.ai".into(),
            context_snippet: "Task: elcoosp/guiwright. Deliver zip.".into(),
            status: AgentStatus::Completed,
            updated_at_ms: 1000,
        };

        assert_eq!(match_tab_to_repo(&tab, &repos, &inherited).as_deref(), Some("guiwright"));

        let unrelated = TabContextPayload {
            context_snippet: "zero size execution test".into(),
            title: "Unrelated".into(),
            ..tab
        };
        assert_eq!(match_tab_to_repo(&unrelated, &repos, &inherited), None);
    }
}
