import { useEffect, useState } from 'react';
import { Action, ActionPanel, Color, Icon, Image, List, showToast, Toast } from '@raycast/api';
import { usePromise } from '@raycast/utils';
import { ensureBraidInstalledAndRunning } from './bootstrap';
import { BRAND_ICONS, browserBadge } from './brandIcons';
import { PairBrowserForm } from './PairBrowserForm';
import { ProjectWorkspace, queryDaemon, TabContext } from './socketClient';

type StatusFilter = 'all' | 'attention' | 'working' | 'editor';

function detectRole(tab: TabContext): { name: string; icon: Image.ImageLike } {
  if (tab.browser === 'editor' || tab.browser === 'zed') {
    return { name: tab.title.split(' — ').at(0) || 'Zed', icon: BRAND_ICONS.zed };
  }
  let host = '';
  let pathname = '';
  try {
    const parsed = new URL(tab.url);
    host = parsed.hostname;
    pathname = parsed.pathname;
  } catch {
    host = tab.url;
  }
  if (host.endsWith('z.ai')) return { name: 'z.ai', icon: BRAND_ICONS.zai };
  if (host.endsWith('qwen.ai')) return { name: 'Qwen', icon: BRAND_ICONS.qwen };
  if (host === '://google.com') return { name: 'Gemini', icon: BRAND_ICONS.gemini };
  if (host.endsWith('google.com') && pathname.startsWith('/search')) {
    return { name: 'Google AI', icon: BRAND_ICONS.gemini };
  }
  if (host.endsWith('deepseek.com')) return { name: 'DeepSeek', icon: BRAND_ICONS.deepseek };
  return { name: 'Web', icon: BRAND_ICONS.browser };
}

function cleanTaskTitle(rawTitle: string, repoName: string): string {
  const stripped = rawTitle
    .replace(/\s*\|\s*Z\.ai.*$/i, '')
    .replace(/\s*-\s*Qwen.*$/i, '')
    .replace(/\s*-\s*Google Gemini.*$/i, '')
    .replace(/\s*-\s*Recherche Google.*$/i, '')
    .trim();
  return !stripped || stripped.toLowerCase() === repoName.toLowerCase() ? '' : stripped;
}

function statusMeta(tab: TabContext): { label: string; color: Color; badgeIcon?: Image.ImageLike } {
  if (tab.browser === 'editor' || tab.browser === 'zed') {
    return { label: 'Actif', color: Color.Purple };
  }
  switch (tab.status) {
    case 'needs_attention':
      return { label: 'Captcha', color: Color.Red, badgeIcon: BRAND_ICONS.alert };
    case 'completed':
      return { label: 'ZIP Prêt', color: Color.Green, badgeIcon: BRAND_ICONS.completed };
    case 'working':
      return { label: 'En cours', color: Color.Blue };
    default:
      return { label: 'Prêt', color: Color.SecondaryText };
  }
}

