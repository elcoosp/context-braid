export function buildGitVelocitySvg(counts: number[], accentHex = '#3B82F6', label = '14d Git Velocity'): string {
  const data = counts && counts.length === 14 ? counts : new Array(14).fill(0);
  const total = data.reduce((a, b) => a + b, 0);
  const maxVal = Math.max(1, ...data);

  const w = 360;
  const h = 88;
  const padX = 12;
  const padTop = 26;
  const padBot = 14;
  const plotW = w - padX * 2;
  const plotH = h - padTop - padBot;

  const pts = data.map((v, i) => {
    const x = padX + (i / (data.length - 1)) * plotW;
    const y = padTop + plotH - (v / maxVal) * plotH;
    return { x, y, v };
  });

  // Courbe lissée Catmull-Rom / Bézier cubique
  let linePath = `M ${pts[0].x.toFixed(1)} ${pts[0].y.toFixed(1)}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[i];
    const p1 = pts[i + 1];
    const cx = ((p0.x + p1.x) / 2).toFixed(1);
    linePath += ` C ${cx} ${p0.y.toFixed(1)}, ${cx} ${p1.y.toFixed(1)}, ${p1.x.toFixed(1)} ${p1.y.toFixed(1)}`;
  }

  const areaPath = `${linePath} L ${pts[pts.length - 1].x.toFixed(1)} ${(h - padBot).toFixed(1)} L ${pts[0].x.toFixed(1)} ${(h - padBot).toFixed(1)} Z`;
  const last = pts[pts.length - 1];

  const dots = pts
    .filter((p) => p.v > 0)
    .map(
      (p) =>
        `<circle cx="${p.x.toFixed(1)}" cy="${p.y.toFixed(1)}" r="2.6" fill="${accentHex}" />`
    )
    .join('');

  const svg = `<svg xmlns="http://w3.org" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">
    <defs>
      <linearGradient id="velGrad" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stop-color="${accentHex}" stop-opacity="0.28"/>
        <stop offset="100%" stop-color="${accentHex}" stop-opacity="0.0"/>
      </linearGradient>
    </defs>
    <rect x="0.5" y="0.5" width="${w - 1}" height="${h - 1}" rx="10" fill="#888888" fill-opacity="0.07" stroke="#888888" stroke-opacity="0.16"/>
    <text x="12" y="17" font-family="-apple-system, BlinkMacSystemFont, sans-serif" font-size="10.5" font-weight="600" fill="#8E8E93">${label}</text>
    <text x="${w - 12}" y="17" text-anchor="end" font-family="-apple-system, BlinkMacSystemFont, sans-serif" font-size="10.5" font-weight="600" fill="${accentHex}">${total} commits</text>
    <line x1="${padX}" y1="${h - padBot}" x2="${w - padX}" y2="${h - padBot}" stroke="#888888" stroke-opacity="0.15" stroke-dasharray="2 3"/>
    <path d="${areaPath}" fill="url(#velGrad)"/>
    <path d="${linePath}" fill="none" stroke="${accentHex}" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
    ${dots}
    <circle cx="${last.x.toFixed(1)}" cy="${last.y.toFixed(1)}" r="4.5" fill="${accentHex}" fill-opacity="0.22"/>
    <circle cx="${last.x.toFixed(1)}" cy="${last.y.toFixed(1)}" r="2.8" fill="${accentHex}"/>
  </svg>`;

  return `data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`;
}
