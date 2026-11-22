import { i18n } from "./i18n";
import "./i18n";
import { useEffect, useRef, useState } from "react";
import {
  getPreferenceValues,
  openExtensionPreferences,
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
import { PairBrowserForm } from "./PairBrowserForm";
import { ProjectWorkspace, queryDaemon, TabContext } from "./socketClient";

type StatusFilter = "all" | "attention" | "working" | "editor";

interface BraidPreferences {
  autoDownloadZip?: boolean;
  favoriteEditor?: string;
  muteDurationMinutes?: string;
}

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
  if (diffSec < 60)
    return i18n._({
      id: "xOTzt5",
      message: "just now",
    });
  const mins = Math.floor(diffSec / 60);
  if (mins < 60)
    return i18n._({
      id: "Mq2B8E",
      message: "{mins}m ago",
      values: {
        mins: mins,
      },
    });
  const hrs = Math.floor(mins / 60);
  return i18n._({
    id: "wXO4Tg",
    message: "{hrs}h ago",
    values: {
      hrs: hrs,
    },
  });
}

function formatCommitAge(unixSec: number): string {
  if (!unixSec || unixSec <= 0) return "";
  const diffSec = Math.max(0, Math.floor(Date.now() / 1000 - unixSec));
  if (diffSec < 60)
    return i18n._({
      id: "xOTzt5",
      message: "just now",
    });
  const mins = Math.floor(diffSec / 60);
  if (mins < 60)
    return i18n._({
      id: "Mq2B8E",
      message: "{mins}m ago",
      values: {
        mins: mins,
      },
    });
  const hrs = Math.floor(mins / 60);
  if (hrs < 24)
    return i18n._({
      id: "wXO4Tg",
      message: "{hrs}h ago",
      values: {
        hrs: hrs,
      },
    });
  const days = Math.floor(hrs / 24);
  return i18n._({
    id: "LPAv9E",
    message: "{days}d ago",
    values: {
      days: days,
    },
  });
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
    return {
      label: i18n._({
        id: "F6pfE9",
        message: "Active",
      }),
      color: Color.Blue,
    };
  }
  switch (tab.status) {
    case "needs_attention":
      return {
        label: i18n._({
          id: "fAuN2i",
          message: "Needs Help",
        }),
        color: Color.Red,
      };
    case "completed":
      return {
        label: i18n._({
          id: "Zqqk3p",
          message: "ZIP Ready",
        }),
        color: Color.Green,
      };
    case "working":
      return {
        label: i18n._({
          id: "b4itZn",
          message: "Working",
        }),
        color: Color.Blue,
      };
    default:
      return {
        label: i18n._({
          id: "ckH3fT",
          message: "Ready",
        }),
        color: Color.SecondaryText,
      };
  }
}

