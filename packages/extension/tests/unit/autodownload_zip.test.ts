import { describe, expect, it, vi } from 'vitest';
import { parseHTML } from 'linkedom';
import {
  findZipDownloadTargets,
  isDirectZipDownloadUrl,
  validateZipDownloadPayload,
} from '../../src/core/extractor';

describe('TDD Auto-Downloader ZIP — Reproduction du faux ZIP HTML 18ko', () => {
  it('rejette les liens sandbox internes /home/z/.../*.zip comme URL de téléchargement direct', () => {
    const { document } = parseHTML(`
      <!doctype html>
      <html>
        <body>
          <main>
            <div data-user-bubble="true">Deliver ship.toml AND THE ZIP.</div>
            <div data-scope-id="turn">
              <a href="https://z.ai" data-sandbox-file="download/glyim-m4-oracles-m5-phase-extract.zip">
                glyim-m4-oracles-m5-phase-extract.zip
              </a>
            </div>
            <ul>
              <li class="group/item">
                <span>glyim-m4-oracles-m5-phase-extract</span>
                <span>· zip</span>
                <button title="Download file">Download</button>
              </li>
            </ul>
          </main>
        </body>
      </html>
    `);

    const targets = findZipDownloadTargets(document as unknown as Document);
    // Aucune cible ne doit exposer l'URL SPA interne /home/z/... comme URL HTTP téléchargeable directement
    for (const t of targets) {
      if (t.url) {
        expect(isDirectZipDownloadUrl(t.url)).toBe(false);
      }
    }
    // Le bouton de téléchargement de la liste `· zip` doit bien être conservé comme cible DOM
    expect(targets.some((t) => t.url === '' && t.element.tagName === 'BUTTON')).toBe(true);
  });

  it('rejette une réponse HTTP HTML SPA (18392 octets <!doctype html>) et accepte un vrai binaire ZIP (PK\\x03\\x04)', () => {
    const htmlSpaResponse = new TextEncoder().encode(
      '<!doctype html>\n<html lang="en">\n\t<head>\n\t\t<meta charset="utf-8" />',
    );
    const fakeCheck = validateZipDownloadPayload({
      url: 'https://z.ai',
      mime: 'text/html; charset=utf-8',
      firstBytes: htmlSpaResponse,
    });
    expect(fakeCheck.valid).toBe(false);
    expect(fakeCheck.reason).toContain('html');

    const realZipBytes = new Uint8Array([0x50, 0x4b, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00]);
    const validCheck = validateZipDownloadPayload({
      url: 'https://chatglm.cn',
      mime: 'application/zip',
      firstBytes: realZipBytes,
    });
    expect(validCheck.valid).toBe(true);
  });
});
