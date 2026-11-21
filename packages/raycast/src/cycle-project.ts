import { t } from '@lingui/core/macro';
import { showHUD } from '@raycast/api';
import './i18n';
import { queryDaemon } from './socketClient';

export default async function Command() {
  await queryDaemon({ type: 'cycle_project', repo_name: null });
  await showHUD(t`Cycled active project windows`);
}
