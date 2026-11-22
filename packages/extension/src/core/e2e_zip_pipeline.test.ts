import { describe, expect, it } from 'vitest';
import { JSDOM } from 'jsdom';
import { extractPageState } from './extractor';

describe('E2E Extension DOM + Multi-ZIP lifecycle', () => {
  it('détecte working -> completed -> premier ZIP puis second ZIP dans le même onglet', () => {
    const dom1 = new JSDOM(`
      <main>
        <div data-user-bubble="true">Build archive v1</div>
        <div data-scope-id="turn">
          <span data-slot="agent-thinking-label">Generating bundle...</span>
        </div>
      </main>
    `);
    const s1 = extractPageState(dom1.window.document);
    expect(s1.status).toBe('working');
    expect(s1.zipDownloadUrl).toBeUndefined();

    const dom2 = new JSDOM(`
      <main>
        <div data-user-bubble="true">Build archive v1</div>
        <div data-scope-id="turn">
          <p>Done!</p>
          <a href="https://z.ai" download="bundle-v1.zip">Download ZIP</a>
        </div>
      </main>
    `);
    const s2 = extractPageState(dom2.window.document);
    expect(s2.status).toBe('completed');
    expect(s2.zipDownloadUrl).toBe('https://z.ai');

    // Second tour dans le même onglet : nouvel artefact ZIP
    const dom3 = new JSDOM(`
      <main>
        <div data-user-bubble="true">Build archive v1</div>
        <div data-scope-id="turn">
          <a href="https://z.ai" download="bundle-v1.zip">Download ZIP</a>
        </div>
        <div data-user-bubble="true">Build archive v2</div>
        <div data-scope-id="turn">
          <p>Updated archive ready.</p>
          <a href="https://z.ai" download="bundle-v2.zip">Download ZIP v2</a>
        </div>
      </main>
    `);
    const s3 = extractPageState(dom3.window.document);
    expect(s3.status).toBe('completed');
    expect(s3.zipDownloadUrl).toBe('https://z.ai');
  });
});
