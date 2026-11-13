//! Project Zomboid texture packs (`media/texturepacks/*.pack`): pages of sprites, each page a
//! PNG atlas with the rectangles of the sprites cut from it. Read straight from the user's own
//! install; lulo never ships the game's art.
//!
//! Layout, all integers little-endian i32, strings as length + Latin-1 bytes:
//! `pages`, then per page: `name`, `count`, `mask flag`, `count` entries of
//! `name, x, y, w, h, offset_x, offset_y, frame_w, frame_h`, then the page PNG. Two versions:
//! the original ends each PNG with the marker `0xDEADBEEF`; newer packs (`UI2.pack`) start with
//! `PZPK` and a version (1), and put the PNG's length before it instead.

use std::io;
use std::ops::Range;
use std::path::Path;

use image::RgbaImage;

const MARKER: [u8; 4] = 0xDEAD_BEEFu32.to_le_bytes();
/// Far above any real pack; a count past this means the file isn't one.
const MAX_COUNT: i32 = 100_000;

pub struct Sprite {
    pub name: String,
    page: usize,
    /// Trimmed rectangle inside the page.
    rect: [u32; 4],
    /// Where the trimmed rectangle sits inside the untrimmed frame, and the frame's size.
    offset: [u32; 2],
    frame: [u32; 2],
}

pub struct Pack {
    data: Vec<u8>,
    pages: Vec<Range<usize>>,
    pub sprites: Vec<Sprite>,
}

pub fn read(path: &Path) -> io::Result<Pack> {
    parse(std::fs::read(path)?)
}

fn bad(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("not a texture pack: {what}"),
    )
}

fn parse(data: Vec<u8>) -> io::Result<Pack> {
    let sized = data.starts_with(b"PZPK");
    let mut pos = if sized { 8 } else { 0 };
    let int = |pos: &mut usize| -> io::Result<i32> {
        let bytes = data.get(*pos..*pos + 4).ok_or_else(|| bad("truncated"))?;
        *pos += 4;
        Ok(i32::from_le_bytes(bytes.try_into().expect("4 bytes")))
    };
    let count = |pos: &mut usize| -> io::Result<usize> {
        let n = int(pos)?;
        if !(0..=MAX_COUNT).contains(&n) {
            return Err(bad("count out of range"));
        }
        Ok(n as usize)
    };
    let text = |pos: &mut usize| -> io::Result<String> {
        let n = count(pos)?;
        let bytes = data
            .get(*pos..*pos + n)
            .ok_or_else(|| bad("truncated name"))?;
        *pos += n;
        Ok(bytes.iter().map(|&b| b as char).collect())
    };
    let size = |pos: &mut usize| -> io::Result<u32> {
        u32::try_from(int(pos)?).map_err(|_| bad("negative size"))
    };

    let mut pages = Vec::new();
    let mut sprites = Vec::new();
    for page in 0..count(&mut pos)? {
        let _name = text(&mut pos)?;
        let entries = count(&mut pos)?;
        let _mask = int(&mut pos)?;
        for _ in 0..entries {
            let name = text(&mut pos)?;
            let mut v = [0; 8];
            for value in &mut v {
                *value = size(&mut pos)?;
            }
            sprites.push(Sprite {
                name,
                page,
                rect: [v[0], v[1], v[2], v[3]],
                offset: [v[4], v[5]],
                frame: [v[6], v[7]],
            });
        }
        if sized {
            let len = usize::try_from(int(&mut pos)?).map_err(|_| bad("negative length"))?;
            let end = pos
                .checked_add(len)
                .filter(|&end| end <= data.len())
                .ok_or_else(|| bad("page past the end"))?;
            pages.push(pos..end);
            pos = end;
        } else {
            let end = data[pos..]
                .windows(4)
                .position(|w| w == MARKER)
                .map(|at| pos + at)
                .ok_or_else(|| bad("page without end marker"))?;
            pages.push(pos..end);
            pos = end + 4;
        }
    }
    Ok(Pack {
        data,
        pages,
        sprites,
    })
}

