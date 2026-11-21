import { describe, expect, test, beforeEach } from 'vitest';
import { extractPageState } from '../../src/core/extractor';

describe('ContextBraid Fast Extractor (Chromium & Firefox)', () => {
  beforeEach(() => {
    document.title = '';
    document.body.innerHTML = '';
  });

  test('extrait le repo depuis [data-user-bubble="true"] et détecte "· zip" dans Generated files', () => {
    document.title = 'Deliver Ship.toml & ZIP for GU... | Z.ai';
    document.body.innerHTML = `
      <nav><span title="Deliver Ship.toml & ZIP for GUIwright">Deliver Ship.toml...</span></nav>
      <main>
        <div data-user-bubble="true">Task: elcoosp/guiwright. Deliver ship.toml AND THE ZIP.</div>
        <ul>
          <li class="group/item">
            <span>guiwright-runner-reports-nextest</span>
            <span>· zip</span>
          </li>
        </ul>
      </main>
    `;

    const res = extractPageState(document);
    expect(res.status).toBe('completed');
    expect(res.contextSnippet).toContain('guiwright');
  });

  test('détecte Aliyun Captcha (#aliyunCaptcha-float-wrapper) sur z.ai en priorité', () => {
    document.title = 'Task: elcoosp/zsx | Z.ai';
    document.body.innerHTML = `
      <div id="aliyunCaptcha-float-wrapper">
        <div id="aliyunCaptcha-captcha-body"></div>
      </div>
      <div class="terminal-loading"></div>
    `;

    const res = extractPageState(document);
    expect(res.status).toBe('needs_attention');
    expect(res.contextSnippet).toContain('zsx');
  });

  test('détecte la génération en cours via .terminal-loading', () => {
    document.title = 'Task: context-braid | Z.ai';
    document.body.innerHTML = '<div class="terminal-loading"></div>';

    expect(extractPageState(document).status).toBe('working');
  });

  test('détecte la bannière d’erreur runtime z.ai ("Oops, something went wrong") comme needs_attention', () => {
    document.title = 'Glyim';
    document.body.innerHTML = `
      <main>
        <div data-user-bubble="true">Fix clippy errors on glyim</div>
        <div>
          <span>Oops, something went wrong. Please refresh the page or try again later.</span>
        </div>
      </main>
    `;

    const res = extractPageState(document);
    expect(res.status).toBe('needs_attention');
    expect(res.contextSnippet).toContain('Glyim');
  });

  test('détecte la bannière d’interruption service z.ai ("The service was updated while this task was running")', () => {
    document.title = 'Oxide';
    document.body.innerHTML = `
      <main>
        <div data-user-bubble="true">Audit workspace crate</div>
        <div class="flex my-2 gap-2.5 border px-4 py-3 border-red-600/10 bg-red-600/10 rounded-lg">
          <div class="self-start mt-0.5"><svg class="remixicon size-5 text-red-700"></svg></div>
          <div class="self-center text-sm">The service was updated while this task was running, so it was interrupted. Please send your request again.</div>
        </div>
      </main>
    `;

    const res = extractPageState(document);
    expect(res.status).toBe('needs_attention');
    expect(res.contextSnippet).toContain('Oxide');
  });

  test('efface l’erreur dès qu’un nouveau message utilisateur relance la conversation', () => {
    document.title = 'Glyim';
    document.body.innerHTML = `
      <main>
        <div data-user-bubble="true">First attempt</div>
        <div class="border-red-600/10 bg-red-600/10">The service was updated while this task was running, so it was interrupted.</div>
        <div data-user-bubble="true">continue working</div>
        <button aria-label="Stop"></button>
      </main>
    `;

    const res = extractPageState(document);
    expect(res.status).toBe('working');
  });

  test('ignore un captcha ou une erreur masquée (display: none) après résolution', () => {
    document.title = 'Task: context-braid | Z.ai';
    document.body.innerHTML = `
      <div id="aliyunCaptcha-float-wrapper" style="display: none;"></div>
      <main>
        <div data-user-bubble="true">New conversation</div>
        <div class="border-red-600/10" style="display: none;">Oops, something went wrong. Please refresh.</div>
      </main>
    `;

    expect(extractPageState(document).status).toBe('idle');
  });

  test('extracts semantic z.ai agent-thinking-label step while working and data-sandbox-file zip when completed', () => {
    document.body.innerHTML = `
      <main>
        <div data-user-bubble="true">Task: ://github.com. Deliver ship.toml.</div>
        <div role="status" data-slot="agent-thinking">
          <span data-slot="agent-thinking-label">Re-run measurement (12 programs)</span>
        </div>
        <button aria-label="Stop">Stop</button>
      </main>
    `;
    const working = extractPageState(document);
    expect(working.status).toBe('working');
    expect(working.contextSnippet.startsWith('Re-run measurement (12 programs) — ')).toBe(true);

    document.body.innerHTML = `
      <main>
        <div data-user-bubble="true">Task: ://github.com. Deliver ship.toml.</div>
        <a href="/home/z/my-project/download/glyim-m2.zip" data-sandbox-file="download/glyim-m2.zip">glyim-m2.zip</a>
      </main>
    `;
    const done = extractPageState(document);
    expect(done.status).toBe('completed');
  });
});
