mod host;
mod installer;
mod chart_png;
mod macos;

use anyhow::Result;
use braid_core::{
    match_tab_to_repo, scan_repos, AgentStatus, BraidConfig, IpcMessage,
    ProjectWorkspace, TabContextPayload,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info};

struct DaemonState {
    config: BraidConfig,
    tabs: HashMap<(String, i64, i64), (TabContextPayload, u64)>,
    tab_repo_history: HashMap<(String, i64), String>,
    active_repo: Option<String>,
    last_focused_tab: Option<(String, i64, i64)>,
    cycle_cursor: usize,
    notified_alerts: HashSet<(String, i64, i64, u8, u64)>,
    seen_tabs: HashSet<(String, i64, i64)>,
    push_tx: broadcast::Sender<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl DaemonState {
    fn new(config: BraidConfig, push_tx: broadcast::Sender<String>) -> Self {
        Self {
            config,
            tabs: HashMap::new(),
            tab_repo_history: HashMap::new(),
            active_repo: None,
            last_focused_tab: None,
            cycle_cursor: 0,
            notified_alerts: HashSet::new(),
            seen_tabs: HashSet::new(),
            push_tx,
        }
    }

    fn build_dashboard(&mut self) -> Result<Vec<ProjectWorkspace>> {
        let cutoff = now_ms().saturating_sub(180_000);
        self.tabs.retain(|_, (_, ts)| *ts >= cutoff);

        let root = self.config.resolved_repos_root();
        let repos = scan_repos(&root, &self.config.repos.ignore)?;
        let editor_spec = macos::resolve_editor_spec(&self.config.editor.favorite);
        let editor_wins = macos::list_editor_windows(&root, &self.config.editor.favorite);

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
                tabs.sort_by(|a, b| {
                    a.browser
                        .cmp(&b.browser)
                        .then(a.window_id.cmp(&b.window_id))
                        .then(a.tab_id.cmp(&b.tab_id))
                });

                let has_editor = editor_wins
                    .iter()
                    .any(|w| w == &repo_path || w.starts_with(&repo_path));
                if tabs.is_empty() && !has_editor {
                    return None;
                }
                if has_editor {
                    tabs.insert(
                        0,
                        TabContextPayload {
                            browser: editor_spec.id.into(),
                            window_id: 0,
                            tab_id: 0,
                            opener_tab_id: None,
                            incognito: false,
                            url: format!("file://{}", repo_path.display()),
                            title: format!("{} — {}", editor_spec.label, repo_name),
                            context_snippet: repo_name.clone(),
                            status: AgentStatus::Idle,
                            updated_at_ms: 0,
                        },
                    );
                }

                let has_alert = tabs.iter().any(|t| {
                    matches!(
                        t.status,
                        AgentStatus::NeedsAttention | AgentStatus::Completed
                    )
                });
                let git_meta = macos::git_repo_stats(&repo_path);
                Some(ProjectWorkspace {
                    repo_name,
                    repo_path: repo_path.to_string_lossy().into_owned(),
                    git_branch: git_meta.branch,
                    dirty_files: git_meta.dirty,
                    favorite_editor: editor_spec.id.into(),
                    last_commit_sha: git_meta.commit_sha,
                    last_commit_msg: git_meta.commit_msg,
                    last_commit_ts: git_meta.commit_ts,
                    commit_sparkline: git_meta.sparkline,
                    sparkline_png_path: git_meta.sparkline_png_path,
                    repo_url: git_meta.repo_url,
                    recent_prs: git_meta.recent_prs,
                    tabs,
                    has_alert,
                })
            })
            .collect();

