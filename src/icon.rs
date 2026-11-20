//! The tray icon: the LFG+ add-on's own icon, an uncompressed 32-bit TGA, read with no image library.

/// The icon's pixels as straight RGBA, top row first, with its width and height.
pub fn decode_tga(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let header = bytes.get(..18)?;
    let id_length = usize::from(header[0]);
    // Uncompressed true colour, 32 bits per pixel, no colour map.
    if header[1] != 0 || header[2] != 2 || header[16] != 32 {
        return None;
    }
    let width = u32::from(u16::from_le_bytes([header[12], header[13]]));
    let height = u32::from(u16::from_le_bytes([header[14], header[15]]));
    let top_first = header[17] & 0x20 != 0;
    let size = (width as usize) * (height as usize) * 4;
    let pixels = bytes.get(18 + id_length..18 + id_length + size)?;
    let row = width as usize * 4;
    let mut rgba = Vec::with_capacity(size);
    for y in 0..height as usize {
        let source = if top_first { y } else { height as usize - 1 - y };
        for bgra in pixels[source * row..(source + 1) * row].as_chunks::<4>().0 {
            rgba.extend_from_slice(&[bgra[2], bgra[1], bgra[0], bgra[3]]);
        }
    }
    Some((rgba, width, height))
}

/// How much larger the tray draws the icon than the add-on does. The add-on's icon leaves a wide margin so it sits
/// inside the minimap's round frame; a tray icon has no frame and reads better filling its square.
pub const TRAY_ZOOM: f32 = 1.25;

/// The drawing's bounds: the first and last column and row with a visible pixel. None for an empty image.
pub fn content_box(rgba: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..h {
        for x in 0..w {
            if rgba[((y * w + x) * 4 + 3) as usize] > 16 {
                let (x0, y0, x1, y1) = bounds.unwrap_or((x, y, x, y));
                bounds = Some((x0.min(x), y0.min(y), x1.max(x), y1.max(y)));
            }
        }
    }
    bounds
}

