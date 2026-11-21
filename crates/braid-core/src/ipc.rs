use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    #[serde(default)]
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
    #[serde(default)]
    pub git_branch: String,
    #[serde(default)]
    pub dirty_files: usize,
    #[serde(default)]
    pub favorite_editor: String,
    pub tabs: Vec<TabContextPayload>,
    pub has_alert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCommand {
    pub action: String,
    pub window_id: i64,
    pub tab_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcMessage {
    SyncTabs {
        browser: String,
        tabs: Vec<TabContextPayload>,
    },
    GetDashboard,
    SubscribeHost {
        browser: String,
    },
    NotifyTabFocused {
        browser: String,
        window_id: i64,
        tab_id: i64,
    },
    DashboardResponse {
        projects: Vec<ProjectWorkspace>,
    },
    FocusTarget {
        browser: String,
        window_id: i64,
        tab_id: i64,
    },
    OpenEditor {
        repo_path: String,
    },
    OpenTerminal {
        repo_path: String,
    },
    JumpNextAlert,
    CycleProject {
        repo_name: Option<String>,
    },
    BrowserCommand {
        commands: Vec<BrowserCommand>,
    },
    Ack,
}
