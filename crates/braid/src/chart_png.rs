use anyhow::Result;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy)]
struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    alpha: f32,
}

type Pt = (f32, f32);
type CurveData = (Vec<Pt>, Vec<Pt>, f32);

fn f32_to_i32(val: f32) -> i32 {
    if !val.is_finite() {
        return 0;
    }
    let clamped = val.round().clamp(-32_768.0, 32_767.0);
    let negative = clamped < 0.0;
    let abs_bits = clamped.abs().to_bits();
    let exp = i32::try_from((abs_bits >> 23) & 0xff).unwrap_or(0) - 127;
    if exp < 0 {
        return 0;
    }
    let mantissa = (abs_bits & 0x007f_ffff) | 0x0080_0000;
    let shift = 23 - exp;
    let mag = if shift >= 0 {
        mantissa >> u32::try_from(shift).unwrap_or(0)
    } else {
        mantissa << u32::try_from(-shift).unwrap_or(0)
    };
    let signed = i32::try_from(mag).unwrap_or(0);
    if negative { -signed } else { signed }
}

fn f32_to_u8(val: f32) -> u8 {
    u8::try_from(f32_to_i32(val.clamp(0.0, 255.0))).unwrap_or(0)
}

fn usize_to_i32(val: usize) -> i32 {
    i32::try_from(val).unwrap_or(i32::MAX)
}

fn usize_to_f32(val: usize) -> f32 {
    f32::from(u16::try_from(val).unwrap_or(u16::MAX))
}

fn i32_to_f32(val: i32) -> f32 {
    f32::from(i16::try_from(val.clamp(-32_768, 32_767)).unwrap_or(0))
}

fn u32_to_f32(val: u32) -> f32 {
    f32::from(u16::try_from(val).unwrap_or(u16::MAX))
}

struct Canvas<'a> {
    buf: &'a mut [u8],
    w: usize,
    h: usize,
}