/// The image redrawn `zoom` times larger about its drawing's centre, at the same size, with bilinear filtering on
/// premultiplied colour (so edges don't darken). The zoom is held down so the drawing keeps a pixel of margin.
pub fn zoomed(rgba: &[u8], w: u32, h: u32, zoom: f32) -> Vec<u8> {
    let Some((x0, y0, x1, y1)) = content_box(rgba, w, h) else { return rgba.to_vec() };
    // Two pixels a side: one of margin, one for the soft edge the filtering adds.
    let fit =
        (w.saturating_sub(4) as f32 / (x1 - x0 + 1) as f32).min(h.saturating_sub(4) as f32 / (y1 - y0 + 1) as f32);
    let zoom = zoom.min(fit).max(1.0);
    let (cx, cy) = ((x0 + x1 + 1) as f32 / 2.0, (y0 + y1 + 1) as f32 / 2.0);
    // A source pixel, premultiplied; transparent outside the image.
    let pixel = |x: i64, y: i64| -> [f32; 4] {
        if x < 0 || y < 0 || x >= i64::from(w) || y >= i64::from(h) {
            return [0.0; 4];
        }
        let i = ((y as u32 * w + x as u32) * 4) as usize;
        let a = f32::from(rgba[i + 3]) / 255.0;
        [f32::from(rgba[i]) * a, f32::from(rgba[i + 1]) * a, f32::from(rgba[i + 2]) * a, a]
    };
    let mut out = Vec::with_capacity(rgba.len());
    for y in 0..h {
        for x in 0..w {
            let sx = cx + (x as f32 + 0.5 - w as f32 / 2.0) / zoom - 0.5;
            let sy = cy + (y as f32 + 0.5 - h as f32 / 2.0) / zoom - 0.5;
            let (fx, fy) = (sx.floor(), sy.floor());
            let (tx, ty) = (sx - fx, sy - fy);
            let (ix, iy) = (fx as i64, fy as i64);
            let mut c = [0.0f32; 4];
            for (dx, dy, weight) in
                [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)]
            {
                let p = pixel(ix + dx, iy + dy);
                for k in 0..4 {
                    c[k] += p[k] * weight;
                }
            }
            let a = c[3];
            let colour = |v: f32| if a > 0.0 { (v / a).round().clamp(0.0, 255.0) as u8 } else { 0 };
            out.extend_from_slice(&[
                colour(c[0]),
                colour(c[1]),
                colour(c[2]),
                (a * 255.0).round().clamp(0.0, 255.0) as u8,
            ]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tray_draws_the_shipped_icon_a_quarter_larger() {
        let (rgba, w, h) = decode_tga(include_bytes!("../assets/icon.tga")).expect("decodes");
        let (x0, y0, x1, y1) = content_box(&rgba, w, h).expect("drawn");
        let big = zoomed(&rgba, w, h, TRAY_ZOOM);
        assert_eq!(big.len(), rgba.len());
        let (bx0, by0, bx1, by1) = content_box(&big, w, h).expect("drawn");
        let grown = (bx1 - bx0 + 1) as f32 / (x1 - x0 + 1) as f32;
        assert!((1.2..=1.3).contains(&grown), "width grew {grown}");
        let grown = (by1 - by0 + 1) as f32 / (y1 - y0 + 1) as f32;
        assert!((1.2..=1.3).contains(&grown), "height grew {grown}");
        // Centred, and still inside the square.
        assert!(bx0 >= 1 && by0 >= 1 && bx1 <= w - 2 && by1 <= h - 2);
        assert!(((bx0 + bx1) as i64 - (w - 1) as i64).abs() <= 2 && ((by0 + by1) as i64 - (h - 1) as i64).abs() <= 2);
    }

    #[test]
    fn zooming_centres_the_drawing_and_keeps_its_colour() {
        // An 8×8 image with a 2×2 red square in its corner. Asked for 10 times larger, it's held to what fits (twice) and
        // moved to the middle, with a pixel of margin.
        let mut rgba = vec![0u8; 8 * 8 * 4];
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            rgba[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
        }
        let out = zoomed(&rgba, 8, 8, 10.0);
        let (x0, y0, x1, y1) = content_box(&out, 8, 8).expect("drawn");
        assert_eq!((x0 + x1, y0 + y1), (7, 7), "centred: {x0},{y0},{x1},{y1}");
        assert!((4..=6).contains(&(x1 - x0 + 1)), "grew to {}", x1 - x0 + 1);
        assert!(x0 >= 1 && y0 >= 1 && x1 <= 6 && y1 <= 6, "margin: {x0},{y0},{x1},{y1}");
        // Every visible pixel is still pure red: no dark fringe from mixing with transparent black.
        for p in out.chunks(4).filter(|p| p[3] > 0) {
            assert_eq!(&p[..3], &[255, 0, 0]);
        }
    }

    #[test]
    fn an_empty_image_stays_empty() {
        let rgba = vec![0u8; 2 * 2 * 4];
        assert_eq!(content_box(&rgba, 2, 2), None);
        assert_eq!(zoomed(&rgba, 2, 2, 2.0), rgba);
    }

    fn tga(descriptor: u8, pixels: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 2, 0, 32, descriptor];
        bytes.extend_from_slice(pixels);
        bytes
    }

    #[test]
    fn reads_the_shipped_icon() {
        let (rgba, w, h) = decode_tga(include_bytes!("../assets/icon.tga")).expect("decodes");
        assert_eq!((w, h, rgba.len()), (64, 64, 64 * 64 * 4));
    }

    #[test]
    fn reads_rows_in_either_order() {
        let pixels = [1, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(decode_tga(&tga(0x28, &pixels)), Some((vec![3, 2, 1, 4, 7, 6, 5, 8], 1, 2)));
        assert_eq!(decode_tga(&tga(0x08, &pixels)), Some((vec![7, 6, 5, 8, 3, 2, 1, 4], 1, 2)));
    }

    #[test]
    fn refuses_other_images() {
        assert_eq!(decode_tga(&[0; 10]), None);
        let mut rle = tga(0x28, &[0; 8]);
        rle[2] = 10;
        assert_eq!(decode_tga(&rle), None);
        let mut mapped = tga(0x28, &[0; 8]);
        mapped[1] = 1;
        assert_eq!(decode_tga(&mapped), None);
        let mut rgb = tga(0x28, &[0; 8]);
        rgb[16] = 24;
        assert_eq!(decode_tga(&rgb), None);
        assert_eq!(decode_tga(&tga(0x28, &[0; 7])), None);
    }
}
