mod host;
mod installer;
mod macos;

use anyhow::Result;
use braid_core::{
    match_tab_to_repo, pick_next_alert, scan_repos, AgentStatus, BraidConfig, BrowserAction,
    IpcMessage, ProjectWorkspace, TabContextPayload,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{error, info};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

struct DaemonState {
    config: BraidConfig,
    tabs: HashMap<(String, i64, i64), (TabContextPayload, u64)>,
    tab_repo_history: HashMap<(String, i64), String>,
    pending_actions: HashMap<String, Vec<BrowserAction>>,
    cycle_cursor: usize,
    active_repo: Option<String>,
}

impl DaemonState {
    fn purge_stale(&mut self) {
        let cutoff = now_ms().saturating_sub(20_000);
        self.tabs.retain(|_, (_, seen_at)| *seen_at >= cutoff);
    }

    fn build_dashboard(&mut self) -> Result<Vec<ProjectWorkspace>> {
        self.purge_stale();
        let root = self.config.resolved_repos_root();
        let repos = scan_repos(&root, &self.config.repos.ignore)?;
        let zed_wins = macos::list_zed_windows(&root);
        info!("DASHBOARD_BUILD repos={} zed_wins={:?} active_tabs={}", repos.len(), zed_wins, self.tabs.len());
        let mut grouped: HashMap<String, Vec<TabContextPayload>> = HashMap::new();

        for (tab, _) in self.tabs.values() {
            if let Some(repo) = match_tab_to_repo(tab, &repos, &self.tab_repo_history) {
                self.tab_repo_history
                    .insert((tab.browser.clone(), tab.tab_id), repo.clone());
                grouped.entry(repo).or_default().push(tab.clone());
            }
        }

        let mut projects: Vec<ProjectWorkspace> = repos
            .into_iter()
            .filter_map(|(repo_name, repo_path)| {
                let mut tabs = grouped.remove(&repo_name).unwrap_or_default();
                let has_zed = zed_wins.iter().any(|w| w == &repo_path || w.starts_with(&repo_path));
                if tabs.is_empty() && !has_zed {
                    return None;
                }
                if has_zed {
tabs.insert(0, TabContextPayload {
browser: "zed".into(),
window_id: 0,
tab_id: 0,
opener_tab_id: None,
incognito: false,
url: format!("file://{}", repo_path.display()),
title: format!("Zed — {}", repo_name),
context_snippet: repo_name.clone(),
status: AgentStatus::Idle,
updated_at_ms: 0,
});
}
let has_alert = tabs.iter().any(|t| {
matches!(t.status, AgentStatus::NeedsAttention | AgentStatus::Completed)
});
Some(ProjectWorkspace {
repo_name,
repo_path: repo_path.to_string_lossy().to_string(),
tabs,
has_alert,
})
})
.collect();
projects.sort_by(|a, b| b.has_alert.cmp(&a.has_alert).then(a.repo_name.cmp(&b.repo_name)));
Ok(projects)
}
fn focus_tab(&mut self, tab: &TabContextPayload) -> Result<()> {
macos::activate_app_window(&tab.browser, Some(&tab.title))?;
if tab.browser != "zed" {
self.pending_actions
.entry(tab.browser.clone())
.or_default()
.push(BrowserAction::FocusTab {
window_id: tab.window_id,
tab_id: tab.tab_id,
});
}
Ok(())
}
}
async fn handle_client(stream: UnixStream, state: Arc<RwLock<DaemonState>>) -> Result<()> {
let (reader, mut writer) = stream.into_split();
let mut lines = BufReader::new(reader).lines();
while let Some(line) = lines.next_line().await? {
if line.trim().is_empty() {
continue;
}
let msg: IpcMessage = serde_json::from_str(&line)?;
let resp = match msg {
IpcMessage::SyncTabs { browser, tabs } => {
let mut st = state.write().await;
let now = now_ms();
for t in tabs {
let key = (t.browser.clone(), t.window_id, t.tab_id);
st.tabs.insert(key, (t, now));
}
let cmds = st.pending_actions.remove(&browser).unwrap_or_default();
if cmds.is_empty() {
IpcMessage::Ack
} else {
IpcMessage::BrowserCommand { commands: cmds }
}
}
IpcMessage::GetDashboard => {
let projects = state.write().await.build_dashboard()?;
IpcMessage::DashboardResponse { projects }
}
IpcMessage::FocusTarget { browser, window_id, tab_id } => {
let mut st = state.write().await;
let title = st
.tabs
.get(&(browser.clone(), window_id, tab_id))
.map(|(t, _)| t.title.clone())
.unwrap_or_default();
let dummy = TabContextPayload {
browser,
window_id,
tab_id,
opener_tab_id: None,
incognito: false,
url: String::new(),
title,
context_snippet: String::new(),
status: AgentStatus::Idle,
updated_at_ms: 0,
};
st.focus_tab(&dummy)?;
IpcMessage::Ack
}
IpcMessage::JumpNextAlert => {
let mut st = state.write().await;
let projects = st.build_dashboard()?;
if let Some(target) = pick_next_alert(&projects).cloned() {
info!("JumpNextAlert -> {} ({})", target.title, target.browser);
st.active_repo = projects
.iter()
.find(|p| p.tabs.iter().any(|t| t.tab_id == target.tab_id))
.map(|p| p.repo_name.clone());
st.focus_tab(&target)?;
}
IpcMessage::Ack
}
IpcMessage::CycleProject { repo_name } => {
                let mut st = state.write().await;
                let projects = st.build_dashboard()?;

                let front_repo = macos::get_frontmost_window().and_then(|(app, win_title)| {
                    let wt = win_title.to_lowercase();
                    projects
                        .iter()
                        .find(|p| wt.contains(&p.repo_name.to_lowercase()))
                        .map(|p| p.repo_name.clone())
                        .or_else(|| {
                            let b_key = if app.contains("brave") {
                                "brave"
                            } else if app.contains("chrome") {
                                "chrome"
                            } else if app.contains("edge") {
                                "edge"
                            } else if app.contains("firefox") {
                                "firefox"
                            } else {
                                ""
                            };
                            if b_key.is_empty() {
                                None
                            } else {
                                projects
                                    .iter()
                                    .find(|p| {
                                        p.tabs.iter().any(|t| {
                                            t.browser == b_key
                                                && !t.title.is_empty()
                                                && wt.contains(&t.title.to_lowercase())
                                        })
                                    })
                                    .map(|p| p.repo_name.clone())
                            }
                        })
                });

                let target_repo = repo_name
                    .or(front_repo)
                    .or_else(|| st.active_repo.clone());

                let proj = target_repo
                    .and_then(|r| projects.iter().find(|p| p.repo_name == r))
                    .or_else(|| projects.first());

                if let Some(p) = proj {
                    if st.active_repo.as_deref() != Some(&p.repo_name) {
                        st.active_repo = Some(p.repo_name.clone());
                        st.cycle_cursor = 1 % p.tabs.len().max(1);
                    }
                    let idx = st.cycle_cursor % p.tabs.len();
                    st.cycle_cursor = st.cycle_cursor.wrapping_add(1);
                    let target = p.tabs[idx].clone();
                    info!("CycleProject [{}] -> {} ({})", p.repo_name, target.title, target.browser);
                    st.focus_tab(&target)?;
                }
                IpcMessage::Ack
            }
_ => IpcMessage::Ack,
};
let mut out = serde_json::to_string(&resp)?;
out.push('\n');
writer.write_all(out.as_bytes()).await?;
}
Ok(())
}
async fn run_daemon() -> Result<()> {
tracing_subscriber::fmt()
.with_writer(std::io::stderr)
.with_env_filter("info")
.init();
let config = BraidConfig::load_or_default()?;
let sock_path = config.daemon.socket_path.clone();
if Path::new(&sock_path).exists() {
std::fs::remove_file(&sock_path)?;
}
let listener = UnixListener::bind(&sock_path)?;
info!("braid daemon prêt sur {}", sock_path);
let state = Arc::new(RwLock::new(DaemonState {
config,
tabs: HashMap::new(),
tab_repo_history: HashMap::new(),
pending_actions: HashMap::new(),
cycle_cursor: 0,
active_repo: None,
}));
loop {
let (stream, _) = listener.accept().await?;
let st = Arc::clone(&state);
tokio::spawn(async move {
if let Err(e) = handle_client(stream, st).await {
error!("Erreur IPC: {:?}", e);
}
});
}
}
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let invoked_as_host = args
        .first()
        .map(|a| a.ends_with("braid-host"))
        .unwrap_or(false);
    if invoked_as_host {
        host::run_host_entry();
        return Ok(());
    }
    match args.get(1).map(|s| s.as_str()) {
        Some("host") => {
            host::run_host_entry();
            Ok(())
        }
        Some("install") => {
            let extra: Vec<String> = args.iter().skip(2).cloned().collect();
            installer::install_self(&extra)
        }
        Some("daemon") | None => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_daemon())
        }
        Some(_other) => {
            eprintln!("Usage: braid [daemon|host|install <optional_ext_id>]");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
use super::host::{read_frame, write_frame};
use std::io::Cursor;
#[test]
fn test_u32_framing_roundtrip() {
let payload = r#"{"type":"get_dashboard"}"#;
let mut wire = Vec::new();
write_frame(&mut wire, payload).unwrap();
let out = read_frame(&mut Cursor::new(wire)).unwrap().unwrap();
assert_eq!(out, payload);
}
}
