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
});