impl Canvas<'_> {
    fn blend_pixel(&mut self, px: i32, py: i32, color: Rgba) {
        let Ok(ux) = usize::try_from(px) else { return };
        let Ok(uy) = usize::try_from(py) else { return };
        if ux >= self.w || uy >= self.h || color.alpha <= 0.001 {
            return;
        }
        let idx = (uy * self.w + ux) * 4;
        let src_a = color.alpha.clamp(0.0, 1.0);
        let dst_a = f32::from(self.buf[idx + 3]) / 255.0;
        let out_a = src_a + dst_a * (1.0 - src_a);
        if out_a <= 0.0001 {
            return;
        }
        let inv = dst_a * (1.0 - src_a);
        let red = (f32::from(color.r) * src_a + f32::from(self.buf[idx]) * inv) / out_a;
        let green = (f32::from(color.g) * src_a + f32::from(self.buf[idx + 1]) * inv) / out_a;
        let blue = (f32::from(color.b) * src_a + f32::from(self.buf[idx + 2]) * inv) / out_a;
        self.buf[idx] = f32_to_u8(red);
        self.buf[idx + 1] = f32_to_u8(green);
        self.buf[idx + 2] = f32_to_u8(blue);
        self.buf[idx + 3] = f32_to_u8(out_a * 255.0);
    }

    fn draw_micro_label(&mut self, mut px: i32, py: i32, text: &str, color: Rgba) {
        const GLYPHS: &[(char, [u8; 5])] = &[
            ('0', [0b111, 0b101, 0b101, 0b101, 0b111]),
            ('1', [0b010, 0b110, 0b010, 0b010, 0b111]),
            ('2', [0b111, 0b001, 0b111, 0b100, 0b111]),
            ('3', [0b111, 0b001, 0b111, 0b001, 0b111]),
            ('4', [0b101, 0b101, 0b111, 0b001, 0b001]),
            ('5', [0b111, 0b100, 0b111, 0b001, 0b111]),
            ('6', [0b111, 0b100, 0b111, 0b101, 0b111]),
            ('7', [0b111, 0b001, 0b010, 0b010, 0b010]),
            ('8', [0b111, 0b101, 0b111, 0b101, 0b111]),
            ('9', [0b111, 0b101, 0b111, 0b001, 0b111]),
            ('A', [0b010, 0b101, 0b111, 0b101, 0b101]),
            ('D', [0b110, 0b101, 0b101, 0b101, 0b110]),
            ('E', [0b111, 0b100, 0b110, 0b100, 0b111]),
            ('I', [0b111, 0b010, 0b010, 0b010, 0b111]),
            ('K', [0b101, 0b101, 0b110, 0b101, 0b101]),
            ('L', [0b100, 0b100, 0b100, 0b100, 0b111]),
            ('P', [0b110, 0b101, 0b110, 0b100, 0b100]),
            ('/', [0b001, 0b001, 0b010, 0b100, 0b100]),
            (' ', [0b000, 0b000, 0b000, 0b000, 0b000]),
        ];
        let scale = 2;
        for ch in text.chars() {
            if let Some((_, rows)) = GLYPHS.iter().find(|(glyph, _)| *glyph == ch) {
                for (ry, bits) in rows.iter().enumerate() {
                    let row_off = usize_to_i32(ry) * scale;
                    for rx in 0..3 {
                        if (bits >> (2 - rx)) & 1 == 1 {
                            for sy in 0..scale {
                                for sx in 0..scale {
                                    self.blend_pixel(
                                        px + rx * scale + sx,
                                        py + row_off + sy,
                                        color,
                                    );
                                }
                            }
                        }
                    }
                }
                px += 4 * scale;
            }
        }
    }

    fn draw_disc_aa(&mut self, cx: f32, cy: f32, radius: f32, color: Rgba) {
        let min_x = f32_to_i32((cx - radius - 1.5).floor());
        let max_x = f32_to_i32((cx + radius + 1.5).ceil());
        let min_y = f32_to_i32((cy - radius - 1.5).floor());
        let max_y = f32_to_i32((cy + radius + 1.5).ceil());
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let dx = (i32_to_f32(px) + 0.5) - cx;
                let dy = (i32_to_f32(py) + 0.5) - cy;
                let cov = (radius + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend_pixel(
                        px,
                        py,
                        Rgba {
                            alpha: color.alpha * cov,
                            ..color
                        },
                    );
                }
            }
        }
    }

    fn draw_segment_aa(&mut self, p0: Pt, p1: Pt, half_w: f32, color: Rgba) {
        let (x0, y0) = p0;
        let (x1, y1) = p1;
        let min_x = f32_to_i32((x0.min(x1) - half_w - 2.0).floor());
        let max_x = f32_to_i32((x0.max(x1) + half_w + 2.0).ceil());
        let min_y = f32_to_i32((y0.min(y1) - half_w - 2.0).floor());
        let max_y = f32_to_i32((y0.max(y1) + half_w + 2.0).ceil());
        let vx = x1 - x0;
        let vy = y1 - y0;
        let len_sq = vx * vx + vy * vy;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let fx = i32_to_f32(px) + 0.5;
                let fy = i32_to_f32(py) + 0.5;
                let ratio = if len_sq > 1e-6 {
                    (((fx - x0) * vx + (fy - y0) * vy) / len_sq).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let dist =
                    ((fx - (x0 + ratio * vx)).powi(2) + (fy - (y0 + ratio * vy)).powi(2)).sqrt();
                let cov = (half_w + 0.65 - dist).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend_pixel(
                        px,
                        py,
                        Rgba {
                            alpha: color.alpha * cov,
                            ..color
                        },
                    );
                }
            }
        }
    }
}

fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, ratio: f32) -> f32 {
    let r2 = ratio * ratio;
    let r3 = r2 * ratio;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * ratio
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * r2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * r3)
}

fn adler32(data: &[u8]) -> u32 {
    let mut sum_a: u32 = 1;
    let mut sum_b: u32 = 0;
    for &byte in data {
        sum_a = (sum_a + u32::from(byte)) % 65_521;
        sum_b = (sum_b + sum_a) % 65_521;
    }
    (sum_b << 16) | sum_a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if (crc & 1) != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn write_png_chunk(out: &mut Vec<u8>, tag: [u8; 4], data: &[u8]) {
    let len_u32 = u32::try_from(data.len()).unwrap_or(0);
    out.extend_from_slice(&len_u32.to_be_bytes());
    out.extend_from_slice(&tag);
    out.extend_from_slice(data);
    let mut crc_buf = Vec::with_capacity(4 + data.len());
    crc_buf.extend_from_slice(&tag);
    crc_buf.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_buf).to_be_bytes());
}

