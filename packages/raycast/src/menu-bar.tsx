import { Color, Icon, MenuBarExtra, launchCommand, LaunchType, open } from '@raycast/api';
import { t } from '@lingui/core/macro';
import { useEffect, useState } from 'react';
import './i18n';
import { ProjectWorkspace, queryDaemon, TabContext } from './socketClient';

export default function ContextBraidMenuBar() {
  const [projects, setProjects] = useState<ProjectWorkspace[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    let active = true;
    queryDaemon({ type: 'get_dashboard' })
      .then((res) => {
        const dash = res as { projects?: ProjectWorkspace[] };
        if (active && Array.isArray(dash?.projects)) {
          setProjects(dash.projects);
        }
      })
      .catch(() => {})
      .finally(() => {
        if (active) setIsLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const allTabs: Array<{ proj: ProjectWorkspace; tab: TabContext }> = projects.flatMap((p) =>
    p.tabs.map((tab) => ({ proj: p, tab })),
  );
  const alertsCount = allTabs.filter(
    ({ tab }) => tab.status === 'needs_attention' || tab.status === 'completed',
  ).length;
  const workingCount = allTabs.filter(({ tab }) => tab.status === 'working').length;

  const title = alertsCount > 0 ? `${alertsCount}` : workingCount > 0 ? `${workingCount}` : undefined;
  const icon =
    alertsCount > 0
      ? { source: Icon.ExclamationMark, tintColor: Color.Red }
      : workingCount > 0
        ? { source: Icon.Bolt, tintColor: Color.Blue }
        : { source: 'icon.svg', tintColor: Color.PrimaryText };

  return (
    <MenuBarExtra icon={icon} title={title} isLoading={isLoading} tooltip="ContextBraid">
      <MenuBarExtra.Item
        title={t`Open Project Cockpit`}
        icon={Icon.AppWindowSidebarLeft}
        shortcut={{ modifiers: ['cmd'], key: 'o' }}
        onAction={() => launchCommand({ name: 'index', type: LaunchType.UserInitiated })}
      />
      {alertsCount > 0 && (
        <MenuBarExtra.Item
          title={t`Jump to Next Agent Alert (${alertsCount})`}
          icon={{ source: Icon.Bell, tintColor: Color.Red }}
          shortcut={{ modifiers: ['opt'], key: 'a' }}
          onAction={() => queryDaemon({ type: 'jump_next_alert' })}
        />
      )}
      <MenuBarExtra.Separator />
      {projects.map((proj) => (
        <MenuBarExtra.Submenu
          key={proj.repo_path}
          title={`${proj.repo_name}${proj.git_branch ? ` (${proj.git_branch})` : ''}`}
          icon={Icon.Folder}
        >
          {proj.tabs.map((tab) => (
            <MenuBarExtra.Item
              key={`${tab.browser}-${tab.window_id}-${tab.tab_id}`}
              title={`${tab.browser} • ${tab.title || proj.repo_name}`}
              icon={
                tab.status === 'needs_attention'
                  ? { source: Icon.ExclamationMark, tintColor: Color.Red }
                  : tab.status === 'completed'
                    ? { source: Icon.CheckCircle, tintColor: Color.Green }
                    : tab.status === 'working'
                      ? { source: Icon.Bolt, tintColor: Color.Blue }
                      : Icon.Window
              }
              onAction={() =>
                queryDaemon({
                  type: 'focus_target',
                  browser: tab.browser,
                  window_id: tab.window_id,
                  tab_id: tab.tab_id,
                })
              }
            />
          ))}
          <MenuBarExtra.Separator />
          <MenuBarExtra.Item
            title={t`Open in Editor`}
            icon={Icon.Code}
            onAction={() => queryDaemon({ type: 'open_editor', repo_path: proj.repo_path })}
          />
          {proj.repo_url && (
            <MenuBarExtra.Item
              title={t`Open Repository on GitHub`}
              icon={Icon.Globe}
              onAction={() => open(proj.repo_url)}
            />
          )}
        </MenuBarExtra.Submenu>
      ))}
    </MenuBarExtra>
  );
}
