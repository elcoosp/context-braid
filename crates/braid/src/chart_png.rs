use anyhow::Result;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy)]
struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    alpha: f32,
}

struct Canvas<'a> {
    buf: &'a mut [u8],
    w: usize,
    h: usize,
}

impl<'a> Canvas<'a> {
    fn blend_pixel(&mut self, x: i32, y: i32, color: Rgba) {
        if x < 0 || y < 0 || (x as usize) >= self.w || (y as usize) >= self.h || color.alpha <= 0.001 {
            return;
        }
        let idx = ((y as usize) * self.w + (x as usize)) * 4;
        let src_a = color.alpha.clamp(0.0, 1.0);
        let dst_a = self.buf[idx + 3] as f32 / 255.0;
        let out_a = src_a + dst_a * (1.0 - src_a);
        if out_a <= 0.0001 {
            return;
        }
        let r = (color.r as f32 * src_a + self.buf[idx] as f32 * dst_a * (1.0 - src_a)) / out_a;
        let g = (color.g as f32 * src_a + self.buf[idx + 1] as f32 * dst_a * (1.0 - src_a)) / out_a;
        let b = (color.b as f32 * src_a + self.buf[idx + 2] as f32 * dst_a * (1.0 - src_a)) / out_a;
        self.buf[idx] = r.round().clamp(0.0, 255.0) as u8;
        self.buf[idx + 1] = g.round().clamp(0.0, 255.0) as u8;
        self.buf[idx + 2] = b.round().clamp(0.0, 255.0) as u8;
        self.buf[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    fn draw_micro_label(&mut self, mut x: i32, y: i32, text: &str, color: Rgba) {
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
            ('M', [0b101, 0b111, 0b101, 0b101, 0b101]),
            ('P', [0b110, 0b101, 0b110, 0b100, 0b100]),
            ('K', [0b101, 0b101, 0b110, 0b101, 0b101]),
            ('X', [0b101, 0b101, 0b010, 0b101, 0b101]),
            ('/', [0b001, 0b001, 0b010, 0b100, 0b100]),
            ('-', [0b000, 0b000, 0b111, 0b000, 0b000]),
            (' ', [0b000, 0b000, 0b000, 0b000, 0b000]),
        ];
        let scale = 2;
        for ch in text.chars() {
            if let Some((_, rows)) = GLYPHS.iter().find(|(c, _)| *c == ch) {
                for (ry, bits) in rows.iter().enumerate() {
                    for rx in 0..3 {
                        if (bits >> (2 - rx)) & 1 == 1 {
                            for sy in 0..scale {
                                for sx in 0..scale {
                                    self.blend_pixel(x + rx * scale + sx, y + (ry as i32) * scale + sy, color);
                                }
                            }
                        }
                    }
                }
                x += 4 * scale;
            }
        }
    }

    fn draw_disc_aa(&mut self, cx: f32, cy: f32, radius: f32, color: Rgba) {
        let min_x = (cx - radius - 1.5).floor() as i32;
        let max_x = (cx + radius + 1.5).ceil() as i32;
        let min_y = (cy - radius - 1.5).floor() as i32;
        let max_y = (cy + radius + 1.5).ceil() as i32;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = (x as f32 + 0.5) - cx;
                let dy = (y as f32 + 0.5) - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                let cov = (radius + 0.5 - dist).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend_pixel(x, y, Rgba { alpha: color.alpha * cov, ..color });
                }
            }
        }
    }

    fn draw_segment_aa(&mut self, p0: (f32, f32), p1: (f32, f32), half_w: f32, color: Rgba) {
        let (x0, y0) = p0;
        let (x1, y1) = p1;
        let min_x = (x0.min(x1) - half_w - 2.0).floor() as i32;
        let max_x = (x0.max(x1) + half_w + 2.0).ceil() as i32;
        let min_y = (y0.min(y1) - half_w - 2.0).floor() as i32;
        let max_y = (y0.max(y1) + half_w + 2.0).ceil() as i32;
        let vx = x1 - x0;
        let vy = y1 - y0;
        let len_sq = vx * vx + vy * vy;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let t = if len_sq > 1e-6 {
                    (((px - x0) * vx + (py - y0) * vy) / len_sq).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let proj_x = x0 + t * vx;
                let proj_y = y0 + t * vy;
                let dist = ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt();
                let cov = (half_w + 0.65 - dist).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend_pixel(x, y, Rgba { alpha: color.alpha * cov, ..color });
                }
            }
        }
    }
}

fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
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

fn write_png_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc_buf = Vec::with_capacity(4 + data.len());
    crc_buf.extend_from_slice(tag);
    crc_buf.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_buf).to_be_bytes());
}

fn write_rgba_png(path: &PathBuf, w: u32, h: u32, rgba: &[u8]) -> Result<()> {
    let stride = (w as usize) * 4;
    let mut raw = Vec::with_capacity((stride + 1) * (h as usize));
    for y in 0..(h as usize) {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * stride..(y + 1) * stride]);
    }

    let mut zlib = Vec::with_capacity(raw.len() + 64);
    zlib.push(0x78);
    zlib.push(0x01);
    let mut pos = 0;
    while pos < raw.len() {
        let chunk_len = (raw.len() - pos).min(65535);
        let is_final = if pos + chunk_len == raw.len() { 1u8 } else { 0u8 };
        zlib.push(is_final);
        let len_u16 = chunk_len as u16;
        zlib.extend_from_slice(&len_u16.to_le_bytes());
        zlib.extend_from_slice(&(!len_u16).to_le_bytes());
        zlib.extend_from_slice(&raw[pos..pos + chunk_len]);
        pos += chunk_len;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.push(8);
    ihdr.push(6);
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    write_png_chunk(&mut png, b"IHDR", &ihdr);
    write_png_chunk(&mut png, b"IDAT", &zlib);
    write_png_chunk(&mut png, b"IEND", &[]);

    fs::write(path, png)?;
    Ok(())
}