        projects.sort_by(|a, b| {
            b.has_alert
                .cmp(&a.has_alert)
                .then_with(|| a.repo_name.cmp(&b.repo_name))
        });
        Ok(projects)
    }

    fn focus_tab(&mut self, tab: &TabContextPayload) -> Result<()> {
        let is_editor = matches!(
            tab.browser.as_str(),
            "zed" | "cursor" | "vscode" | "windsurf" | "neovim" | "editor"
        );
        let receivers = self.push_tx.receiver_count();
        tracing::info!(
            is_editor = is_editor,
            browser = %tab.browser,
            window_id = tab.window_id,
            tab_id = tab.tab_id,
            title = %tab.title,
            host_receivers = receivers,
            "CYCLE_E2E_FOCUS_TAB_ENTER"
        );
        if is_editor {
            let res = macos::focus_specific_editor_window(&self.config.editor.favorite, &tab.title);
            tracing::info!(result = ?res, "CYCLE_E2E_FOCUS_EDITOR_DONE");
            res?;
        } else {
            macos::activate_browser_app(&tab.browser);
            let cmd = serde_json::json!({
                "type": "browser_command",
                "commands": [{
                    "action": "focus_tab",
                    "window_id": tab.window_id,
                    "tab_id": tab.tab_id,
                }]
            });
            if let Ok(json) = serde_json::to_string(&cmd) {
                let sent = self.push_tx.send(json.clone());
                tracing::info!(sent = ?sent, payload = %json, "CYCLE_E2E_PUSH_BROWSER_CMD");
            }
        }
        self.last_focused_tab = Some((tab.browser.clone(), tab.window_id, tab.tab_id));
        Ok(())
    }
}

