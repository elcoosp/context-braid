use anyhow::Result;
use std::fs;
use std::path::PathBuf;

fn blend_pixel(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, r: u8, g: u8, b: u8, alpha: f32) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 || alpha <= 0.001 {
        return;
    }
    let idx = ((y as usize) * w + (x as usize)) * 4;
    let src_a = alpha.clamp(0.0, 1.0);
    let dst_a = buf[idx + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0001 {
        return;
    }
    let blend = |src_c: u8, dst_c: u8| -> u8 {
        let sc = src_c as f32 / 255.0;
        let dc = dst_c as f32 / 255.0;
        let oc = (sc * src_a + dc * dst_a * (1.0 - src_a)) / out_a;
        (oc * 255.0).round().clamp(0.0, 255.0) as u8
    };
    buf[idx] = blend(r, buf[idx]);
    buf[idx + 1] = blend(g, buf[idx + 1]);
    buf[idx + 2] = blend(b, buf[idx + 2]);
    buf[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn draw_disc_aa(buf: &mut [u8], w: usize, h: usize, cx: f32, cy: f32, radius: f32, r: u8, g: u8, b: u8, alpha: f32) {
    let min_x = (cx - radius - 1.5).floor() as i32;
    let max_x = (cx + radius + 1.5).ceil() as i32;
    let min_y = (cy - radius - 1.5).floor() as i32;
    let max_y = (cy + radius + 1.5).ceil() as i32;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let cov = (radius + 0.6 - dist).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend_pixel(buf, w, h, px, py, r, g, b, alpha * cov);
            }
        }
    }
}

fn draw_segment_aa(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    r: u8,
    g: u8,
    b: u8,
    alpha: f32,
) {
    let min_x = x0.min(x1).floor() as i32 - 3;
    let max_x = x0.max(x1).ceil() as i32 + 3;
    let min_y = y0.min(y1).floor() as i32 - 3;
    let max_y = y0.max(y1).ceil() as i32 + 3;
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len_sq = (dx * dx + dy * dy).max(1e-6);
    let half_w = thickness * 0.5;

    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let cx = px as f32 + 0.5;
            let cy = py as f32 + 0.5;
            let t = (((cx - x0) * dx + (cy - y0) * dy) / len_sq).clamp(0.0, 1.0);
            let proj_x = x0 + t * dx;
            let proj_y = y0 + t * dy;
            let dist = ((cx - proj_x).powi(2) + (cy - proj_y).powi(2)).sqrt();
            let cov = (half_w + 0.7 - dist).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend_pixel(buf, w, h, px, py, r, g, b, alpha * cov);
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
    let h: usize = 84;
    let mut buf = vec![0u8; w * h * 4];

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
                blend_pixel(&mut buf, w, h, gx, gy, 142, 142, 147, 0.09);
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
                let fill_alpha = 0.14 * rel.powf(1.7);
                blend_pixel(&mut buf, w, h, px, py, ar, ag, ab, fill_alpha);
            }
        }
    }

    for idx in 0..curve.len().saturating_sub(1) {
        let (x0, y0) = curve[idx];
        let (x1, y1) = curve[idx + 1];
        draw_segment_aa(&mut buf, w, h, x0, y0, x1, y1, 1.85, ar, ag, ab, 0.88);
    }

    if let Some(&(lx, ly)) = anchors.last() {
        draw_disc_aa(&mut buf, w, h, lx, ly, 6.0, ar, ag, ab, 0.14);
        draw_disc_aa(&mut buf, w, h, lx, ly, 2.6, ar, ag, ab, 0.95);
    }

write_rgba_png(&out_path, w as u32, h as u32, &buf).ok()?;
Some(out_path.to_string_lossy().into_owned())
}