pub fn render_velocity_chart_png(repo_name: &str, counts: &[u32], accent: (u8, u8, u8)) -> Option<String> {
    let dir = PathBuf::from("/tmp/contextbraid-charts");
    fs::create_dir_all(&dir).ok()?;

    let safe_name: String = repo_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();

    // Content-addressed hash so WebKit never displays stale cached images
    let mut sig: u64 = 0xcbf29ce484222325;
    for &c in counts {
        sig ^= c as u64;
        sig = sig.wrapping_mul(0x100000001b3);
    }
    sig ^= ((accent.0 as u64) << 16) | ((accent.1 as u64) << 8) | (accent.2 as u64);
    let file_name = format!("{}_v3_{:08x}.png", safe_name, (sig & 0xffff_ffff) as u32);
    let out_path = dir.join(&file_name);

    if out_path.exists() {
        return Some(out_path.to_string_lossy().into_owned());
    }

    // Remove older chart files for this repository
    let prefix = format!("{}_", safe_name);
    let legacy = format!("{}.png", safe_name);
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().into_owned();
            if (fname.starts_with(&prefix) || fname == legacy) && fname != file_name {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    let w: usize = 720;
    let h: usize = 100;
    if out_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Some(out_path.to_string_lossy().into_owned());
    }
    let mut buf = vec![0u8; w * h * 4];
    let mut canvas = Canvas { buf: &mut buf, w, h };

    let data: Vec<f32> = if counts.is_empty() {
        vec![0.0; 30]
    } else {
        counts.iter().map(|&v| v as f32).collect()
    };
    let max_val = data.iter().cloned().fold(1.0_f32, f32::max);

    let pad_x = 12.0_f32;
    let pad_top = 8.0_f32;
    let pad_bot = 8.0_f32;
    let plot_w = w as f32 - pad_x * 2.0;
    let plot_h = h as f32 - pad_top - pad_bot;
    let base_y = h as f32 - pad_bot;

    for &ratio in &[0.0_f32, 0.5_f32] {
        let gy = (base_y - ratio * plot_h).round() as i32;
        for gx in (pad_x as i32)..((w as f32 - pad_x) as i32) {
            if (gx / 6) % 2 == 0 {
                canvas.blend_pixel(gx, gy, Rgba { r: 142, g: 142, b: 147, alpha: 0.09 });
            }
        }
    }

    let n = data.len();
    let anchors: Vec<(f32, f32)> = data
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = pad_x + (i as f32 / (n.saturating_sub(1).max(1) as f32)) * plot_w;
            let norm = (v / max_val).sqrt();
            let y = base_y - norm * plot_h;
            (x, y)
        })
        .collect();

    let mut curve: Vec<(f32, f32)> = Vec::with_capacity(n * 16);
    for i in 0..n.saturating_sub(1) {
        let p0 = anchors[i.saturating_sub(1)];
        let p1 = anchors[i];
        let p2 = anchors[i + 1];
        let p3 = anchors[(i + 2).min(n - 1)];
        let steps = 16;
        for s in 0..=steps {
            if i > 0 && s == 0 {
                continue;
            }
            let t = s as f32 / steps as f32;
            let cx = p1.0 + (p2.0 - p1.0) * t;
            let cy = catmull_rom(p0.1, p1.1, p2.1, p3.1, t).clamp(pad_top - 2.0, base_y);
            curve.push((cx, cy));
        }
    }

    let (ar, ag, ab) = accent;

    for idx in 0..curve.len().saturating_sub(1) {
        let (x0, y0) = curve[idx];
        let (x1, y1) = curve[idx + 1];
        let ix0 = x0.floor() as i32;
        let ix1 = x1.ceil() as i32;
        for px in ix0..=ix1 {
            let t = if (x1 - x0).abs() < 1e-4 {
                0.0
            } else {
                ((px as f32 - x0) / (x1 - x0)).clamp(0.0, 1.0)
            };
            let cy = y0 + t * (y1 - y0);
            let start_y = cy.ceil() as i32;
            let end_y = base_y as i32;
            let span = (end_y - start_y).max(1) as f32;
            for py in start_y..=end_y {
                let rel: f32 = 1.0 - ((py - start_y) as f32 / span);
                let fill_alpha = 0.07 * rel.powf(1.8);
                canvas.blend_pixel(px, py, Rgba { r: ar, g: ag, b: ab, alpha: fill_alpha });
            }
        }
    }

    for idx in 0..curve.len().saturating_sub(1) {
        let (x0, y0) = curve[idx];
        let (x1, y1) = curve[idx + 1];
        canvas.draw_segment_aa((x0, y0), (x1, y1), 1.85, Rgba { r: ar, g: ag, b: ab, alpha: 0.88 });
    }

    let neutral_line = Rgba { r: 132, g: 136, b: 144, alpha: 0.22 };
    let neutral_text = Rgba { r: 120, g: 125, b: 134, alpha: 0.80 };
    let base_row = (h - 6) as i32;
    let left_x = anchors.first().map(|p| p.0.round() as i32).unwrap_or(10);
    let right_x = anchors.last().map(|p| p.0.round() as i32).unwrap_or((w - 10) as i32);

    // Ligne de base micro-pointillée + 4 repères hebdomadaires discrets
    let mut gx = left_x;
    while gx <= right_x {
        canvas.blend_pixel(gx, base_row, neutral_line);
        canvas.blend_pixel(gx + 1, base_row, neutral_line);
        gx += 6;
    }
    for step in 0..=4 {
        let tx = left_x + ((right_x - left_x) * step) / 4;
        for ty in (base_row - 3)..=base_row {
            canvas.blend_pixel(tx, ty, Rgba { r: 132, g: 136, b: 144, alpha: 0.32 });
        }
    }

    // Repère vertical subtil du pic d'activité (si peak > 0)
    let max_val = counts.iter().copied().max().unwrap_or(0);
    if max_val > 0 {
        if let Some((peak_idx, _)) = counts.iter().enumerate().max_by_key(|&(_, &v)| v) {
            if let Some(&(px, py)) = anchors.get(peak_idx) {
                let pxi = px.round() as i32;
                let mut dy = (py.round() as i32) + 5;
                while dy < base_row - 2 {
                    canvas.blend_pixel(pxi, dy, Rgba { r: ar, g: ag, b: ab, alpha: 0.24 });
                    dy += 4;
                }
                canvas.draw_disc_aa(px, py, 2.0, Rgba { r: ar, g: ag, b: ab, alpha: 0.85 });
            }
        }
    }

    // Micro-labels typographiques adaptatifs Light/Dark Mode (`30D` à gauche, `PEAK N/D` à droite)
    canvas.draw_micro_label(left_x, 5, "30D", neutral_text);
    let peak_label = if max_val > 0 {
        format!("PEAK {}/D", max_val)
    } else {
        "IDLE".to_string()
    };
    let label_w = (peak_label.len() as i32) * 8;
    canvas.draw_micro_label((right_x - label_w).max(left_x + 40), 5, &peak_label, neutral_text);

    let neutral_line = Rgba { r: 132, g: 136, b: 144, alpha: 0.22 };
    let neutral_text = Rgba { r: 120, g: 125, b: 134, alpha: 0.80 };
    let base_row = (h - 6) as i32;
    let left_x = anchors.first().map(|p| p.0.round() as i32).unwrap_or(10);
    let right_x = anchors.last().map(|p| p.0.round() as i32).unwrap_or((w - 10) as i32);

    let mut gx = left_x;
    while gx <= right_x {
        canvas.blend_pixel(gx, base_row, neutral_line);
        canvas.blend_pixel(gx + 1, base_row, neutral_line);
        gx += 6;
    }
    for step in 0..=4 {
        let tx = left_x + ((right_x - left_x) * step) / 4;
        for ty in (base_row - 3)..=base_row {
            canvas.blend_pixel(tx, ty, Rgba { r: 132, g: 136, b: 144, alpha: 0.32 });
        }
    }

    let max_val = counts.iter().copied().max().unwrap_or(0);
    if max_val > 0 {
        if let Some((peak_idx, _)) = counts.iter().enumerate().max_by_key(|&(_, &v)| v) {
            if let Some(&(px, py)) = anchors.get(peak_idx) {
                let pxi = px.round() as i32;
                let mut dy = (py.round() as i32) + 5;
                while dy < base_row - 2 {
                    canvas.blend_pixel(pxi, dy, Rgba { r: ar, g: ag, b: ab, alpha: 0.24 });
                    dy += 4;
                }
                canvas.draw_disc_aa(px, py, 2.0, Rgba { r: ar, g: ag, b: ab, alpha: 0.85 });
            }
        }
    }

    canvas.draw_micro_label(left_x, 5, "30D", neutral_text);
    let peak_label = if max_val > 0 {
        format!("PEAK {}/D", max_val)
    } else {
        "IDLE".to_string()
    };
    let label_w = (peak_label.len() as i32) * 8;
    canvas.draw_micro_label((right_x - label_w).max(left_x + 40), 5, &peak_label, neutral_text);

    if let Some(&(lx, ly)) = anchors.last() {
        canvas.draw_disc_aa(lx, ly, 6.0, Rgba { r: ar, g: ag, b: ab, alpha: 0.14 });
        canvas.draw_disc_aa(lx, ly, 2.6, Rgba { r: ar, g: ag, b: ab, alpha: 0.95 });
    }

write_rgba_png(&out_path, w as u32, h as u32, &buf).ok()?;
Some(out_path.to_string_lossy().into_owned())
}
