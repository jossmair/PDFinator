//! Development-only rasterization of the original SVG and Windows ICO directory.
use std::{fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let tree = resvg::usvg::Tree::from_str(
        &fs::read_to_string(assets.join("pdfinator.svg"))?,
        &Default::default(),
    )?;
    let render = |size: u32| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).ok_or("Invalid icon size")?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(size as f32 / 256., size as f32 / 256.),
            &mut pixmap.as_mut(),
        );
        Ok(pixmap.encode_png()?)
    };
    fs::write(assets.join("pdfinator.png"), render(1024)?)?;
    let sizes = [16_u32, 20, 24, 32, 40, 48, 64, 128, 256];
    let frames: Vec<_> = sizes
        .iter()
        .map(|&size| {
            if size == 256 {
                return render(size);
            }
            // Explorer, file pickers and older shell extensions expect a Windows DIB
            // at small sizes. Keep PNG only for the 256px jumbo icon.
            let mut pixmap =
                resvg::tiny_skia::Pixmap::new(size, size).ok_or("Invalid icon size")?;
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(size as f32 / 256., size as f32 / 256.),
                &mut pixmap.as_mut(),
            );
            let mask_stride = size.div_ceil(32) * 4;
            let mut dib = Vec::new();
            dib.extend(40_u32.to_le_bytes());
            dib.extend(size.to_le_bytes());
            dib.extend((size * 2).to_le_bytes());
            dib.extend(1_u16.to_le_bytes());
            dib.extend(32_u16.to_le_bytes());
            for value in [0, size * size * 4, 0, 0, 0, 0] {
                dib.extend(value.to_le_bytes());
            }
            for y in (0..size).rev() {
                for x in 0..size {
                    let i = ((y * size + x) * 4) as usize;
                    let p = &pixmap.data()[i..i + 4];
                    let channel = |v: u8| {
                        if p[3] == 0 {
                            0
                        } else {
                            ((v as u32 * 255 + p[3] as u32 / 2) / p[3] as u32).min(255) as u8
                        }
                    };
                    dib.extend([channel(p[2]), channel(p[1]), channel(p[0]), p[3]]);
                }
            }
            for y in (0..size).rev() {
                let mut mask = vec![0; mask_stride as usize];
                for x in 0..size {
                    if pixmap.data()[((y * size + x) * 4 + 3) as usize] == 0 {
                        mask[(x / 8) as usize] |= 0x80 >> (x % 8);
                    }
                }
                dib.extend(mask);
            }
            Ok(dib)
        })
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    let mut ico = vec![0, 0, 1, 0];
    ico.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + sizes.len() as u32 * 16;
    for (&size, png) in sizes.iter().zip(&frames) {
        ico.extend([size as u8, size as u8, 0, 0]);
        ico.extend(1_u16.to_le_bytes());
        ico.extend(32_u16.to_le_bytes());
        ico.extend((png.len() as u32).to_le_bytes());
        ico.extend(offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for frame in frames {
        ico.extend(frame);
    }
    fs::write(assets.join("pdfinator.ico"), ico)?;
    Ok(())
}
