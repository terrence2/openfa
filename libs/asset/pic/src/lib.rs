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
use anyhow::{bail, ensure, Result};
use image::{DynamicImage, GenericImage, GenericImageView, RgbaImage};
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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PicFormat {
    Format0,
    Format1,
    Jpeg,
}

impl PicFormat {
    pub fn from_word(format: u16) -> Result<Self> {
        Ok(match format {
            0 => PicFormat::Format0,
            1 => PicFormat::Format1,
            // Note that this is a totally normal jpeg start-of-stream marker and we should pass
            // the full data block to the jpeg decoder rather than trying to slice of the "header".
            0xD8FF => PicFormat::Jpeg,
            _ => bail!("unknown pic format: 0x{:04X}", format),
        })
    }
}

pub struct Pic<'a> {
    format: PicFormat,
    width: u32,
    height: u32,
    palette: Option<Palette>,
    spans: &'a [Span],
    // TODO: remove offset and size as soon as we remove decode_to_buffer;
    //       this is currently needed by shape's atlas; port to new atlas to do decode on GPU,
    //       obviating the need to decode on CPU at all
    pixels_offset: usize,
    pixels_size: usize,
    pixels_data: &'a [u8],
}

impl<'a> Pic<'a> {
    pub fn read_format(data: &[u8]) -> Result<PicFormat> {
        let header = Header::overlay(&data[..mem::size_of::<Header>()])?;
        PicFormat::from_word(header.format())
    }

    /// Returns metadata about the image. Fails if called on a jpeg format image.
    /// Note: Useful for applications that want to e.g. use the palettized pixels bare.
    pub fn from_bytes_non_jpeg(data: &'a [u8]) -> Result<Pic> {
        let header = Header::overlay(&data[..mem::size_of::<Header>()])?;
        let format = PicFormat::from_word(header.format())?;
        ensure!(format != PicFormat::Jpeg);

        let palette = if header.palette_size() > 0 {
            let palette_data =
                &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
            Some(Palette::from_bytes(palette_data)?)
        } else {
            None
        };

        Ok(Pic {
            format: PicFormat::from_word(header.format())?,
            width: header.width(),
            height: header.height(),
            palette,
            // Note: last span is always a marker.
            spans: Span::overlay_slice(
                &data[header.spans_offset()
                    ..header.spans_offset() + header.spans_size() - mem::size_of::<Span>()],
            )?,
            pixels_offset: header.pixels_offset(),
            pixels_size: header.pixels_size(),
            pixels_data: &data
                [header.pixels_offset()..header.pixels_offset() + header.pixels_size()],
        })
    }

    /// Returns metadata about the image. Call decode to get a DynamicImage.
    /// Note: Does a full image decode if the image is a jpeg.
    pub fn from_bytes(data: &'a [u8]) -> Result<Pic> {
        Ok(if let Ok(meta) = Self::from_bytes_non_jpeg(data) {
            meta
        } else {
            let img = image::load_from_memory(data)?;
            Pic {
                format: PicFormat::Jpeg,
                width: img.width(),
                height: img.height(),
                palette: None,
                spans: &[],
                pixels_offset: 0,
                pixels_size: 0,
                pixels_data: &[],
            }
        })
    }

    /// Render the PIC in `data` into a raster image. The given palette will be used if the image does not contain its own.
    pub fn decode(palette: &Palette, data: &[u8]) -> Result<DynamicImage> {
        let header = Header::overlay(&data[..mem::size_of::<Header>()])?;
        let format = PicFormat::from_word(header.format())?;
        Ok(match format {
            PicFormat::Jpeg => image::load_from_memory(data)?,
            PicFormat::Format0 => {
                let palette = Self::make_palette(header, data, palette)?;
                DynamicImage::ImageRgba8(Self::decode_format0(
                    header.width(),
                    header.height(),
                    &palette,
                    &data[header.pixels_offset()..header.pixels_offset() + header.pixels_size()],
                ))
            }
            PicFormat::Format1 => {
                let palette = Self::make_palette(header, data, palette)?;
                DynamicImage::ImageRgba8(Self::decode_format1(
                    header.width(),
                    header.height(),
                    &palette,
                    &data[header.spans_offset()..header.spans_offset() + header.spans_size()],
                    &data[header.pixels_offset()..header.pixels_offset() + header.pixels_size()],
                )?)
            }
        })
    }

