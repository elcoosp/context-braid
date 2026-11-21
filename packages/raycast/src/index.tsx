import { useEffect, useState } from "react";
import { plural, t } from "@lingui/core/macro";
import {
  Action,
  ActionPanel,
  Color,
  Icon,
  Image,
  List,
  showToast,
  Toast,
  Keyboard,
} from "@raycast/api";
import { usePromise } from "@raycast/utils";
import { ensureBraidInstalledAndRunning } from "./bootstrap";
import { BRAND_ICONS, browserBadge, editorMeta } from "./brandIcons";
import "./i18n";
import { PairBrowserForm } from "./PairBrowserForm";
import { ProjectWorkspace, queryDaemon, TabContext } from "./socketClient";

type StatusFilter = "all" | "attention" | "working" | "editor";

function detectRole(
  tab: TabContext,
  favoriteEditor: string,
): { name: string; icon: Image.ImageLike } {
  if (tab.browser === "editor" || tab.browser === "zed") {
    const ed = editorMeta(favoriteEditor);
    return { name: ed.label, icon: ed.icon };
  }
  let host = "";
  let pathname = "";
  try {
    const parsed = new URL(tab.url);
    host = parsed.hostname;
    pathname = parsed.pathname;
  } catch {
    host = tab.url;
  }
  if (host.endsWith("z.ai")) return { name: "z.ai", icon: BRAND_ICONS.zai };
  if (host.endsWith("qwen.ai")) return { name: "Qwen", icon: BRAND_ICONS.qwen };
  if (host === ["gemini", "google", "com"].join("."))
    return { name: "Gemini", icon: BRAND_ICONS.gemini };
  if (host.endsWith("google.com") && pathname.startsWith("/search")) {
    return { name: "Google AI", icon: BRAND_ICONS.gemini };
  }
  if (host.endsWith("deepseek.com"))
    return { name: "DeepSeek", icon: BRAND_ICONS.deepseek };
  if (host.endsWith("claude.ai")) return { name: "Claude", icon: Icon.Stars };
  if (
    host.endsWith(["chatgpt", "com"].join(".")) ||
    host.endsWith(["openai", "com"].join("."))
  )
    return { name: "ChatGPT", icon: Icon.Bubble };
  if (host.endsWith("perplexity.ai"))
    return { name: "Perplexity", icon: Icon.MagnifyingGlass };
  if (host.endsWith("mistral.ai")) return { name: "Mistral", icon: Icon.Bolt };
  if (host.endsWith("v0.dev") || host.endsWith("v0.app"))
    return { name: "v0", icon: Icon.AppWindow };
  if (host === "github.com" && pathname.startsWith("/copilot"))
    return { name: "Copilot", icon: Icon.Code };
  return { name: "Web", icon: BRAND_ICONS.browser };
}

function sanitizeMarkdownText(raw: string, maxLen = 64): string {
  if (!raw) return "";
  const cleaned = raw
    .replace(/\r?\n+/g, " ")
    .replace(/[`*_~<>|]/g, " ")
    .replace(/\s{2,}/g, " ")
    .trim();
  return cleaned.length <= maxLen
    ? cleaned
    : `${cleaned.slice(0, maxLen - 1).trimEnd()}…`;
}

function extractCleanIntent(
  rawTitle: string,
  repoName: string,
  isGoogleAi: boolean,
): string {
  if (!rawTitle) return "";
  let s = rawTitle
    .replace(
      /^(Zed|Cursor|Visual Studio Code|Code|Windsurf|Neovim)\s*[—–-]\s*/i,
      "",
    )
    .replace(/\s*\|\s*Z\.ai.*$/i, "")
    .replace(/\s*-\s*Qwen.*$/i, "")
    .replace(/\s*-\s*Google Gemini.*$/i, "")
    .replace(
      /\s*-\s*(Recherche Google|Google Search|Claude|ChatGPT|Perplexity|Le Chat|Mistral).*$/i,
      "",
    )
    .trim();

  if (isGoogleAi) {
    // Google AI embeds the entire pasted prompt + error log in document.title:
    // keep only the first human instruction before pasted logs, brackets, or separators
    s = s.split(/\r?\n|[|×•]|\[\[/)[0].trim();
    s = s.replace(/\s*\[[^\]]+\]\s*$/g, "").trim();
  }

  s = s
    .replace(/\r?\n+/g, " ")
    .replace(/[`*_~<>|]/g, " ")
    .replace(/\s{2,}/g, " ")
    .trim();

  if (!s || s.toLowerCase() === repoName.toLowerCase()) return "";
  const maxLen = isGoogleAi ? 64 : 88;
  return s.length <= maxLen ? s : `${s.slice(0, maxLen - 1).trimEnd()}…`;
}

