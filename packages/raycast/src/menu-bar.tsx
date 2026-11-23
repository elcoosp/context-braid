import {
  Color,
  Icon,
  LaunchType,
  MenuBarExtra,
  getPreferenceValues,
  launchCommand,
  open,
  openExtensionPreferences,
  showHUD,
  Keyboard,
} from "@raycast/api";
import { useEffect, useState } from "react";
import { i18n, initI18n } from "./i18n";
import { ProjectWorkspace, TabContext, queryDaemon } from "./socketClient";

initI18n();

interface MenuBarPrefs {
  notifyWhenFocused?: boolean;
}

function truncate(str: string, max = 38): string {
  const clean = str.replace(/\s+/g, " ").trim();
  if (clean.length <= max) return clean;
  return `${clean.slice(0, max - 1).trimEnd()}…`;
}

function getTabPlatform(tab: TabContext): string | null {
  const u = (tab.url || "").toLowerCase();
  if (u.includes("chat.z.ai")) return "Z.ai";
  if (u.includes("chat.qwen.ai")) return "Qwen";
  if (u.includes("://google.com")) return "Gemini";
  if (u.includes("deepseek.com")) return "DeepSeek";
  if (u.includes("://google.com") || u.includes("udm=50")) return "Google AI";
  return null;
}

function formatTabTitle(tab: TabContext): string {
  const browser =
    tab.browser.charAt(0).toUpperCase() + tab.browser.slice(1).toLowerCase();
  const rawTitle = (tab.title || tab.url || "")
    .replace(
      /\s*[-–|]\s*(Google Search|Recherche Google|ChatGPT|Claude|Gemini|DeepSeek|Z\.ai).*$/i,
      "",
    )
    .trim();

  const platform = getTabPlatform(tab);
  if (platform) {
    const summary = rawTitle ? ` — ${truncate(rawTitle, 26)}` : "";
    return `${platform}${summary}`;
  }
  return `${browser} • ${truncate(rawTitle || tab.url, 32)}`;
}

function getTabStatusIcon(tab: TabContext): {
  source: Icon;
  tintColor: Color;
} {
  if (tab.status === "needs_attention") {
    return { source: Icon.ExclamationMark, tintColor: Color.Red };
  }
  if (tab.status === "working") {
    return { source: Icon.Bolt, tintColor: Color.Orange };
  }
  if (tab.status === "completed") {
    return { source: Icon.CheckCircle, tintColor: Color.Green };
  }
  if ((tab as TabContext & { is_focused?: boolean }).is_focused) {
    return { source: Icon.Eye, tintColor: Color.Blue };
  }
  return { source: Icon.Globe, tintColor: Color.SecondaryText };
}

function getProjectIcon(proj: ProjectWorkspace): {
  source: Icon;
  tintColor: Color;
} {
  const hasWorking = proj.tabs.some(
    (tab: TabContext) => tab.status === "working",
  );
  const hasReady = proj.tabs.some(
    (tab: TabContext) => tab.status === "completed",
  );

  if (proj.has_alert) {
    return { source: Icon.ExclamationMark, tintColor: Color.Red };
  }
  if (hasWorking) {
    return { source: Icon.Bolt, tintColor: Color.Orange };
  }
  if (hasReady) {
    return { source: Icon.CheckCircle, tintColor: Color.Green };
  }
  if (proj.dirty_files > 0) {
    return { source: Icon.Pencil, tintColor: Color.Yellow };
  }
  return { source: Icon.Folder, tintColor: Color.SecondaryText };
}

function formatSubmenuTitle(proj: ProjectWorkspace): string {
  return proj.dirty_files > 0
    ? `${proj.repo_name} (${proj.dirty_files})`
    : proj.repo_name;
}

function isProjectActive(proj: ProjectWorkspace): boolean {
  return (
    proj.has_alert ||
    proj.dirty_files > 0 ||
    proj.tabs.some((tab: TabContext) => getTabPlatform(tab) !== null)
  );
}

