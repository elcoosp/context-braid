import { showHUD } from '@raycast/api';
import { queryDaemon } from './socketClient';

export default async function main() {
  try {
    await queryDaemon({ type: 'jump_next_alert' });
  } catch {
    await showHUD('ContextBraid daemon (braidd) hors ligne');
  }
}