fn write_rgba_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    let uw = usize::try_from(width)?;
    let uh = usize::try_from(height)?;
    let stride = uw * 4;
    let mut raw = Vec::with_capacity((stride + 1) * uh);
    for row in 0..uh {
        raw.push(0);
        raw.extend_from_slice(&rgba[row * stride..(row + 1) * stride]);
    }

    let mut zlib = Vec::with_capacity(raw.len() + 64);
    zlib.extend_from_slice(&[0x78, 0x01]);
    let mut pos = 0;
    while pos < raw.len() {
        let chunk_len = (raw.len() - pos).min(65_535);
        zlib.push(u8::from(pos + chunk_len == raw.len()));
        let len_u16 = u16::try_from(chunk_len)?;
        zlib.extend_from_slice(&len_u16.to_le_bytes());
        zlib.extend_from_slice(&(!len_u16).to_le_bytes());
        zlib.extend_from_slice(&raw[pos..pos + chunk_len]);
        pos += chunk_len;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(&u64::to_be_bytes(0x8950_4e47_0d0a_1a0a));
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_png_chunk(&mut png, *b"IHDR", &ihdr);
    write_png_chunk(&mut png, *b"IDAT", &zlib);
    write_png_chunk(&mut png, *b"IEND", &[]);

    fs::write(path, png)?;
    Ok(())
}

fn build_chart_curve(counts: &[u32], width: usize, height: usize) -> CurveData {
    let data: Vec<f32> = if counts.is_empty() {
        vec![0.0; 30]
    } else {
        counts.iter().map(|&val| u32_to_f32(val)).collect()
    };
    let max_val = data.iter().copied().fold(1.0_f32, f32::max);
    let pad_x = 12.0_f32;
    let pad_top = 28.0_f32;
    let pad_bot = 8.0_f32;
    let plot_w = usize_to_f32(width) - pad_x * 2.0;
    let plot_h = usize_to_f32(height) - pad_top - pad_bot;
    let base_y = usize_to_f32(height) - pad_bot;
    let count = data.len();
    let denom = usize_to_f32(count.saturating_sub(1).max(1));
    let anchors: Vec<Pt> = data
        .iter()
        .enumerate()
        .map(|(idx, &val)| {
            let ax = pad_x + (usize_to_f32(idx) / denom) * plot_w;
            let ay = base_y - (val / max_val).sqrt() * plot_h;
            (ax, ay)
        })
        .collect();
    let mut curve: Vec<Pt> = Vec::with_capacity(count * 16);
    for idx in 0..count.saturating_sub(1) {
        let p0 = anchors[idx.saturating_sub(1)];
        let p1 = anchors[idx];
        let p2 = anchors[idx + 1];
        let p3 = anchors[(idx + 2).min(count - 1)];
        for step in 0..=16 {
            if idx > 0 && step == 0 {
                continue;
            }
            let ratio = i32_to_f32(step) / 16.0;
            let cx = p1.0 + (p2.0 - p1.0) * ratio;
            let cy = catmull_rom(p0.1, p1.1, p2.1, p3.1, ratio).clamp(pad_top - 2.0, base_y);
            curve.push((cx, cy));
        }
    }
    (anchors, curve, base_y)
}

fn paint_curve_fill_and_stroke(
    canvas: &mut Canvas<'_>,
    curve: &[Pt],
    base_y: f32,
    accent: (u8, u8, u8),
) {
    let (ar, ag, ab) = accent;
    let end_y = f32_to_i32(base_y);
    for idx in 0..curve.len().saturating_sub(1) {
        let (x0, y0) = curve[idx];
        let (x1, y1) = curve[idx + 1];
        for px in f32_to_i32(x0.floor())..=f32_to_i32(x1.ceil()) {
            let ratio = if (x1 - x0).abs() < 1e-4 {
                0.0
            } else {
                ((i32_to_f32(px) - x0) / (x1 - x0)).clamp(0.0, 1.0)
            };
            let start_y = f32_to_i32((y0 + ratio * (y1 - y0)).ceil());
            let span = i32_to_f32((end_y - start_y).max(1));
            for py in start_y..=end_y {
                let rel = 1.0 - (i32_to_f32(py - start_y) / span);
                canvas.blend_pixel(
                    px,
                    py,
                    Rgba {
                        r: ar,
                        g: ag,
                        b: ab,
                        alpha: 0.07 * rel.powf(1.8),
                    },
                );
            }
        }
    }
    let stroke = Rgba {
        r: ar,
        g: ag,
        b: ab,
        alpha: 0.88,
    };
    for idx in 0..curve.len().saturating_sub(1) {
        canvas.draw_segment_aa(curve[idx], curve[idx + 1], 1.85, stroke);
    }
}