export default function Command() {
  const prefs = getPreferenceValues<BraidPreferences>();
  const muteMinutes = Math.max(
    1,
    Number.parseInt(prefs.muteDurationMinutes ?? "30", 10) || 30,
  );
  const syncedPrefsRef = useRef<string>("");
  const [filter, setFilter] = useState<StatusFilter>("all");
  const [showDetail, setShowDetail] = useState<boolean>(true);

  const lastSnapshotRef = useRef<{ json: string; list: ProjectWorkspace[] }>({
    json: "[]",
    list: [],
  });

  const { data, isLoading, error, revalidate } = usePromise(async () => {
    const res = await queryDaemon<{ projects: ProjectWorkspace[] }>({
      type: "get_dashboard",
    });
    const next = res.projects ?? [];
    const nextJson = JSON.stringify(next);
    if (nextJson === lastSnapshotRef.current.json) {
      return lastSnapshotRef.current.list;
    }
    lastSnapshotRef.current = { json: nextJson, list: next };
    return next;
  });

  useEffect(() => {
    const payload = {
      type: "update_config",
      auto_download_zip:
        typeof prefs.autoDownloadZip === "boolean"
          ? prefs.autoDownloadZip
          : true,
      favorite_editor: prefs.favoriteEditor || "zed",
    };
    const key = JSON.stringify(payload);
    if (key !== syncedPrefsRef.current) {
      syncedPrefsRef.current = key;
      queryDaemon(payload).catch(() => {});
    }
  }, [prefs.autoDownloadZip, prefs.favoriteEditor]);

  useEffect(() => {
    const id = setInterval(() => revalidate(), 1500);
    return () => clearInterval(id);
  }, [revalidate]);

  const installEngine = async () => {
    const toast = await showToast({
      style: Toast.Style.Animated,
      title: i18n._({
        id: "13MsVT",
        message: "Starting ContextBraid engine...",
      }),
    });
    try {
      await ensureBraidInstalledAndRunning();
      toast.style = Toast.Style.Success;
      toast.title = i18n._({
        id: "nRAiAG",
        message: "ContextBraid engine active",
      });
      revalidate();
    } catch (err) {
      toast.style = Toast.Style.Failure;
      toast.title = i18n._({
        id: "3VczXj",
        message: "Installation error",
      });
      toast.message = err instanceof Error ? err.message : String(err);
    }
  };

  const toggleRepoMute = async (proj: ProjectWorkspace) => {
    const isMuted = (proj.muted_until_ms ?? 0) > Date.now();
    await queryDaemon({
      type: "toggle_repo_mute",
      repo_name: proj.repo_name,
      duration_ms: muteMinutes * 60 * 1000,
    });
    await showToast({
      style: Toast.Style.Success,
      title: isMuted
        ? i18n._({
            id: "mEyGmz",
            message: "Alerts unmuted for {0}",
            values: {
              0: proj.repo_name,
            },
          })
        : i18n._({
            id: "rRzI8x",
            message: "Muted {0} for {muteMinutes}m",
            values: {
              0: proj.repo_name,
              muteMinutes: muteMinutes,
            },
          }),
    });
    revalidate();
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
      searchBarPlaceholder={i18n._({
        id: "KVBM8V",
        message: "Search a repo, branch, AI agent, or editor...",
      })}
      searchBarAccessory={
        <List.Dropdown
          tooltip={i18n._({
            id: "KwybLi",
            message: "Filter by status",
          })}
          value={filter}
          onChange={(v) => setFilter(v as StatusFilter)}
        >
          <List.Dropdown.Item
            title={i18n._({
              id: "dKqrLj",
              message: "All contexts",
            })}
            value="all"
            icon={Icon.AppWindowGrid2x2}
          />
          <List.Dropdown.Item
            title={i18n._({
              id: "J9_E9D",
              message: "Alerts & Ready ZIPs",
            })}
            value="attention"
            icon={Icon.Bell}
          />
          <List.Dropdown.Item
            title={i18n._({
              id: "0uESxc",
              message: "Generating",
            })}
            value="working"
            icon={Icon.Clock}
          />
          <List.Dropdown.Item
            title={i18n._({
              id: "qFAOGF",
              message: "Open editors",
            })}
            value="editor"
            icon={Icon.Code}
          />
        </List.Dropdown>
      }
      actions={
        <ActionPanel>
          <Action
            title={i18n._({
              id: "cyuks0",
              message: "Install / Start Engine",
            })}
            icon={Icon.Download}
            onAction={installEngine}
          />
          <Action
            title={i18n._({
              id: "4UQiFn",
              message: "Open Extension Preferences",
            })}
            icon={Icon.Gear}
            shortcut={{ modifiers: ["cmd", "shift"], key: "," }}
            onAction={openExtensionPreferences}
          />
          <Action.Push
            title={i18n._({
              id: "faid0A",
              message: "Connect Browser Extension ID",
            })}
            icon={Icon.Plug}
            target={<PairBrowserForm onDone={revalidate} />}
          />
        </ActionPanel>
      }
    >
      {error && (
        <List.EmptyView
          icon={{ source: Icon.Download, tintColor: Color.Blue }}
          title={i18n._({
            id: "0YEPcH",
            message: "ContextBraid local engine is not running",
          })}
          description={i18n._({
            id: "Xl2781",
            message:
              "Press Enter to automatically install and start the local background service.",
          })}
          actions={
            <ActionPanel>
              <Action
                title={i18n._({
                  id: "Us7jY4",
                  message: "Install and Start Now",
                })}
                icon={Icon.Download}
                onAction={installEngine}
              />
              <Action.Push
                title={i18n._({
                  id: "wmVzS_",
                  message: "Configure Extension ID",
                })}
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
              ? i18n._({
                  id: "oPhHLf",
                  message: "{0, plural, one {# modified} other {# modified}}",
                  values: {
                    0: proj.dirty_files,
                  },
                })
              : i18n._({
                  id: "tcceNK",
                  message: "clean",
                });
          const windowsLabel = i18n._({
            id: "qvv8aA",
            message: "{0, plural, one {# window} other {# windows}}",
            values: {
              0: proj.tabs.length,
            },
          });
          const mutedRemainingMin =
            (proj.muted_until_ms ?? 0) > Date.now()
              ? Math.max(
                  1,
                  Math.ceil(((proj.muted_until_ms ?? 0) - Date.now()) / 60000),
                )
              : 0;
          const mutedLabel =
            mutedRemainingMin > 0
              ? i18n._({
                  id: "8gx77c",
                  message: "Muted {mutedRemainingMin}m",
                  values: {
                    mutedRemainingMin: mutedRemainingMin,
                  },
                })
              : "";
          const sectionParts = [
            proj.git_branch ? `⎇ ${proj.git_branch}` : "",
            dirtyLabel,
            windowsLabel,
            mutedLabel,
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
                  accessories.push({
                    text: age,
                    tooltip: i18n._({
                      id: "4AKv0-",
                      message: "Last activity",
                    }),
                  });
                }
                if (tab.incognito) {
                  accessories.push({
                    icon: {
                      source: Icon.EyeDisabled,
                      tintColor: Color.SecondaryText,
                    },
                    tooltip: i18n._({
                      id: "t1Asbz",
                      message: "Private window",
                    }),
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
                    ? i18n._({
                        id: "oPhHLf",
                        message:
                          "{0, plural, one {# modified} other {# modified}}",
                        values: {
                          0: proj.dirty_files,
                        },
                      })
                    : i18n._({
                        id: "FsGJXJ",
                        message: "Clean",
                      });

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

                const prsBlock =
                  proj.recent_prs && proj.recent_prs.length > 0
                    ? [
                        `**${i18n._({
                          id: "so-RxJ",
                          message: "Recently Merged PRs",
                        })}**`,
                        ...proj.recent_prs.slice(0, 3).map((pr) => {
                          const rawPr = sanitizeMarkdownText(pr.title, 44);
                          return pr.url
                            ? `[#${pr.number}](${pr.url}) ${rawPr}`
                            : `\`#${pr.number}\` ${rawPr}`;
                        }),
                      ].join("  \n")
                    : "";

                const detailMarkdown = [
                  `## ${proj.repo_name}`,
                  activityCallout,
                  chartImage,
                  commitLine,
                  prsBlock,
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
                                title={i18n._({
                                  id: "poZmDH",
                                  message: "Current Step",
                                })}
                                text={workingAction}
                                icon={{
                                  source: Icon.Bolt,
                                  tintColor: Color.Blue,
                                }}
                              />
                            )}
                            <List.Item.Detail.Metadata.TagList
                              title={i18n._({
                                id: "MpZRQy",
                                message: "Git",
                              })}
                            >
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
                              title={i18n._({
                                id: "CqO9qC",
                                message: "Session",
                              })}
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
                        <ActionPanel.Section
                          title={i18n._({
                            id: "UxKoFf",
                            message: "Navigation",
                          })}
                        >
                          <Action
                            title={i18n._({
                              id: "CSREUX",
                              message: "Switch to {0} ({1})",
                              values: {
                                0: proj.repo_name,
                                1: role.name,
                              },
                            })}
                            icon={Icon.ArrowRight}
                            onAction={() =>
                              focusTab(proj.repo_name, role.name, tab)
                            }
                          />
                          <Action
                            title={i18n._({
                              id: "T51bMr",
                              message: "Cycle Project Windows",
                            })}
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
                            title={i18n._({
                              id: "nHdPm9",
                              message: "Jump to Next Agent Alert",
                            })}
                            icon={Icon.Bolt}
                            shortcut={{ modifiers: ["opt"], key: "a" }}
                            onAction={async () => {
                              await queryDaemon({ type: "jump_next_alert" });
                              revalidate();
                            }}
                          />
                        </ActionPanel.Section>

                        <ActionPanel.Section
                          title={i18n._({
                            id: "UsyU-l",
                            message: "Git & GitHub",
                          })}
                        >
                          {latestPr && latestPr.url && (
                            <Action.OpenInBrowser
                              title={i18n._({
                                id: "MB7Bqw",
                                message: "Open Latest PR (#{0})",
                                values: {
                                  0: latestPr.number,
                                },
                              })}
                              url={latestPr.url}
                              icon={Icon.Globe}
                              shortcut={{ modifiers: ["cmd"], key: "g" }}
                            />
                          )}
                          {proj.repo_url && (
                            <Action.OpenInBrowser
                              title={i18n._({
                                id: "b2C2hd",
                                message: "Open Repository on GitHub",
                              })}
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
                              title={i18n._({
                                id: "_EYfiL",
                                message: "Copy HEAD Commit SHA ({0})",
                                values: {
                                  0: proj.last_commit_sha,
                                },
                              })}
                              content={proj.last_commit_sha}
                              shortcut={{
                                modifiers: ["cmd", "shift"],
                                key: "h",
                              }}
                            />
                          )}
                        </ActionPanel.Section>

                        <ActionPanel.Section
                          title={i18n._({
                            id: "pmUArF",
                            message: "Workspace",
                          })}
                        >
                          <Action
                            title={i18n._({
                              id: "Z5IrB3",
                              message: "Open in {0}",
                              values: {
                                0: favEditor.label,
                              },
                            })}
                            icon={favEditor.icon}
                            shortcut={Keyboard.Shortcut.Common.Edit}
                            onAction={async () => {
                              await queryDaemon({
                                type: "open_editor",
                                repo_path: proj.repo_path,
                              });
                              await showToast({
                                style: Toast.Style.Success,
                                title: i18n._({
                                  id: "uc2X47",
                                  message: "Opened in {0}",
                                  values: {
                                    0: favEditor.label,
                                  },
                                }),
                              });
                            }}
                          />
                          <Action
                            title={
                              (proj.muted_until_ms ?? 0) > Date.now()
                                ? i18n._({
                                    id: "3lMESr",
                                    message: "Unmute Repo Alerts",
                                  })
                                : i18n._({
                                    id: "6fzWH-",
                                    message:
                                      "Mute Repo Alerts ({muteMinutes}m)",
                                    values: {
                                      muteMinutes: muteMinutes,
                                    },
                                  })
                            }
                            icon={
                              (proj.muted_until_ms ?? 0) > Date.now()
                                ? Icon.Bell
                                : Icon.BellDisabled
                            }
                            shortcut={{ modifiers: ["cmd"], key: "m" }}
                            onAction={() => toggleRepoMute(proj)}
                          />
                          <Action
                            title={i18n._({
                              id: "BJpGCq",
                              message: "Open in Terminal",
                            })}
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
                                ? i18n._({
                                    id: "IJOMB_",
                                    message: "Hide Detail Pane",
                                  })
                                : i18n._({
                                    id: "DcYffY",
                                    message: "Show Detail Pane",
                                  })
                            }
                            icon={Icon.Sidebar}
                            shortcut={{ modifiers: ["cmd"], key: "i" }}
                            onAction={() => setShowDetail((prev) => !prev)}
                          />
                          <Action.CopyToClipboard
                            title={i18n._({
                              id: "EP87tx",
                              message: "Copy Repository Path",
                            })}
                            content={proj.repo_path}
                            shortcut={Keyboard.Shortcut.Common.Copy}
                          />
                          <Action.ShowInFinder path={proj.repo_path} />
                          <Action.Push
                            title={i18n._({
                              id: "faid0A",
                              message: "Connect Browser Extension ID",
                            })}
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