    pub fn decode_into(
        system_palette: &Palette,
        into_image: &mut DynamicImage,
        offset_x: u32,
        offset_y: u32,
        pic: &Pic,
        data: &[u8],
    ) -> Result<()> {
        match pic.format {
            PicFormat::Jpeg => bail!("cannot load jpeg into a texture atlas"),
            PicFormat::Format0 => {
                ensure!(
                    pic.palette.is_none(),
                    "format0 image loaded into texture atlas must not have a custom palette"
                );
                Self::decode_format0_into(
                    into_image,
                    offset_x,
                    offset_y,
                    pic.width,
                    system_palette,
                    &data[pic.pixels_offset..pic.pixels_offset + pic.pixels_size],
                );
            }
            PicFormat::Format1 => bail!("cannot load format 1 pic into a texture atlas"),
        }
        Ok(())
    }

    pub fn decode_into_buffer(
        system_palette: &Palette,
        into_buffer: &mut [u8],
        span: usize,
        offset: [u32; 2],
        pic: &Pic,
        data: &[u8],
    ) -> Result<()> {
        match pic.format {
            PicFormat::Jpeg => bail!("cannot load jpeg into a texture atlas"),
            PicFormat::Format0 => {
                ensure!(
                    pic.palette.is_none(),
                    "format0 image loaded into texture atlas must not have a custom palette"
                );
                Self::decode_format0_into_buffer(
                    into_buffer,
                    offset,
                    span,
                    pic.width,
                    system_palette,
                    &data[pic.pixels_offset..pic.pixels_offset + pic.pixels_size],
                );
            }
            PicFormat::Format1 => bail!("cannot load format 1 pic into a texture atlas"),
        }
        Ok(())
    }

    pub fn format(&self) -> PicFormat {
        self.format
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn palette(&self) -> Option<&Palette> {
        self.palette.as_ref()
    }

    pub fn raw_data(&self) -> &[u8] {
        self.pixels_data
    }

    /// Return the raw, palettized pixels if stored directly, or build them from the spans data.
    pub fn pixel_data(&self) -> Result<Cow<'a, [u8]>> {
        Ok(match self.format {
            PicFormat::Format0 => Cow::Borrowed(self.pixels_data),
            PicFormat::Format1 => {
                let mut out = vec![0u8; (self.width * self.height) as usize];
                for span in self.spans {
                    ensure!(span.row() < self.height);
                    ensure!(span.index() < self.pixels_data.len());
                    ensure!(span.start() < self.width);
                    ensure!(span.end() < self.width);
                    ensure!(span.start() <= span.end());
                    ensure!(
                        span.index() + ((span.end() - span.start()) as usize)
                            < self.pixels_data.len()
                    );

                    let start_off = (span.row() * self.width + span.start()) as usize;
                    let end_off = (span.row() * self.width + span.end()) as usize;
                    let cnt = (span.end() - span.start() + 1) as usize;
                    out[start_off..=end_off].copy_from_slice(
                        &self.pixels_data[span.index() as usize..span.index() as usize + cnt],
                    )
                }
                Cow::Owned(out)
            }
            PicFormat::Jpeg => panic!("pixel_data invalid when called on jpeg fmt images"),
        })
    }