function formatRelativeAge(updatedAtMs: number): string {
  if (!updatedAtMs || updatedAtMs <= 0) return "";
  const diffSec = Math.max(0, Math.floor((Date.now() - updatedAtMs) / 1000));
  if (diffSec < 60) return t`just now`;
  const mins = Math.floor(diffSec / 60);
  if (mins < 60) return t`${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  return t`${hrs}h ago`;
}

function formatCommitAge(unixSec: number): string {
  if (!unixSec || unixSec <= 0) return "";
  const diffSec = Math.max(0, Math.floor(Date.now() / 1000 - unixSec));
  if (diffSec < 60) return t`just now`;
  const mins = Math.floor(diffSec / 60);
  if (mins < 60) return t`${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return t`${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return t`${days}d ago`;
}

function shortenHomePath(fullPath: string): string {
  return fullPath.replace(/^\/Users\/[^/]+/, "~");
}

function extractWorkingAction(tab: TabContext): string {
  if (tab.status !== "working" || !tab.context_snippet) return "";
  const idx = tab.context_snippet.indexOf(" — ");
  if (idx > 0 && idx <= 80) {
    return sanitizeMarkdownText(tab.context_snippet.slice(0, idx), 44);
  }
  return "";
}

function mainCiIcon(status: string): Image.ImageLike | undefined {
  switch (status) {
    case "success":
      return { source: Icon.CheckCircle, tintColor: Color.Green };
    case "failure":
      return { source: Icon.XMarkCircle, tintColor: Color.Red };
    case "pending":
      return { source: Icon.Clock, tintColor: Color.Yellow };
    default:
      return undefined;
  }
}

function statusMeta(tab: TabContext): { label: string; color: Color } {
  if (tab.browser === "editor" || tab.browser === "zed") {
    return { label: t`Active`, color: Color.Blue };
  }
  switch (tab.status) {
    case "needs_attention":
      return { label: t`Needs Help`, color: Color.Red };
    case "completed":
      return { label: t`ZIP Ready`, color: Color.Green };
    case "working":
      return { label: t`Working`, color: Color.Blue };
    default:
      return { label: t`Ready`, color: Color.SecondaryText };
  }
}

export default function Command() {
  const [filter, setFilter] = useState<StatusFilter>("all");
  const [showDetail, setShowDetail] = useState<boolean>(true);

  const { data, isLoading, error, revalidate } = usePromise(async () => {
    const res = await queryDaemon<{ projects: ProjectWorkspace[] }>({
      type: "get_dashboard",
    });
    return res.projects ?? [];
  });

  useEffect(() => {
    const id = setInterval(() => revalidate(), 1500);
    return () => clearInterval(id);
  }, [revalidate]);

  const installEngine = async () => {
    const toast = await showToast({
      style: Toast.Style.Animated,
      title: t`Starting ContextBraid engine...`,
    });
    try {
      await ensureBraidInstalledAndRunning();
      toast.style = Toast.Style.Success;
      toast.title = t`ContextBraid engine active`;
      revalidate();
    } catch (err) {
      toast.style = Toast.Style.Failure;
      toast.title = t`Installation error`;
      toast.message = err instanceof Error ? err.message : String(err);
    }
  };

  const focusTab = async (
    projName: string,
    roleName: string,
    tab: TabContext,
  ) => {
    await queryDaemon({
      type: "focus_target",
      browser: tab.browser,
      window_id: tab.window_id,
      tab_id: tab.tab_id,
    });
    await showToast({
      style: Toast.Style.Success,
      title: `${projName} • ${roleName}`,
    });
  };

  const filteredProjects = (data ?? [])
    .map((proj) => ({
      ...proj,
      tabs: proj.tabs.filter((t) => {
        if (filter === "attention")
          return t.status === "needs_attention" || t.status === "completed";
        if (filter === "working") return t.status === "working";
        if (filter === "editor")
          return t.browser === "editor" || t.browser === "zed";
        return true;
      }),
    }))
    .filter((proj) => proj.tabs.length > 0);

  return (
    <List
      isLoading={isLoading}
      isShowingDetail={showDetail}
      searchBarPlaceholder={t`Search a repo, branch, AI agent, or editor...`}
      searchBarAccessory={
        <List.Dropdown
          tooltip={t`Filter by status`}
          value={filter}
          onChange={(v) => setFilter(v as StatusFilter)}
        >
          <List.Dropdown.Item
            title={t`All contexts`}
            value="all"
            icon={Icon.AppWindowGrid2x2}
          />
          <List.Dropdown.Item
            title={t`Alerts & Ready ZIPs`}
            value="attention"
            icon={Icon.Bell}
          />
          <List.Dropdown.Item
            title={t`Generating`}
            value="working"
            icon={Icon.Clock}
          />
          <List.Dropdown.Item
            title={t`Open editors`}
            value="editor"
            icon={Icon.Code}
          />
        </List.Dropdown>
      }
      actions={
        <ActionPanel>
          <Action
            title={t`Install / Start Engine`}
            icon={Icon.Download}
            onAction={installEngine}
          />
          <Action.Push
            title={t`Connect Browser Extension ID`}
            icon={Icon.Plug}
            target={<PairBrowserForm onDone={revalidate} />}
          />
        </ActionPanel>
      }
    >
      {error && (
        <List.EmptyView
          icon={{ source: Icon.Download, tintColor: Color.Blue }}
          title={t`ContextBraid local engine is not running`}
          description={t`Press Enter to automatically install and start the local background service.`}
          actions={
            <ActionPanel>
              <Action
                title={t`Install and Start Now`}
                icon={Icon.Download}
                onAction={installEngine}
              />
              <Action.Push
                title={t`Configure Extension ID`}
                icon={Icon.Plug}
                target={<PairBrowserForm onDone={revalidate} />}
              />
            </ActionPanel>
          }
        />
      )}

      {!error &&
        filteredProjects.map((proj) => {
          const favEditor = editorMeta(proj.favorite_editor);
          const dirtyLabel =
            proj.dirty_files > 0
              ? plural(proj.dirty_files, {
                  one: "# modified",
                  other: "# modified",
                })
              : t`clean`;
          const windowsLabel = plural(proj.tabs.length, {
            one: "# window",
            other: "# windows",
          });
          const sectionParts = [
            proj.git_branch ? `⎇ ${proj.git_branch}` : "",
            dirtyLabel,
            windowsLabel,
          ].filter(Boolean);

          return (
            <List.Section
              key={proj.repo_name}
              title={proj.repo_name}
              subtitle={sectionParts.join("  •  ")}
            >
              {proj.tabs.map((tab) => {
                const role = detectRole(tab, proj.favorite_editor);
                const meta = statusMeta(tab);
                const bBadge = browserBadge(tab.browser, proj.favorite_editor);
                const workingAction = extractWorkingAction(tab);
                const taskDetail = extractCleanIntent(
                  tab.title,
                  proj.repo_name,
                  role.name === "Google AI",
                );
                const rawRowSub = workingAction || taskDetail;
                const rowSubtitle =
                  rawRowSub.length <= 26
                    ? rawRowSub
                    : `${rawRowSub.slice(0, 25).trimEnd()}…`;
                const age = formatRelativeAge(tab.updated_at_ms);

                const accessories: List.Item.Accessory[] = [];
                if (age && !showDetail) {
                  accessories.push({ text: age, tooltip: t`Last activity` });
                }
                if (tab.incognito) {
                  accessories.push({
                    icon: {
                      source: Icon.EyeDisabled,
                      tintColor: Color.SecondaryText,
                    },
                    tooltip: t`Private window`,
                  });
                }
                accessories.push({ icon: bBadge.icon, tooltip: bBadge.label });
                accessories.push({
                  tag: { value: meta.label, color: meta.color },
                });

                const commitAge = formatCommitAge(proj.last_commit_ts);

                const commits30d = (proj.commit_sparkline ?? []).reduce(
                  (a, b) => a + b,
                  0,
                );
                const formattedCommits = commits30d.toLocaleString("en-US");
                const chartImage = proj.sparkline_png_path
                  ? `![](file://${proj.sparkline_png_path})`
                  : "";
                const latestPr = proj.recent_prs?.at(0) ?? null;

                const gitStatusLabel =
                  proj.dirty_files > 0
                    ? plural(proj.dirty_files, {
                        one: "# modified",
                        other: "# modified",
                      })
                    : t`Clean`;

                const activityCallout =
                  tab.browser === "editor" || tab.browser === "zed"
                    ? `> **${favEditor.label}** · \`${shortenHomePath(proj.repo_path)}\``
                    : workingAction
                      ? taskDetail
                        ? `> **${workingAction}**  \n> *${taskDetail}*`
                        : `> **${workingAction}**`
                      : taskDetail
                        ? `> **${role.name}** — ${taskDetail}`
                        : `\`${shortenHomePath(proj.repo_path)}\``;

                const cleanCommitMsg = sanitizeMarkdownText(
                  proj.last_commit_msg,
                  56,
                );
                const commitLine = proj.last_commit_sha
                  ? `**HEAD** \`${proj.last_commit_sha}\` · ${cleanCommitMsg}${commitAge ? ` *(${commitAge})*` : ""}`
                  : "";

                const detailMarkdown = [
                  `## ${proj.repo_name}`,
                  activityCallout,
                  chartImage,
                  commitLine,
                ]
                  .filter(Boolean)
                  .join("\n\n");

                return (
                  <List.Item
                    key={`${tab.browser}-${tab.window_id}-${tab.tab_id}`}
                    icon={role.icon}
                    title={role.name}
                    subtitle={
                      rowSubtitle
                        ? { value: rowSubtitle, tooltip: rowSubtitle }
                        : undefined
                    }
                    keywords={[
                      proj.repo_name,
                      proj.git_branch,
                      proj.last_commit_sha,
                      proj.last_commit_msg,
                      role.name,
                      tab.browser,
                      workingAction,
                      taskDetail,
                      meta.label,
                    ]}
                    accessories={accessories}
                    detail={
                      <List.Item.Detail
                        markdown={detailMarkdown}
                        metadata={
                          <List.Item.Detail.Metadata>
                            {workingAction && (
                              <List.Item.Detail.Metadata.Label
                                title={t`Current Step`}
                                text={workingAction}
                                icon={{
                                  source: Icon.Bolt,
                                  tintColor: Color.Blue,
                                }}
                              />
                            )}
                            <List.Item.Detail.Metadata.TagList title={t`Git`}>
                              {proj.git_branch && (
                                <List.Item.Detail.Metadata.TagList.Item
                                  text={`⎇ ${proj.git_branch}`}
                                  icon={mainCiIcon(proj.main_ci_status)}
                                  color={Color.SecondaryText}
                                />
                              )}
                              <List.Item.Detail.Metadata.TagList.Item
                                text={gitStatusLabel}
                                color={
                                  proj.dirty_files > 0
                                    ? Color.Orange
                                    : Color.SecondaryText
                                }
                              />
                              {commits30d > 0 && (
                                <List.Item.Detail.Metadata.TagList.Item
                                  text={`${formattedCommits} / 30d`}
                                  color={Color.SecondaryText}
                                />
                              )}
                            </List.Item.Detail.Metadata.TagList>
                            <List.Item.Detail.Metadata.TagList
                              title={t`Session`}
                            >
                              <List.Item.Detail.Metadata.TagList.Item
                                text={bBadge.label}
                                icon={bBadge.icon}
                                color={Color.SecondaryText}
                              />
                              <List.Item.Detail.Metadata.TagList.Item
                                text={meta.label}
                                color={meta.color}
                              />
                              {age && (
                                <List.Item.Detail.Metadata.TagList.Item
                                  text={age}
                                  color={Color.SecondaryText}
                                />
                              )}
                            </List.Item.Detail.Metadata.TagList>
                          </List.Item.Detail.Metadata>
                        }
                      />
                    }
                    actions={
                      <ActionPanel>
                        <ActionPanel.Section title={t`Navigation`}>
                          <Action
                            title={t`Switch to ${proj.repo_name} (${role.name})`}
                            icon={Icon.ArrowRight}
                            onAction={() =>
                              focusTab(proj.repo_name, role.name, tab)
                            }
                          />
                          <Action
                            title={t`Cycle Project Windows`}
                            icon={Icon.Switch}
                            shortcut={{ modifiers: ["opt"], key: "tab" }}
                            onAction={async () => {
                              await queryDaemon({
                                type: "cycle_project",
                                repo_name: proj.repo_name,
                              });
                              revalidate();
                            }}
                          />
                          <Action
                            title={t`Jump to Next Agent Alert`}
                            icon={Icon.Bolt}
                            shortcut={{ modifiers: ["opt"], key: "a" }}
                            onAction={async () => {
                              await queryDaemon({ type: "jump_next_alert" });
                              revalidate();
                            }}
                          />
                        </ActionPanel.Section>

                        <ActionPanel.Section title={t`Git & GitHub`}>
                          {latestPr && latestPr.url && (
                            <Action.OpenInBrowser
                              title={t`Open Latest PR (#${latestPr.number})`}
                              url={latestPr.url}
                              icon={Icon.Globe}
                              shortcut={{ modifiers: ["cmd"], key: "g" }}
                            />
                          )}
                          {proj.repo_url && (
                            <Action.OpenInBrowser
                              title={t`Open Repository on GitHub`}
                              url={proj.repo_url}
                              icon={Icon.Link}
                              shortcut={{
                                modifiers: ["cmd", "shift"],
                                key: "g",
                              }}
                            />
                          )}
                          {proj.last_commit_sha && (
                            <Action.CopyToClipboard
                              title={t`Copy HEAD Commit SHA (${proj.last_commit_sha})`}
                              content={proj.last_commit_sha}
                              shortcut={{
                                modifiers: ["cmd", "shift"],
                                key: "h",
                              }}
                            />
                          )}
                        </ActionPanel.Section>

                        <ActionPanel.Section title={t`Workspace`}>
                          <Action
                            title={t`Open in ${favEditor.label}`}
                            icon={favEditor.icon}
                            shortcut={Keyboard.Shortcut.Common.Edit}
                            onAction={async () => {
                              await queryDaemon({
                                type: "open_editor",
                                repo_path: proj.repo_path,
                              });
                              await showToast({
                                style: Toast.Style.Success,
                                title: t`Opened in ${favEditor.label}`,
                              });
                            }}
                          />
                          <Action
                            title={t`Open in Terminal`}
                            icon={Icon.Terminal}
                            shortcut={{ modifiers: ["cmd"], key: "t" }}
                            onAction={async () => {
                              await queryDaemon({
                                type: "open_terminal",
                                repo_path: proj.repo_path,
                              });
                              await showToast({
                                style: Toast.Style.Success,
                                title: `Terminal • ${proj.repo_name}`,
                              });
                            }}
                          />
                          <Action
                            title={
                              showDetail
                                ? t`Hide Detail Pane`
                                : t`Show Detail Pane`
                            }
                            icon={Icon.Sidebar}
                            shortcut={{ modifiers: ["cmd"], key: "i" }}
                            onAction={() => setShowDetail((prev) => !prev)}
                          />
                          <Action.CopyToClipboard
                            title={t`Copy Repository Path`}
                            content={proj.repo_path}
                            shortcut={Keyboard.Shortcut.Common.Copy}
                          />
                          <Action.ShowInFinder path={proj.repo_path} />
                          <Action.Push
                            title={t`Connect Browser Extension ID`}
                            icon={Icon.Plug}
                            target={<PairBrowserForm onDone={revalidate} />}
                          />
                        </ActionPanel.Section>
                      </ActionPanel>
                    }
                  />
                );
              })}
            </List.Section>
          );
        })}
    </List>
  );
}