export default function WorkspaceMenuBar() {
  const prefs = getPreferenceValues<MenuBarPrefs>();
  const [projects, setProjects] = useState<ProjectWorkspace[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isOffline, setIsOffline] = useState(false);

  useEffect(() => {
    let mounted = true;
    queryDaemon<{ projects?: ProjectWorkspace[] }>({ type: "get_dashboard" })
      .then((res) => {
        if (!mounted) return;
        if (Array.isArray(res?.projects)) {
          setProjects(res.projects);
          setIsOffline(false);
        } else {
          setIsOffline(true);
        }
      })
      .catch(() => {
        if (mounted) setIsOffline(true);
      })
      .finally(() => {
        if (mounted) setIsLoading(false);
      });
    return () => {
      mounted = false;
    };
  }, []);

  const activeProjects = projects.filter(isProjectActive);
  const idleProjects = projects.filter((p) => !isProjectActive(p));
  const alertCount = projects.filter((p) => p.has_alert).length;
  const workingCount = projects.filter((p) =>
    p.tabs.some((tab: TabContext) => tab.status === "working"),
  ).length;
  const completedCount = projects.filter((p) =>
    p.tabs.some((tab: TabContext) => tab.status === "completed"),
  ).length;

  const menuTitle =
    alertCount > 0
      ? `${alertCount}`
      : workingCount > 0
        ? `${workingCount}`
        : completedCount > 0
          ? `${completedCount}`
          : undefined;

  const menuIcon = isOffline
    ? { source: Icon.Warning, tintColor: Color.SecondaryText }
    : alertCount > 0
      ? { source: Icon.ExclamationMark, tintColor: Color.Red }
      : workingCount > 0
        ? { source: Icon.Bolt, tintColor: Color.Orange }
        : completedCount > 0
          ? { source: Icon.CheckCircle, tintColor: Color.Green }
          : { source: "command-icon.png" };

  const renderProjectSubmenu = (proj: ProjectWorkspace) => (
    <MenuBarExtra.Submenu
      key={proj.repo_path}
      title={formatSubmenuTitle(proj)}
      icon={getProjectIcon(proj)}
    >
      <MenuBarExtra.Section
        title={
          proj.dirty_files > 0
            ? `${proj.git_branch} • ${proj.dirty_files} modified`
            : `${proj.git_branch} • ${i18n._("Clean")}`
        }
      >
        {proj.tabs.map((tab: TabContext) => (
          <MenuBarExtra.Item
            key={`${tab.browser}-${tab.tab_id}`}
            title={formatTabTitle(tab)}
            tooltip={tab.title || tab.url}
            icon={getTabStatusIcon(tab)}
            onAction={() =>
              queryDaemon({
                type: "focus_tab",
                browser: tab.browser,
                window_id: tab.window_id,
                tab_id: tab.tab_id,
              })
            }
          />
        ))}
      </MenuBarExtra.Section>

      <MenuBarExtra.Section title={i18n._("Action")}>
        <MenuBarExtra.Item
          title={i18n._("Open in Editor")}
          icon={Icon.Code}
          onAction={() =>
            queryDaemon({ type: "open_editor", repo_path: proj.repo_path })
          }
        />
        <MenuBarExtra.Item
          title={i18n._("Open in Terminal")}
          icon={Icon.Terminal}
          onAction={() =>
            queryDaemon({ type: "open_terminal", repo_path: proj.repo_path })
          }
        />
        <MenuBarExtra.Item
          title={i18n._("Open in Finder")}
          icon={Icon.Finder}
          onAction={() => open(proj.repo_path)}
        />
        {proj.repo_url && (
          <MenuBarExtra.Item
            title={i18n._("Open Repository on GitHub")}
            icon={Icon.Globe}
            onAction={() => open(proj.repo_url!)}
          />
        )}
        {proj.has_alert && (
          <MenuBarExtra.Item
            title={i18n._("Mute Repo Alerts (30m)")}
            icon={Icon.BellDisabled}
            onAction={async () => {
              await queryDaemon({
                type: "toggle_repo_mute",
                repo_name: proj.repo_name,
                duration_ms: 30 * 60 * 1000,
              });
              await showHUD(`Muted ${proj.repo_name} for 30m`);
            }}
          />
        )}
      </MenuBarExtra.Section>
    </MenuBarExtra.Submenu>
  );

  return (
    <MenuBarExtra
      icon={menuIcon}
      title={menuTitle}
      tooltip="ContextBraid"
      isLoading={isLoading}
    >
      {isOffline ? (
        <MenuBarExtra.Section title={i18n._("ContextBraid Engine Offline")}>
          <MenuBarExtra.Item
            title={i18n._("Open Project Cockpit")}
            icon={Icon.AppWindowSidebarLeft}
            onAction={() =>
              launchCommand({
                name: "index",
                type: LaunchType.UserInitiated,
              })
            }
          />
        </MenuBarExtra.Section>
      ) : (
        <>
          {alertCount > 0 && (
            <MenuBarExtra.Section>
              <MenuBarExtra.Item
                title={i18n._("Jump to Next Agent Alert")}
                icon={{ source: Icon.ExclamationMark, tintColor: Color.Red }}
                onAction={() => queryDaemon({ type: "jump_next_alert" })}
              />
            </MenuBarExtra.Section>
          )}

          {activeProjects.length > 0 && (
            <MenuBarExtra.Section title={i18n._("Active")}>
              {activeProjects.map(renderProjectSubmenu)}
            </MenuBarExtra.Section>
          )}

          {idleProjects.length > 0 && (
            <MenuBarExtra.Section title={i18n._("Repositories")}>
              {idleProjects.map(renderProjectSubmenu)}
            </MenuBarExtra.Section>
          )}

          <MenuBarExtra.Section title={i18n._("Preferences")}>
            <MenuBarExtra.Item
              title={
                prefs.notifyWhenFocused
                  ? i18n._("Notifications: Always On")
                  : i18n._("Notifications: Focus-Aware")
              }
              icon={
                prefs.notifyWhenFocused ? Icon.Bell : Icon.BellDisabled
              }
              onAction={() => openExtensionPreferences()}
            />
            <MenuBarExtra.Item
              title={i18n._("Open Project Cockpit")}
              icon={Icon.AppWindowSidebarLeft}
              shortcut={Keyboard.Shortcut.Common.Open}
              onAction={() =>
                launchCommand({
                  name: "index",
                  type: LaunchType.UserInitiated,
                })
              }
            />
          </MenuBarExtra.Section>
        </>
      )}
    </MenuBarExtra>
  );
}
