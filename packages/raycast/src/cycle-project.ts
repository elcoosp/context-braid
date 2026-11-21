import { queryDaemon } from './socketClient';

export default async function Command() {
  await queryDaemon({ type: 'cycle_project', repo_name: null });
}
