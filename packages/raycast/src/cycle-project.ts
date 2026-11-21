import { closeMainWindow } from '@raycast/api';
import { queryDaemon } from './socketClient';

export default async function Command() {
  await Promise.all([
    closeMainWindow({ clearRootSearch: false }),
    queryDaemon({ type: 'cycle_project', repo_name: null }),
  ]);
}