fn paint_chart_overlays(
    canvas: &mut Canvas<'_>,
    counts: &[u32],
    anchors: &[Pt],
    width: usize,
    height: usize,
    accent: (u8, u8, u8),
) {
    let (ar, ag, ab) = accent;
    let rgba = |r, g, b, alpha| Rgba { r, g, b, alpha };
    let neutral_line = rgba(132, 136, 144, 0.22);
    let neutral_text = rgba(120, 125, 134, 0.80);
    let tick_color = rgba(132, 136, 144, 0.32);
    let base_row = usize_to_i32(height.saturating_sub(6));
    let left_x = anchors.first().map_or(10, |pt| f32_to_i32(pt.0));
    let right_x = anchors.last().map_or_else(
        || usize_to_i32(width.saturating_sub(10)),
        |pt| f32_to_i32(pt.0),
    );
    for gx in (left_x..=right_x).step_by(6) {
        canvas.blend_pixel(gx, base_row, neutral_line);
        canvas.blend_pixel(gx + 1, base_row, neutral_line);
    }
    for step in 0..=4 {
        let tx = left_x + ((right_x - left_x) * step) / 4;
        for ty in (base_row - 3)..=base_row {
            canvas.blend_pixel(tx, ty, tick_color);
        }
    }
    let max_val = counts.iter().copied().max().unwrap_or(0);
    if max_val > 0
        && let Some((peak_idx, _)) = counts.iter().enumerate().max_by_key(|&(_idx, &val)| val)
        && let Some(&(px, py)) = anchors.get(peak_idx)
    {
        let pxi = f32_to_i32(px);
        for dy in ((f32_to_i32(py) + 5)..(base_row - 2)).step_by(4) {
            canvas.blend_pixel(pxi, dy, rgba(ar, ag, ab, 0.24));
        }
        canvas.draw_disc_aa(px, py, 2.0, rgba(ar, ag, ab, 0.85));
    }
    canvas.draw_micro_label(left_x, 5, "30D", neutral_text);
    let peak_label = if max_val > 0 {
        format!("PEAK {max_val}/D")
    } else {
        "IDLE".to_string()
    };
    let label_w = usize_to_i32(peak_label.len()) * 8;
    canvas.draw_micro_label(
        (right_x - label_w).max(left_x + 40),
        5,
        &peak_label,
        neutral_text,
    );
    if let Some(&(lx, ly)) = anchors.last() {
        canvas.draw_disc_aa(lx, ly, 6.0, rgba(ar, ag, ab, 0.14));
        canvas.draw_disc_aa(lx, ly, 2.6, rgba(ar, ag, ab, 0.95));
    }
}

pub fn render_velocity_chart_png(
    repo_name: &str,
    counts: &[u32],
    accent: (u8, u8, u8),
) -> Option<String> {
    let dir = Path::new("/tmp/contextbraid-charts");
    fs::create_dir_all(dir).ok()?;

    let safe_name: String = repo_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == char::from(95) {
                ch
            } else {
                char::from(95)
            }
        })
        .collect();

    let mut sig: u64 = 0xcbf2_9ce4_8422_2325;
    for &cnt in counts {
        sig ^= u64::from(cnt);
        sig = sig.wrapping_mul(0x0100_0000_01b3);
    }
    sig ^= (u64::from(accent.0) << 16) | (u64::from(accent.1) << 8) | u64::from(accent.2);
    let file_name = format!(
        "{safe_name}_v3_{:08x}.png",
        u32::try_from(sig & 0xffff_ffff).ok()?
    );
    let out_path = dir.join(&file_name);

    if out_path.exists() {
        return Some(out_path.to_string_lossy().into_owned());
    }

    let prefix = format!("{safe_name}_");
    let legacy = format!("{safe_name}.png");
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().into_owned();
            if (fname.starts_with(&prefix) || fname == legacy) && fname != file_name {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    let width: usize = 720;
    let height: usize = 100;
    let mut buf = vec![0u8; width * height * 4];
    let mut canvas = Canvas {
        buf: &mut buf,
        w: width,
        h: height,
    };
    let (anchors, curve, base_y) = build_chart_curve(counts, width, height);

    for &ratio in &[0.0_f32, 0.5_f32] {
        let gy = f32_to_i32(base_y - ratio * (usize_to_f32(height) - 36.0));
        for gx in (12..708).step_by(6) {
            canvas.blend_pixel(
                gx,
                gy,
                Rgba {
                    r: 142,
                    g: 142,
                    b: 147,
                    alpha: 0.09,
                },
            );
        }
    }
    paint_curve_fill_and_stroke(&mut canvas, &curve, base_y, accent);
    paint_chart_overlays(&mut canvas, counts, &anchors, width, height, accent);

    write_rgba_png(
        &out_path,
        u32::try_from(width).ok()?,
        u32::try_from(height).ok()?,
        &buf,
    )
    .ok()?;
    Some(out_path.to_string_lossy().into_owned())
}
