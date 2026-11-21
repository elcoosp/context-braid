use crate::{AgentStatus, ProjectWorkspace, TabContextPayload};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcMessage {
    SyncTabs { browser: String, tabs: Vec<TabContextPayload> },
    GetDashboard,
    SubscribeHost { browser: String },
    NotifyTabFocused { browser: String, window_id: i64, tab_id: i64 },
    DashboardResponse { projects: Vec<ProjectWorkspace> },
    FocusTarget { browser: String, window_id: i64, tab_id: i64 },
    JumpNextAlert,
    CycleProject { repo_name: Option<String> },
    BrowserCommand { commands: Vec<BrowserAction> },
    Ack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BrowserAction {
    FocusTab { window_id: i64, tab_id: i64 },
}

pub fn pick_next_alert(projects: &[ProjectWorkspace]) -> Option<&TabContextPayload> {
    projects
        .iter()
        .flat_map(|p| p.tabs.iter())
        .find(|t| t.status == AgentStatus::NeedsAttention)
        .or_else(|| {
            projects
                .iter()
                .flat_map(|p| p.tabs.iter())
                .find(|t| t.status == AgentStatus::Completed)
        })
}