async fn handle_client(stream: UnixStream, state: Arc<RwLock<DaemonState>>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let msg: IpcMessage = match serde_json::from_str(trimmed) {
            Ok(m) => m,
            Err(e) => {
                error!("JSON IPC invalide: {}", e);
                continue;
            }
        };

        if let IpcMessage::SubscribeHost { .. } = msg {
            let (mut rx, auto_dl) = {
                let st = state.read().await;
                (st.push_tx.subscribe(), st.config.auto_download_zip)
            };
            let cfg_msg = serde_json::json!({
                "type": "sync_config",
                "auto_download_zip": auto_dl,
            });
            if let Ok(cfg_line) = serde_json::to_string(&cfg_msg) {
                let _ = writer.write_all(cfg_line.as_bytes()).await;
                let _ = writer.write_all(b"\n").await;
                let _ = writer.flush().await;
            }
            while let Ok(payload) = rx.recv().await {
                if writer.write_all(payload.as_bytes()).await.is_err()
                    || writer.write_all(b"\n").await.is_err()
                    || writer.flush().await.is_err()
                {
                    break;
                }
}
return Ok(());
}
let response = match msg {
IpcMessage::NotifyTabFocused {
                browser,
                window_id,
                tab_id,
            } => {
                let mut st = state.write().await;
                tracing::info!(
    browser = %browser,
    window_id = window_id,
    tab_id = tab_id,
    prev_active_repo = ?st.active_repo,
    mapped_repo = ?st.tab_repo_history.get(&(browser.clone(), tab_id)),
    "CYCLE_INSTR_TAB_FOCUSED"
);
st.last_focused_tab = Some((browser.clone(), window_id, tab_id));
                if let Some(repo) = st.tab_repo_history.get(&(browser, tab_id)).cloned()
                    && st.active_repo.as_deref() != Some(&repo) {
                        st.active_repo = Some(repo);
                        st.cycle_cursor = 0;
                    }
                IpcMessage::Ack
            }
IpcMessage::SyncTabs { tabs, .. } => {
                let mut st = state.write().await;
                let now = now_ms();
                let root = st.config.resolved_repos_root();
                let repos = scan_repos(&root, &st.config.repos.ignore).unwrap_or_default();
                for t in tabs {
                    let key = (t.browser.clone(), t.window_id, t.tab_id);
                    let alert_kind = match t.status {
                        AgentStatus::NeedsAttention => Some((1u8, format!("Alerte sur {}", t.browser))),
                        AgentStatus::Completed => Some((2u8, format!("ZIP prêt sur {}", t.browser))),
                        _ => None,
                    };
                    let is_first_seen = st.seen_tabs.insert(key.clone());
                    if let Some((kind, subtitle)) = alert_kind {
                        use std::hash::{Hash, Hasher};
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        if kind == 2 {
                            t.title.hash(&mut hasher);
                            t.context_snippet.hash(&mut hasher);
                        }
                        let sig = hasher.finish();
                        let alert_key = (t.browser.clone(), t.window_id, t.tab_id, kind, sig);
                        if st.notified_alerts.insert(alert_key) && !is_first_seen {
                            let repo_label = match_tab_to_repo(&t, &repos, &st.tab_repo_history)
                                .or_else(|| st.tab_repo_history.get(&(t.browser.clone(), t.tab_id)).cloned())
                                .unwrap_or_else(|| "Workspace".into());
                            macos::notify_agent_alert(&repo_label, &t.browser, t.window_id, t.tab_id, &subtitle, &t.title);
                        }
                    } else {
                        st.notified_alerts.retain(|(b, w, tid, _, _)| !(b == &t.browser && *w == t.window_id && *tid == t.tab_id));
                    }
                    st.tabs.insert(key, (t, now));
                }
                IpcMessage::Ack
            }
            IpcMessage::GetDashboard => {
let mut st = state.write().await;
let projects = st.build_dashboard()?;
IpcMessage::DashboardResponse { projects }
}
IpcMessage::FocusTarget {
browser,
window_id,
tab_id,
} => {
let mut st = state.write().await;
let projects = st.build_dashboard()?;
for p in &projects {
if let Some(t) = p
.tabs
.iter()
.find(|t| t.browser == browser && t.window_id == window_id && t.tab_id == tab_id)
{
st.active_repo = Some(p.repo_name.clone());
if let Some(pos) = p.tabs.iter().position(|x| x.browser == browser && x.window_id == window_id && x.tab_id == tab_id) {
st.cycle_cursor = pos + 1;
}
st.focus_tab(t)?;
break;
}
}
IpcMessage::Ack
}
IpcMessage::OpenEditor { repo_path } => {
                let fav = state.read().await.config.editor.favorite.clone();
                macos::open_in_favorite_editor(&fav, &repo_path)?;
                IpcMessage::Ack
            }
            IpcMessage::OpenTerminal { repo_path } => {
                macos::open_in_terminal(&repo_path)?;
                IpcMessage::Ack
            }
IpcMessage::JumpNextAlert => {
let mut st = state.write().await;
let projects = st.build_dashboard()?;
if let Some((proj_name, tab)) = projects.iter().find_map(|p| {
p.tabs
.iter()
.find(|t| {
matches!(
t.status,
AgentStatus::NeedsAttention | AgentStatus::Completed
)
})
.map(|t| (p.repo_name.clone(), t.clone()))
}) {
st.active_repo = Some(proj_name);
st.focus_tab(&tab)?;
}
IpcMessage::Ack
}
IpcMessage::CycleProject { repo_name } => {
                let mut st = state.write().await;
                let projects = st.build_dashboard()?;
                let (front_owner, cg_win_title) = macos::frontmost_app_and_window();
                let editor_spec = macos::resolve_editor_spec(&st.config.editor.favorite);
                let owner_low = front_owner.to_lowercase();
                let is_editor_front = owner_low.contains(&editor_spec.label.to_lowercase())
                    || owner_low.contains(&editor_spec.cli_app.to_lowercase());

                // Titre fenêtre : CoreGraphics si disponible, sinon Accessibility AXUIElement pour l'éditeur (< 2ms)
                let front_win_title = cg_win_title;

                tracing::info!(
                    arg_repo_name = ?repo_name,
                    front_owner = %front_owner,
                    front_win_title = %front_win_title,
                    is_editor_front = is_editor_front,
                    last_focused_tab = ?st.last_focused_tab,
                    stored_active_repo = ?st.active_repo,
                    "CYCLE_PROJECT_REQUEST"
                );
                for p in &projects {
                    let surfaces: Vec<String> = p
                        .tabs
                        .iter()
                        .map(|t| format!("{}[w={},t={},title={:?}]", t.browser, t.window_id, t.tab_id, t.title))
                        .collect();
                    tracing::info!(repo = %p.repo_name, surfaces = ?surfaces, "CYCLE_E2E_DASHBOARD_REPO");
                }

                // 1. Si l'éditeur est au premier plan, résout le repo via le titre AX de la fenêtre éditeur
                let win_lower = front_win_title.to_lowercase();
                let by_editor_title = if is_editor_front && !win_lower.is_empty() {
                    projects
                        .iter()
                        .filter(|p| {
                            let rn = p.repo_name.to_lowercase();
                            rn.len() >= 2 && win_lower.contains(&rn)
                        })
                        .max_by_key(|p| p.repo_name.len())
                        .map(|p| p.repo_name.clone())
                } else {
                    None
                };

                // 2. Si un navigateur est au premier plan, résout le repo via l'onglet actif réel (`last_focused_tab`)
                let by_browser_active_tab = if !is_editor_front {
                    st.last_focused_tab.as_ref().and_then(|(b, w, tid)| {
                        projects
                            .iter()
                            .find(|p| {
                                p.tabs.iter().any(|t| {
                                    t.browser.eq_ignore_ascii_case(b)
                                        && t.window_id == *w
                                        && t.tab_id == *tid
                                })
                            })
                            .map(|p| p.repo_name.clone())
                    })
                } else {
                    None
                };

                let target_repo = repo_name
                    .or(by_editor_title)
                    .or(by_browser_active_tab)
                    .or_else(|| st.active_repo.clone());

                let proj = target_repo
                    .as_deref()
                    .and_then(|r| projects.iter().find(|p| p.repo_name == r))
                    .or_else(|| projects.first());

                if let Some(p) = proj {
                    if st.active_repo.as_deref() != Some(&p.repo_name) {
                        st.active_repo = Some(p.repo_name.clone());
                        st.cycle_cursor = 0;
                    }

                    let len = p.tabs.len();
                    let mut idx = st.cycle_cursor % len;
                    let candidate = &p.tabs[idx];
                    let candidate_is_editor = matches!(
                        candidate.browser.as_str(),
                        "zed" | "cursor" | "vscode" | "windsurf" | "neovim" | "editor"
                    );

                    // Saute la surface qui est déjà au premier plan
                    if len > 1 && is_editor_front && candidate_is_editor {
                        idx = (idx + 1) % len;
                    } else if len > 1 && !is_editor_front && !candidate_is_editor
                        && let Some((fb, fw, ftid)) = &st.last_focused_tab
                            && candidate.browser.eq_ignore_ascii_case(fb)
                                && candidate.window_id == *fw
                                && candidate.tab_id == *ftid
                            {
                                idx = (idx + 1) % len;
                            }

                    st.cycle_cursor = (idx + 1) % len;
                    let target = p.tabs[idx].clone();
                    tracing::info!(
                        repo = %p.repo_name,
                        step = format!("{}/{}", idx + 1, len),
                        target_browser = %target.browser,
                        target_win = target.window_id,
                        target_tab = target.tab_id,
                        target_title = %target.title,
                        "CYCLE_PROJECT_DISPATCH"
                    );
                    st.focus_tab(&target)?;
                }
                IpcMessage::Ack
            }
            IpcMessage::SubscribeHost { .. } => unreachable!(),
_ => IpcMessage::Ack,
};
let out = serde_json::to_string(&response)?;
writer.write_all(out.as_bytes()).await?;
writer.write_all(b"\n").await?;
writer.flush().await?;
}
Ok(())
}
async fn run_daemon() -> Result<()> {
tracing_subscriber::fmt::init();
let config = BraidConfig::load_or_default()?;
let sock_path = Path::new(&config.daemon.socket_path);
if sock_path.exists() {
let _ = fs::remove_file(sock_path);
}
let listener = UnixListener::bind(sock_path)?;
let (push_tx, _) = broadcast::channel(64);
info!("braid daemon prêt sur {}", sock_path.display());
let state = Arc::new(RwLock::new(DaemonState::new(config, push_tx)));
loop {
let (stream, _) = listener.accept().await?;
let st = Arc::clone(&state);
tokio::spawn(async move {
if let Err(e) = handle_client(stream, st).await {
error!("Erreur IPC: {}", e);
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
Some("notify") => {
let repo = args.get(2).map(|s| s.as_str()).unwrap_or("ContextBraid");
let subtitle = args.get(3).map(|s| s.as_str()).unwrap_or("Agent Alert");
let message = args.get(4).map(|s| s.as_str()).unwrap_or("");
macos::deliver_un_notification_gui(repo, subtitle, message);
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
    use super::*;
use super::host::{read_frame, write_frame};
use std::io::Cursor;
#[test]
fn test_u32_framing_roundtrip() {
let mut buf = Vec::new();
let sample = r#"{"type":"get_dashboard"}"#;
write_frame(&mut buf, sample).unwrap();
let mut cur = Cursor::new(buf);
let decoded = read_frame(&mut cur).unwrap().unwrap();
assert_eq!(decoded, sample);
}

    #[test]
    fn test_alert_transition_dedup_and_rearm() {
        let _ = crate::macos::take_test_notifications();
        let (push_tx, _) = broadcast::channel(8);
        let mut st = DaemonState::new(BraidConfig::default(), push_tx);

        let make_tab = |status: AgentStatus| TabContextPayload {
            browser: "Brave".into(),
            window_id: 101,
            tab_id: 202,
            opener_tab_id: None,
            incognito: false,
            url: "https://z.ai".into(),
            title: "Task: context-braid".into(),
            context_snippet: "Running task".into(),
            status,
            updated_at_ms: now_ms(),
        };

        let apply_tab = |st: &mut DaemonState, tab: TabContextPayload| {
            let raw = serde_json::json!({
                "type": "sync_tabs",
                "browser": tab.browser,
                "tabs": [tab]
            })
            .to_string();
            let parsed: IpcMessage = serde_json::from_str(&raw).expect("valid sync_tabs JSON");
            if let IpcMessage::SyncTabs { tabs, .. } = parsed {
                let now = now_ms();
                for t in tabs {
                    let key = (t.browser.clone(), t.window_id, t.tab_id);
                    let alert_kind = match t.status {
                        AgentStatus::NeedsAttention => Some((1u8, format!("Alerte sur {}", t.browser))),
                        AgentStatus::Completed => Some((2u8, format!("ZIP prêt sur {}", t.browser))),
                        _ => None,
                    };
                    let is_first_seen = st.seen_tabs.insert(key.clone());
                    if let Some((kind, subtitle)) = alert_kind {
                        let alert_key = (t.browser.clone(), t.window_id, t.tab_id, kind, 0);
                        if st.notified_alerts.insert(alert_key) && !is_first_seen {
                            crate::macos::notify_agent_alert("context-braid", &t.browser, t.window_id, t.tab_id, &subtitle, &t.title);
                        }
                    } else {
                        st.notified_alerts.retain(|(b, w, tid, _, _)| !(b == &t.browser && *w == t.window_id && *tid == t.tab_id));
                    }
                    st.tabs.insert(key, (t, now));
                }
            }
        };

        // 0. Découverte initiale d'un onglet déjà Completed (F5 / démarrage daemon) -> 0 notification
        let mut stale_tab = make_tab(AgentStatus::Completed);
        stale_tab.tab_id = 999;
        apply_tab(&mut st, stale_tab);
        assert!(crate::macos::take_test_notifications().is_empty());

        // 1. Working -> aucune notification
        apply_tab(&mut st, make_tab(AgentStatus::Working));
        assert!(crate::macos::take_test_notifications().is_empty());

        // 2. Transition vers NeedsAttention -> 1 notification déclenchée
        apply_tab(&mut st, make_tab(AgentStatus::NeedsAttention));
        let first = crate::macos::take_test_notifications();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].1, "Alerte sur Brave");

        // 3. Poll suivant toujours en NeedsAttention -> dédupliqué (0 nouvelle notif)
        apply_tab(&mut st, make_tab(AgentStatus::NeedsAttention));
        assert!(crate::macos::take_test_notifications().is_empty());

        // 4. Reprise de la conversation (Working) -> réarme les alertes
        apply_tab(&mut st, make_tab(AgentStatus::Working));
        assert!(crate::macos::take_test_notifications().is_empty());

        // 5. Transition vers Completed (ZIP Ready) -> 1 notification ZIP déclenchée
        apply_tab(&mut st, make_tab(AgentStatus::Completed));
        let second = crate::macos::take_test_notifications();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].1, "ZIP prêt sur Brave");
    }

    #[test]
    fn test_aqua_session_and_lsui_element_in_installer() {
        let installer_src = include_str!("installer.rs");
        assert!(installer_src.contains("LimitLoadToSessionType"));
        assert!(installer_src.contains("<string>Aqua</string>"));
        assert!(installer_src.contains("<key>LSUIElement</key>"));
    }
}
