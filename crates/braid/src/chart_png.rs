use std::fs;
use std::path::PathBuf;

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
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

fn adler32(data: &[u8]) -> u32 {
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for &b in data {
        s1 = (s1 + b as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    (s2 << 16) | s1
}

fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 4 + 1024) as usize);
    out.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // RGBA
    ihdr.extend_from_slice(&[0, 0, 0]);
    write_chunk(&mut out, b"IHDR", &ihdr);

    // Raw uncompressed deflate blocks (fast & zero-dependency)
    let row_bytes = (width * 4 + 1) as usize;
    let mut raw = Vec::with_capacity(row_bytes * height as usize);
    for y in 0..height as usize {
        raw.push(0); // filter None
        let start = y * (width as usize * 4);
        raw.extend_from_slice(&rgba[start..start + (width as usize * 4)]);
    }

    let mut zlib = Vec::with_capacity(raw.len() + 64);
    zlib.extend_from_slice(&[0x78, 0x01]);
    let mut pos = 0;
    while pos < raw.len() {
        let len = (raw.len() - pos).min(65535);
        let is_last = if pos + len == raw.len() { 1u8 } else { 0u8 };
        zlib.push(is_last);
        zlib.extend_from_slice(&(len as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(len as u16)).to_le_bytes());
        zlib.extend_from_slice(&raw[pos..pos + len]);
        pos += len;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    write_chunk(&mut out, b"IDAT", &zlib);
    write_chunk(&mut out, b"IEND", &[]);
    out
}

fn write_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc_buf = Vec::with_capacity(4 + data.len());
    crc_buf.extend_from_slice(tag);
    crc_buf.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_buf).to_be_bytes());
}

fn blend_px(buf: &mut [u8], w: u32, h: u32, x: i32, y: i32, r: u8, g: u8, b: u8, a: f32) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 || a <= 0.0 {
        return;
    }
    let idx = ((y as u32 * w + x as u32) * 4) as usize;
    let src_a = a.clamp(0.0, 1.0);
    let dst_a = buf[idx + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        return;
    }
    let blend = |s: u8, d: u8| -> u8 {
        (((s as f32 * src_a) + (d as f32 * dst_a * (1.0 - src_a))) / out_a)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    buf[idx] = blend(r, buf[idx]);
    buf[idx + 1] = blend(g, buf[idx + 1]);
    buf[idx + 2] = blend(b, buf[idx + 2]);
    buf[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn draw_circle_aa(buf: &mut [u8], w: u32, h: u32, cx: f32, cy: f32, radius: f32, col: (u8, u8, u8), alpha: f32) {
    let min_x = (cx - radius - 2.0).floor() as i32;
    let max_x = (cx + radius + 2.0).ceil() as i32;
    let min_y = (cy - radius - 2.0).floor() as i32;
    let max_y = (cy + radius + 2.0).ceil() as i32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let cov = (radius + 0.75 - dist).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend_px(buf, w, h, x, y, col.0, col.1, col.2, alpha * cov);
            }
        }
    }
}

fn draw_segment_aa(buf: &mut [u8], w: u32, h: u32, x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, col: (u8, u8, u8), alpha: f32) {
    let min_x = x0.min(x1).floor() as i32 - 4;
    let max_x = x0.max(x1).ceil() as i32 + 4;
    let min_y = y0.min(y1).floor() as i32 - 4;
    let max_y = y0.max(y1).ceil() as i32 + 4;
    let vx = x1 - x0;
    let vy = y1 - y0;
    let len_sq = vx * vx + vy * vy;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let t = if len_sq > 0.0 {
                (((px - x0) * vx + (py - y0) * vy) / len_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let proj_x = x0 + t * vx;
            let proj_y = y0 + t * vy;
            let dist = ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt();
            let cov = (thickness * 0.5 + 0.8 - dist).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend_px(buf, w, h, x, y, col.0, col.1, col.2, alpha * cov);
            }
        }
    }
}