    fn make_palette<'b>(
        header: &'b Header,
        data: &'b [u8],
        system_palette: &'b Palette,
    ) -> Result<Cow<'b, Palette>> {
        if header.palette_size() == 0 {
            return Ok(Cow::from(system_palette));
        }

        let palette_data =
            &data[header.palette_offset()..header.palette_offset() + header.palette_size()];
        let local_palette = Palette::from_bytes(palette_data)?;
        let mut palette = system_palette.clone();
        palette.overlay_at(&local_palette, 0)?;
        Ok(Cow::from(palette))
    }

    fn decode_format0(width: u32, height: u32, palette: &Palette, pixels: &[u8]) -> RgbaImage {
        let mut imgbuf = RgbaImage::new(width, height);
        for (i, p) in imgbuf.pixels_mut().enumerate() {
            let pix = pixels[i] as usize;
            let mut clr = palette.rgba(pix);
            if pix == 0xFF {
                clr[3] = 0x00;
            }
            *p = clr;
        }
        imgbuf
    }

    fn decode_format0_into(
        into_image: &mut DynamicImage,
        offset_x: u32,
        offset_y: u32,
        width: u32,
        palette: &Palette,
        pixels: &[u8],
    ) {
        for (index, p) in pixels.iter().enumerate() {
            let i = index as u32;
            let pix = *p as usize;
            let mut clr = palette.rgba(pix);
            if pix == 0xFF {
                clr[3] = 0x00;
            }
            into_image.put_pixel(offset_x + i % width, offset_y + i / width, clr);
        }
    }

    fn decode_format0_into_buffer(
        into_buffer: &mut [u8],
        offset: [u32; 2],
        span: usize,
        width: u32,
        palette: &Palette,
        pixels: &[u8],
    ) {
        for (index, p) in pixels.iter().enumerate() {
            let i = index as u32;
            let pix = *p as usize;
            let mut clr = palette.rgba(pix);
            if pix == 0xFF {
                clr[3] = 0x00;
            }
            let pos = (offset[0] + i % width, offset[1] + i / width);
            let base = 4 * (pos.1 as usize * span + pos.0 as usize);
            // println!(
            //     "i: {}, offset: {:?}, pos: {:?}, base: {}",
            //     i, offset, pos, base
            // );
            into_buffer[base..base + 4].copy_from_slice(&clr.0);
        }
    }

    fn decode_format1(
        width: u32,
        height: u32,
        palette: &Palette,
        spans: &[u8],
        pixels: &[u8],
    ) -> Result<RgbaImage> {
        let mut imgbuf = RgbaImage::new(width, height);
        assert_eq!(spans.len() % mem::size_of::<Span>(), 0);
        let span_cnt = spans.len() / mem::size_of::<Span>() - 1;
        for i in 0..span_cnt {
            let span = Span::overlay(
                &spans[i * mem::size_of::<Span>()..(i + 1) * mem::size_of::<Span>()],
            )?;
            assert!(span.row() < height);
            assert!(span.index() < pixels.len());
            assert!(span.start() < width);
            assert!(span.end() < width);
            assert!(span.start() <= span.end());
            assert!(span.index() + ((span.end() - span.start()) as usize) < pixels.len());

            for (j, column) in (span.start()..=span.end()).enumerate() {
                let offset = span.index() + j;
                let pix = pixels[offset] as usize;
                let clr = palette.rgba(pix);
                imgbuf.put_pixel(column, span.row(), clr);
            }
        }
        Ok(imgbuf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib::Libs;
    use std::{fs, path::Path};

    #[test]
    fn it_can_new_all_pics() -> Result<()> {
        let catalogs = Libs::for_testing()?;
        for (game, catalog) in catalogs.all() {
            for fid in catalog.find_with_extension("PIC")? {
                let meta = catalog.stat(fid)?;
                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                let _img = Pic::from_bytes(&catalog.read(fid)?)?;
            }
        }

        Ok(())
    }

    #[test]
    fn it_can_decode_all_pics() -> Result<()> {
        let libs = Libs::for_testing()?;
        for (game, catalog) in libs.all() {
            let palette = Palette::from_bytes(catalog.read_name("PALETTE.PAL")?.as_ref())?;
            for fid in catalog.find_with_extension("PIC")? {
                let meta = catalog.stat(fid)?;
                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                let img = Pic::decode(&palette, catalog.read(fid)?.as_ref())?;

                if false {
                    let name = format!(
                        "dump/{}/{}.png",
                        game.test_dir,
                        meta.name().split('.').collect::<Vec<_>>().first().unwrap()
                    );
                    let path = Path::new(&name);
                    println!("Write: {}", path.display());
                    let _ = fs::create_dir("dump");
                    let _ = fs::create_dir(path.parent().unwrap());
                    img.save(path)?;
                }
            }
        }

        Ok(())
    }
}
