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

#[cfg(test)]
mod tests {
    use super::*;

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