pub fn render_velocity_chart_png(repo_name: &str, counts: &[u32], accent: (u8, u8, u8)) -> Option<String> {
    let dir = PathBuf::from("/tmp/contextbraid-charts");
    fs::create_dir_all(&dir).ok()?;

    let w: u32 = 720;
    let h: u32 = 152;
    let mut buf = vec![0u8; (w * h * 4) as usize];

    // Subtle rounded card background & horizontal grid lines
    for y in 4..(h - 4) {
        for x in 4..(w - 4) {
            blend_px(&mut buf, w, h, x as i32, y as i32, 130, 140, 160, 0.07);
        }
    }
    for grid_y in [34, 76, 118] {
        for x in (20..(w - 20)).step_by(8) {
            for dx in 0..4 {
                blend_px(&mut buf, w, h, (x + dx) as i32, grid_y, 140, 150, 170, 0.16);
            }
        }
    }

    let data: Vec<u32> = if counts.len() == 14 {
        counts.to_vec()
    } else {
        vec![0; 14]
    };
    let max_val = (*data.iter().max().unwrap_or(&1)).max(1) as f32;

    let pad_x = 28.0_f32;
    let pad_top = 24.0_f32;
    let pad_bot = 126.0_f32;
    let plot_w = w as f32 - pad_x * 2.0;
    let plot_h = pad_bot - pad_top;

    let pts: Vec<(f32, f32, u32)> = data
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = pad_x + (i as f32 / 13.0) * plot_w;
            let y = pad_bot - (v as f32 / max_val) * plot_h;
            (x, y, v)
        })
        .collect();

    // Sample smooth cubic Hermite / cosine curve across all X columns for area gradient + crisp curve
    let steps = 280usize;
    let mut curve_pts = Vec::with_capacity(steps + 1);
    for s in 0..=steps {
        let u = (s as f32 / steps as f32) * 13.0;
        let idx = (u.floor() as usize).min(12);
        let frac = u - idx as f32;
        let mu = (1.0 - (frac * std::f32::consts::PI).cos()) * 0.5;
        let x = pts[idx].0 + frac * (pts[idx + 1].0 - pts[idx].0);
        let y = pts[idx].1 * (1.0 - mu) + pts[idx + 1].1 * mu;
        curve_pts.push((x, y));
    }

    // Vertical translucent gradient fill under the curve
    for &(cx, cy) in &curve_pts {
        let xi = cx.round() as i32;
        let y_start = cy.ceil() as i32;
        let y_end = pad_bot.round() as i32;
        for yi in y_start..=y_end {
            let t = 1.0 - ((yi as f32 - cy) / (pad_bot - cy + 1.0)).clamp(0.0, 1.0);
            blend_px(&mut buf, w, h, xi, yi, accent.0, accent.1, accent.2, 0.24 * t * t);
        }
    }

    // Crisp anti-aliased curve segments
    for win in curve_pts.windows(2) {
        draw_segment_aa(&mut buf, w, h, win[0].0, win[0].1, win[1].0, win[1].1, 4.4, accent, 0.96);
    }

    // Nodes on active commit days + glowing beacon on latest day
    for &(px, py, v) in &pts {
        if v > 0 {
            draw_circle_aa(&mut buf, w, h, px, py, 6.5, accent, 0.25);
            draw_circle_aa(&mut buf, w, h, px, py, 4.2, accent, 1.0);
        }
    }
    if let Some(&(lx, ly, _)) = pts.last() {
        draw_circle_aa(&mut buf, w, h, lx, ly, 9.5, accent, 0.24);
        draw_circle_aa(&mut buf, w, h, lx, ly, 5.0, accent, 1.0);
    }

    let png_bytes = encode_rgba_png(w, h, &buf);
    let safe_name: String = repo_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let out_path = dir.join(format!("{}.png", safe_name));
    fs::write(&out_path, png_bytes).ok()?;
    Some(out_path.to_string_lossy().into_owned())
}
