// This file is part of OpenFA.
//
// OpenFA is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// OpenFA is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.
use failure::{bail, Fallible};
use image::{DynamicImage, GenericImageView, ImageRgba8};
use packed_struct::packed_struct;
use pal::Palette;
use std::{borrow::Cow, mem};

packed_struct!(Header {
    _0 => format: u16,
    _1 => width: u32,
    _2 => height: u32,
    _3 => pixels_offset: u32 as usize,
    _4 => pixels_size: u32 as usize,
    _5 => palette_offset: u32 as usize,
    _6 => palette_size: u32 as usize,
    _7 => spans_offset: u32 as usize,
    _8 => spans_size: u32 as usize,
    _9 => rowheads_offset: u32 as usize,
    _a => rowheads_size: u32 as usize
});

packed_struct!(Span {
    _row   => row: u16 as u32,
    _start => start: u16 as u32,
    _end   => end: u16 as u32,
    _index => index: u32 as usize
});

#[derive(Debug, Eq, PartialEq)]
pub enum PicFormat {
    Format0,
    Format1,
    JPEG,
}

impl PicFormat {
    pub fn from_word(format: u16) -> Fallible<Self> {
        Ok(match format {
            0 => PicFormat::Format0,
            1 => PicFormat::Format1,
            0xD8FF => PicFormat::JPEG,
            _ => bail!("unknown pic format: 0x{:04X}", format),
        })
    }
}

pub struct Pic {
    pub format: PicFormat,
    pub width: u32,
    pub height: u32,
    pub palette: Option<Palette>,
}

impl Pic {
    /// Returns metadata about the image. Call decode to get a DynamicImage for rendering.
    pub fn from_bytes(data: &[u8]) -> Fallible<Pic> {
        let header = Header::overlay(data)?;
        let format = PicFormat::from_word(header.format())?;
        if format == PicFormat::JPEG {
            let img = image::load_from_memory(data)?;
            return Ok(Pic {
                format: PicFormat::JPEG,
                width: img.width(),
                height: img.height(),
                palette: None,
            });
        }

        let palette = if header.palette_size() > 0 {
            let palette_data =
                &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
            Some(Palette::from_bytes(&palette_data)?)
        } else {
            None
        };

        Ok(Pic {
            format: PicFormat::from_word(header.format())?,
            width: header.width(),
            height: header.height(),
            palette,
        })
    }

    /// Render the PIC in `data` into a raster image. The given palette will be used if the image does not contain its own.
    pub fn decode(system_palette: &Palette, data: &[u8]) -> Fallible<DynamicImage> {
        let header = Header::overlay(data)?;
        let format = PicFormat::from_word(header.format())?;
        Ok(match format {
            PicFormat::JPEG => image::load_from_memory(data)?,
            PicFormat::Format0 => {
                let palette = Self::make_palette(header, data, system_palette)?;
                Self::decode_format0(
                    header.width(),
                    header.height(),
                    &palette,
                    &data[header.pixels_offset()..header.pixels_offset() + header.pixels_size()],
                )?
            }
            PicFormat::Format1 => {
                let palette = Self::make_palette(header, data, system_palette)?;
                Self::decode_format1(
                    header.width(),
                    header.height(),
                    &palette,
                    &data[header.spans_offset()..header.spans_offset() + header.spans_size()],
                    &data[header.pixels_offset()..header.pixels_offset() + header.pixels_size()],
                )?
            }
        })
    }

    fn make_palette<'a>(
        header: &'a Header,
        data: &'a [u8],
        system_palette: &'a Palette,
    ) -> Fallible<Cow<'a, Palette>> {
        if header.palette_size() == 0 {
            return Ok(Cow::from(system_palette));
        }

        let palette_data =
            &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
        let local_palette = Palette::from_bytes(&palette_data)?;
        let mut palette = system_palette.clone();
        palette.overlay_at(&local_palette, 0)?;
        Ok(Cow::from(palette))
    }

    fn decode_format0(
        width: u32,
        height: u32,
        palette: &Palette,
        pixels: &[u8],
    ) -> Fallible<DynamicImage> {
        let mut imgbuf = image::ImageBuffer::new(width, height);
        for (i, p) in imgbuf.pixels_mut().enumerate() {
            let pix = pixels[i] as usize;
            let mut clr = palette.rgba(pix)?;
            if pix == 0xFF {
                clr.data[3] = 0x00;
            }
            *p = clr;
        }
        Ok(ImageRgba8(imgbuf))
    }

    fn decode_format1(
        width: u32,
        height: u32,
        palette: &Palette,
        spans: &[u8],
        pixels: &[u8],
    ) -> Fallible<DynamicImage> {
        let mut imgbuf = image::ImageBuffer::new(width, height);
        assert_eq!(spans.len() % mem::size_of::<Span>(), 0);
        let span_cnt = spans.len() / mem::size_of::<Span>() - 1;
        for i in 0..span_cnt {
            let span = Span::overlay(&spans[i * mem::size_of::<Span>()..])?;
            assert!(span.row() < height);
            assert!(span.index() < pixels.len());
            assert!(span.start() < width);
            assert!(span.end() < width);
            assert!(span.start() <= span.end());
            assert!(span.index() + ((span.end() - span.start()) as usize) < pixels.len());

            for (j, column) in (span.start()..=span.end()).enumerate() {
                let offset = span.index() + j;
                let pix = pixels[offset] as usize;
                let clr = palette.rgba(pix)?;
                imgbuf.put_pixel(column, span.row(), clr);
            }
        }
        Ok(ImageRgba8(imgbuf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omnilib::OmniLib;
    use std::{fs, path::Path};

    #[test]
    fn it_can_new_all_pics() -> Fallible<()> {
        let omni = OmniLib::new_for_test()?;
        for (game, name) in omni.find_matching("*.PIC")? {
            println!("AT: {}:{}", game, name);
            let _img = Pic::from_bytes(&omni.library(&game).load(&name)?)?;
        }

        Ok(())
    }

    #[test]
    fn it_can_decode_all_pics() -> Fallible<()> {
        let omni = OmniLib::new_for_test()?;
        for (game, name) in omni.find_matching("*.PIC")? {
            println!("AT: {}:{}", game, name);
            let palette = Palette::from_bytes(&omni.library(&game).load("PALETTE.PAL")?)?;
            let img = Pic::decode(&palette, &omni.library(&game).load(&name)?)?;

            if false {
                let name = format!(
                    "../../dump/pic/{}/{}.png",
                    game,
                    name.split(".").collect::<Vec<_>>().first().unwrap()
                );
                let path = Path::new(&name);
                println!("Write: {}", path.display());
                let _ = fs::create_dir("dump");
                let _ = fs::create_dir(path.parent().unwrap());
                img.save(path)?;
            }
        }

        Ok(())
    }
}