impl Pack {
    /// Decodes each page once and hands every sprite, cut into its untrimmed frame, to `each`.
    pub fn for_each(&self, mut each: impl FnMut(&Sprite, RgbaImage)) -> io::Result<()> {
        for (ix, range) in self.pages.iter().enumerate() {
            let page = image::load_from_memory_with_format(
                &self.data[range.clone()],
                image::ImageFormat::Png,
            )
            .map_err(|e| bad(&e.to_string()))?
            .to_rgba8();
            for sprite in self.sprites.iter().filter(|s| s.page == ix) {
                if let Some(frame) = cut(&page, sprite) {
                    each(sprite, frame);
                }
            }
        }
        Ok(())
    }
}

/// The sprite placed in its full frame; `None` when its rectangle falls outside the page.
fn cut(page: &RgbaImage, sprite: &Sprite) -> Option<RgbaImage> {
    let [x, y, w, h] = sprite.rect;
    let [ox, oy] = sprite.offset;
    let [fw, fh] = sprite.frame;
    if x.checked_add(w)? > page.width() || y.checked_add(h)? > page.height() {
        return None;
    }
    let mut frame = RgbaImage::new(fw.max(ox + w), fh.max(oy + h));
    let trimmed = image::imageops::crop_imm(page, x, y, w, h).to_image();
    image::imageops::overlay(&mut frame, &trimmed, ox.into(), oy.into());
    Some(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(out: &mut Vec<u8>, v: i32) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    fn text(out: &mut Vec<u8>, s: &str) {
        int(out, s.len() as i32);
        out.extend_from_slice(s.as_bytes());
    }

    /// One page: a 4x2 atlas whose right half is a red 2x2 sprite placed at (1, 1) in a 4x4
    /// frame, the way PZ trims transparent borders. `sized` writes the newer `PZPK` layout.
    fn pack(sized: bool) -> Vec<u8> {
        let mut atlas = RgbaImage::new(4, 2);
        for (x, y) in [(2, 0), (3, 0), (2, 1), (3, 1)] {
            atlas.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
        }
        let mut png = Vec::new();
        atlas
            .write_to(&mut io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();

        let mut out = Vec::new();
        if sized {
            out.extend_from_slice(b"PZPK");
            int(&mut out, 1);
        }
        int(&mut out, 1);
        text(&mut out, "page0");
        int(&mut out, 1);
        int(&mut out, 1);
        text(&mut out, "Item_Red");
        for v in [2, 0, 2, 2, 1, 1, 4, 4] {
            int(&mut out, v);
        }
        if sized {
            int(&mut out, png.len() as i32);
            out.extend_from_slice(&png);
        } else {
            out.extend_from_slice(&png);
            out.extend_from_slice(&MARKER);
        }
        out
    }

    fn sample() -> Vec<u8> {
        pack(false)
    }

    #[test]
    fn cuts_sprites_into_their_frames() {
        for sized in [false, true] {
            let pack = parse(pack(sized)).unwrap();
            let mut seen = Vec::new();
            pack.for_each(|sprite, image| seen.push((sprite.name.clone(), image)))
                .unwrap();
            let (name, image) = &seen[0];
            assert_eq!(name, "Item_Red");
            assert_eq!(image.dimensions(), (4, 4));
            assert_eq!(image.get_pixel(1, 1).0, [255, 0, 0, 255]);
            assert_eq!(
                image.get_pixel(0, 0).0[3],
                0,
                "the trimmed border stays clear"
            );
        }
    }

    #[test]
    fn rejects_broken_packs() {
        let full = sample();
        assert!(
            parse(full[..full.len() - 4].to_vec()).is_err(),
            "no end marker"
        );
        assert!(parse(full[..10].to_vec()).is_err(), "truncated");
        let mut huge = full.clone();
        huge[..4].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(parse(huge).is_err(), "absurd page count");
        let sized = pack(true);
        assert!(
            parse(sized[..sized.len() - 1].to_vec()).is_err(),
            "PNG cut short"
        );
    }

    /// `LULO_PZ_PACK=/path/to/media/texturepacks/UI.pack cargo test -- --ignored`
    #[test]
    #[ignore]
    fn reads_a_real_pack() {
        let path = std::env::var("LULO_PZ_PACK").expect("LULO_PZ_PACK");
        let pack = read(Path::new(&path)).unwrap();
        let mut items = 0;
        pack.for_each(|sprite, _| items += sprite.name.starts_with("Item_") as usize)
            .unwrap();
        assert!(items > 100, "{items} item sprites");
    }
}