export default function Command() {
  const [filter, setFilter] = useState<StatusFilter>('all');
  const [showDetail, setShowDetail] = useState<boolean>(false);

  const { data, isLoading, error, revalidate } = usePromise(async () => {
    const res = await queryDaemon<{ projects: ProjectWorkspace[] }>({ type: 'get_dashboard' });
    return res.projects ?? [];
  });

  useEffect(() => {
    const id = setInterval(() => revalidate(), 1500);
    return () => clearInterval(id);
  }, [revalidate]);

  const installEngine = async () => {
    const toast = await showToast({ style: Toast.Style.Animated, title: 'Démarrage moteur ContextBraid...' });
    try {
      await ensureBraidInstalledAndRunning();
      toast.style = Toast.Style.Success;
      toast.title = 'Moteur ContextBraid actif';
      revalidate();
    } catch (err) {
      toast.style = Toast.Style.Failure;
      toast.title = "Erreur d'installation";
      toast.message = err instanceof Error ? err.message : String(err);
    }
  };

  const focusTab = async (projName: string, roleName: string, tab: TabContext) => {
    await queryDaemon({
      type: 'focus_target',
      browser: tab.browser,
      window_id: tab.window_id,
      tab_id: tab.tab_id,
    });
    await showToast({ style: Toast.Style.Success, title: `${projName} • ${roleName}` });
  };

  const filteredProjects = (data ?? [])
    .map((proj) => ({
      ...proj,
      tabs: proj.tabs.filter((t) => {
        if (filter === 'attention') return t.status === 'needs_attention' || t.status === 'completed';
        if (filter === 'working') return t.status === 'working';
        if (filter === 'editor') return t.browser === 'editor' || t.browser === 'zed';
        return true;
      }),
    }))
    .filter((proj) => proj.tabs.length > 0);

  return (
    <List
      isLoading={isLoading}
      isShowingDetail={showDetail}
      searchBarPlaceholder="Rechercher un dépôt, un agent (Qwen, z.ai, Gemini) ou un navigateur..."
      searchBarAccessory={
        <List.Dropdown tooltip="Filtrer par état" value={filter} onChange={(v) => setFilter(v as StatusFilter)}>
          <List.Dropdown.Item title="Tous les contextes" value="all" icon={Icon.AppWindowGrid2x2} />
          <List.Dropdown.Item title="Alertes & ZIP Prêts" value="attention" icon={Icon.Bell} />
          <List.Dropdown.Item title="En cours de génération" value="working" icon={Icon.Clock} />
          <List.Dropdown.Item title="Éditeurs ouverts" value="editor" icon={Icon.Code} />
        </List.Dropdown>
      }
      actions={
        <ActionPanel>
          <Action title="Installer / Démarrer le moteur" icon={Icon.Download} onAction={installEngine} />
          <Action.Push
            title="Connecter un ID d'extension navigateur"
            icon={Icon.Plug}
            target={<PairBrowserForm onDone={revalidate} />}
          />
        </ActionPanel>
      }
    >
      {error && (
        <List.EmptyView
          icon={{ source: Icon.Download, tintColor: Color.Blue }}
          title="Moteur local ContextBraid non démarré"
          description="Appuyez sur Entrée pour installer et démarrer automatiquement le service local."
          actions={
            <ActionPanel>
              <Action title="Installer et démarrer maintenant" icon={Icon.Download} onAction={installEngine} />
              <Action.Push
                title="Configurer un ID d'extension"
                icon={Icon.Plug}
                target={<PairBrowserForm onDone={revalidate} />}
              />
            </ActionPanel>
          }
        />
      )}

      {!error &&
        filteredProjects.map((proj) => (
          <List.Section
            key={proj.repo_name}
            title={proj.repo_name}
            subtitle={`${proj.tabs.length} fenêtre${proj.tabs.length > 1 ? 's' : ''}`}
          >
            {proj.tabs.map((tab) => {
              const role = detectRole(tab);
              const meta = statusMeta(tab);
              const bBadge = browserBadge(tab.browser);
              const taskDetail = cleanTaskTitle(tab.title, proj.repo_name);

              const accessories: List.Item.Accessory[] = [];
              if (tab.incognito) {
                accessories.push({
                  icon: { source: Icon.EyeDisabled, tintColor: Color.SecondaryText },
                  tooltip: 'Fenêtre privée',
                });
              }
              accessories.push({ icon: bBadge.icon, tooltip: bBadge.label });
              accessories.push({ tag: { value: meta.label, color: meta.color } });

              const detailMarkdown = [
                `### ${proj.repo_name} — ${role.name}`,
                taskDetail ? taskDetail : '',
              ]
                .filter(Boolean)
                .join('\n\n');

              return (
                <List.Item
                  key={`${tab.browser}-${tab.window_id}-${tab.tab_id}`}
                  icon={role.icon}
                  title={role.name}
                  subtitle={showDetail ? undefined : taskDetail}
                  keywords={[proj.repo_name, role.name, tab.browser, taskDetail, meta.label]}
                  accessories={accessories}
                  detail={
                    <List.Item.Detail
                      markdown={detailMarkdown}
                      metadata={
                        <List.Item.Detail.Metadata>
                          <List.Item.Detail.Metadata.Label title="Dépôt" text={proj.repo_name} icon={Icon.Folder} />
                          <List.Item.Detail.Metadata.Label title="Chemin" text={proj.repo_path} />
                          <List.Item.Detail.Metadata.Separator />
                          <List.Item.Detail.Metadata.Label title="Surface" text={bBadge.label} icon={bBadge.icon} />
                          <List.Item.Detail.Metadata.TagList title="État">
                            <List.Item.Detail.Metadata.TagList.Item text={meta.label} color={meta.color} />
                            {tab.incognito && (
                              <List.Item.Detail.Metadata.TagList.Item text="Privé" color={Color.SecondaryText} />
                            )}
                          </List.Item.Detail.Metadata.TagList>
                        </List.Item.Detail.Metadata>
                      }
                    />
                  }
                  actions={
                    <ActionPanel>
                      <Action
                        title={`Ouvrir ${proj.repo_name} (${role.name})`}
                        icon={Icon.ArrowRight}
                        onAction={() => focusTab(proj.repo_name, role.name, tab)}
                      />
                      <Action
                        title="Rotation fenêtres du projet"
                        icon={Icon.Switch}
                        shortcut={{ modifiers: ['opt'], key: 'tab' }}
                        onAction={async () => {
                          await queryDaemon({ type: 'cycle_project', repo_name: proj.repo_name });
                          revalidate();
                        }}
                      />
                      <Action
                        title="Sauter vers la prochaine alerte"
                        icon={Icon.Bolt}
                        shortcut={{ modifiers: ['opt'], key: 'a' }}
                        onAction={async () => {
                          await queryDaemon({ type: 'jump_next_alert' });
                          revalidate();
                        }}
                      />
                      <Action
                        title={showDetail ? 'Masquer le panneau détail' : 'Afficher le panneau détail'}
                        icon={Icon.Sidebar}
                        shortcut={{ modifiers: ['cmd'], key: 'i' }}
                        onAction={() => setShowDetail((prev) => !prev)}
                      />
                      <Action.ShowInFinder path={proj.repo_path} />
                      <Action.Push
                        title="Connecter un ID d'extension navigateur"
                        icon={Icon.Plug}
                        target={<PairBrowserForm onDone={revalidate} />}
                      />
                    </ActionPanel>
                  }
                />
              );
            })}
          </List.Section>
        ))}
    </List>
  );
}
