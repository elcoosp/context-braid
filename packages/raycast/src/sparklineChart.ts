const BARS = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

export function buildGitVelocityBar(counts: number[]): { spark: string; total: number; peak: number } {
  const data = counts && counts.length === 14 ? counts : new Array(14).fill(0);
  const total = data.reduce((a, b) => a + b, 0);
  const peak = Math.max(0, ...data);
  if (peak === 0) {
    return { spark: '▁'.repeat(14), total: 0, peak: 0 };
  }
  const spark = data
    .map((v) => {
      if (v <= 0) return '▁';
      const idx = Math.min(BARS.length - 1, Math.max(1, Math.round((v / peak) * (BARS.length - 1))));
      return BARS[idx];
    })
    .join('');
  return { spark, total, peak };
}
