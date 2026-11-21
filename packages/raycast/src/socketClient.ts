import net from 'node:net';

const SOCKET_PATH = '/tmp/braid.sock';

export interface TabContext {
  browser: string;
  window_id: number;
  tab_id: number;
  opener_tab_id: number | null;
  incognito: boolean;
  url: string;
  title: string;
  context_snippet: string;
  status: 'idle' | 'working' | 'needs_attention' | 'completed';
  updated_at_ms: number;
}

export interface MergedPr {
  number: number;
  title: string;
  url: string;
}

export interface ProjectWorkspace {
  repo_name: string;
  repo_path: string;
  git_branch: string;
  dirty_files: number;
  favorite_editor: string;
  last_commit_sha: string;
  last_commit_msg: string;
  last_commit_ts: number;
  commit_sparkline: number[];
  sparkline_png_path: string;
  repo_url: string;
  recent_prs: MergedPr[];
  tabs: TabContext[];
  has_alert: boolean;
}

function sendOnce<T>(payload: Record<string, unknown>): Promise<T> {
  return new Promise((resolve, reject) => {
    const client = net.createConnection({ path: SOCKET_PATH });
    let buffer = '';

    client.setTimeout(5000);
    client.on('connect', () => {
      client.write(`${JSON.stringify(payload)}\n`);
    });

    client.on('data', (chunk) => {
      buffer += chunk.toString('utf8');
      const nl = buffer.indexOf('\n');
      if (nl !== -1) {
        const line = buffer.slice(0, nl);
        client.end();
        try {
          resolve(JSON.parse(line) as T);
        } catch (err) {
          reject(err);
        }
      }
    });

    client.on('timeout', () => {
      client.destroy();
      reject(new Error('Daemon socket timeout'));
    });

    client.on('error', (err) => reject(err));
  });
}

export async function queryDaemon<T>(payload: Record<string, unknown>): Promise<T> {
  let lastErr: unknown;
  for (let attempt = 0; attempt < 4; attempt++) {
    try {
      return await sendOnce<T>(payload);
    } catch (err) {
      lastErr = err;
      await new Promise((r) => setTimeout(r, 150));
    }
  }
  throw lastErr;
}
