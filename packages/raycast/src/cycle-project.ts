import { showHUD } from '@raycast/api';
import { queryDaemon } from './socketClient';

export default async function main() {
  try {
    await queryDaemon({ type: 'cycle_project', repo_name: null });
  } catch {
    await showHUD('ContextBraid daemon (braidd) hors ligne');
  }
}
