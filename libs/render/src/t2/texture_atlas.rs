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
use failure::{bail, ensure, Fallible};
use image::{DynamicImage, GenericImage, GenericImageView};
use log::trace;
use mm::{MapOrientation, TLoc};
use std::collections::HashMap;

pub struct TexCoord {
    pub s: f32,
    pub t: f32,
}

pub struct Frame {
    pub coord0: TexCoord,
    pub coord1: TexCoord,
}

impl Frame {
    pub fn interp(&self, fs: f32, ft: f32, orientation: &MapOrientation) -> Fallible<[f32; 2]> {
        match orientation {
            MapOrientation::Unk0 => {
                let (s0, s1, t1, t0) = (self.coord0.s, self.coord1.s, self.coord0.t, self.coord1.t);
                Ok([s0 + ((s1 - s0) * fs), t0 + ((t1 - t0) * ft)])
            }
            MapOrientation::Unk1 => {
                let (s0, s1, t0, t1) = (self.coord1.s, self.coord0.s, self.coord1.t, self.coord0.t);
                Ok([s0 + ((s1 - s0) * ft), t0 + ((t1 - t0) * fs)])
            }
            MapOrientation::FlipS => {
                let (s0, s1, t0, t1) = (self.coord1.s, self.coord0.s, self.coord0.t, self.coord1.t);
                Ok([s0 + ((s1 - s0) * fs), t0 + ((t1 - t0) * ft)])
            }
            MapOrientation::RotateCCW => {
                let (s0, s1, t0, t1) = (self.coord0.s, self.coord1.s, self.coord0.t, self.coord1.t);
                Ok([s0 + ((s1 - s0) * ft), t0 + ((t1 - t0) * fs)])
            }
        }
    }
}

pub struct TextureAtlas {
    pub img: DynamicImage,
    pub frames: HashMap<TLoc, Frame>,
}

impl TextureAtlas {
    pub fn new(sources: Vec<(TLoc, DynamicImage)>) -> Fallible<Self> {
        ensure!(!sources.is_empty(), "cannot create atlas with no textures");
        let mut uniform = false;
        if let Some((TLoc::Index(_), _)) = sources.iter().next() {
            uniform = true;
        }

        if uniform {
            Self::pack_trivial(256, sources)
        } else {
            Self::pack_complex(sources)
        }
    }

    // Most terrains all use 256x256 images, so
    fn pack_trivial(size: u32, sources: Vec<(TLoc, DynamicImage)>) -> Fallible<Self> {
        let num_across = (sources.len() as f64).sqrt().ceil() as u32;
        let extra = num_across * num_across - sources.len() as u32;
        let num_down = num_across - (extra / num_across);

        let atlas_width = (num_across * size) + num_across + 1;
        let atlas_height = (num_down * size) + num_down + 1;

        trace!(
            "t2::TextureAtlas::trivial: {} images, {} across, {}x{} pixels",
            sources.len(),
            num_across,
            atlas_width,
            atlas_height
        );
        let mut img = DynamicImage::new_rgba8(atlas_width, atlas_height);
        let mut frames = HashMap::new();
        let mut cursor_x = 1;
        let mut cursor_y = 1;
        for (tloc, src) in &sources {
            let coord0 = TexCoord {
                s: cursor_x as f32 / atlas_width as f32,
                t: cursor_y as f32 / atlas_height as f32,
            };
            let coord1 = TexCoord {
                s: (cursor_x + size) as f32 / atlas_width as f32,
                t: (cursor_y + size) as f32 / atlas_height as f32,
            };
            frames.insert(tloc.to_owned(), Frame { coord0, coord1 });

            trace!(
                "t2::TextureAtlas::trivial: {:?} @ {}x{}",
                tloc,
                cursor_x,
                cursor_y
            );
            img.copy_from(src, cursor_x, cursor_y);

            cursor_x += size + 1;
            if cursor_x >= atlas_width {
                cursor_x = 1;
                cursor_y += size + 1;
            }
        }

        Ok(Self { img, frames })
    }

    fn pack_complex(mut sources: Vec<(TLoc, DynamicImage)>) -> Fallible<Self> {
        sources.sort_unstable_by(|a, b| a.1.width().cmp(&b.1.width()).reverse());
        for (tloc, src) in &sources {
            println!("{:?}: {}", tloc, src.width());
        }

        let mut count256 = 0;
        let mut count128 = 0;
        for (_, src) in &sources {
            if src.width() == 256 {
                count256 += 1;
                count128 += 1;
            }
        }

        ensure!(
            count128 % 4 == 0,
            "expected count of 128x128 images to be divisible by 4"
        );
        let square_count = count256 + (count128 / 4);
        let num_across = f64::from(square_count).sqrt().ceil() as u32;
        let extra = num_across * num_across - square_count as u32;
        let num_down = num_across - (extra / num_across);

        let size = 256;
        let atlas_width = (num_across * size) + (2 * num_across) + 2;
        let atlas_height = (num_down * size) + (2 * num_down) + 2;

        trace!(
            "t2::TextureAtlas::complex: {} squares, {} across, {}x{} pixels",
            square_count,
            num_across,
            atlas_width,
            atlas_height
        );

        let mut img = DynamicImage::new_rgba8(atlas_width, atlas_height);
        let mut frames = HashMap::new();
        let mut cursor_x = 1;
        let mut cursor_y = 1;
        let mut offset128 = 0;
        for (tloc, src) in &sources {
            let mut target_x = cursor_x;
            let mut target_y = cursor_y;
            if src.width() == 128 {
                match offset128 {
                    0 => {}
                    1 => target_x += 129,
                    2 => target_y += 129,
                    3 => {
                        target_x += 129;
                        target_y += 129;
                    }
                    _ => bail!("offset128 out of range"),
                }
                offset128 = (offset128 + 1) % 4;
            }

            let coord0 = TexCoord {
                s: target_x as f32 / atlas_width as f32,
                t: target_y as f32 / atlas_height as f32,
            };
            let coord1 = TexCoord {
                s: (target_x + size) as f32 / atlas_width as f32,
                t: (target_y + size) as f32 / atlas_height as f32,
            };
            frames.insert(tloc.to_owned(), Frame { coord0, coord1 });

            trace!(
                "t2::TextureAtlas::complex: {:?} @ {}x{}",
                tloc,
                target_x,
                target_y
            );
            img.copy_from(src, target_x, target_y);

            cursor_x += size + 2;
            if cursor_x >= atlas_width {
                cursor_x = 1;
                cursor_y += size + 2;
            }
        }

        Ok(Self { img, frames })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use asset::AssetManager;
    use lay::Layer;
    use mm::MissionMap;
    use omnilib::OmniLib;
    use pal::Palette;
    use pic::decode_pic;
    use std::path::Path;
    use xt::TypeManager;

    #[test]
    fn test_it_works() -> Fallible<()> {
        //use simplelog::{Config, LevelFilter, TermLogger};
        //TermLogger::init(LevelFilter::Trace, Config::default())?;

        if !Path::new("dump").exists() {
            std::fs::create_dir("dump")?;
        }

        let omni = OmniLib::new_for_test_in_games(&[
            "FA", "USNF97", "ATFGOLD", "ATFNATO", "ATF", "MF", "USNF",
        ])?;
        for (game, name) in omni.find_matching("*.T2")?.iter() {
            let name = &(name[0..name.len() - 2].to_owned() + "MM");

            if name == "$VARF.MM" {
                // This looks a fragment of an MM used for... something?
                continue;
            }

            println!(
                "At: {}:{} @ {}",
                game,
                name,
                omni.path(game, name).unwrap_or("<unknown>".to_owned())
            );
            let lib = omni.library(game);
            let assets = AssetManager::new(lib.clone())?;
            let types = TypeManager::new(lib.clone());
            let contents = lib.load_text(name)?;
            let mm = MissionMap::from_str(&contents, &types)?;

            let layer = assets.load_lay(&mm.layer_name.to_uppercase())?;

            let mut pic_data = HashMap::new();
            let base_name = mm.get_base_texture_name()?;
            for (_pos, tmap) in &mm.tmaps {
                if !pic_data.contains_key(&tmap.loc) {
                    let name = tmap.loc.pic_file(&base_name);
                    pic_data.insert(tmap.loc.clone(), lib.load(&name)?.to_vec());
                }
            }
            let base_palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;
            let palette = load_palette(&base_palette, &layer, mm.layer_index)?;

            let all_texture_start_coords = mm
                .tmaps
                .keys()
                .map(|&v| v.clone())
                .collect::<Vec<(u32, u32)>>();
            for &(x, y) in &all_texture_start_coords {
                assert_eq!(x % 4, 0);
                assert_eq!(y % 4, 0);
            }

            // Load all images with our new palette.
            let mut pics = Vec::new();
            for (tloc, data) in &pic_data {
                let pic = decode_pic(&palette, data)?;
                pics.push((tloc.clone(), pic));
            }

            let _atlas = TextureAtlas::new(pics)?;
            // atlas
            //     .img
            //     .save(&format!("dump/atlas-{}-{}.png", game, base_name))?;
        }

        Ok(())
    }

    fn load_palette(
        base_palette: &Palette,
        layer: &Layer,
        layer_index: usize,
    ) -> Fallible<Palette> {
        // Note: we need to really find the right palette.
        let mut palette = base_palette.clone();
        let layer_data = layer.for_index(layer_index + 2, 0)?;
        let r0 = layer_data.slice(0x00, 0x10)?;
        let r1 = layer_data.slice(0x10, 0x20)?;
        let r2 = layer_data.slice(0x20, 0x30)?;
        let r3 = layer_data.slice(0x30, 0x40)?;

        // We need to put rows r0, r1, and r2 into into 0xC0, 0xE0, 0xF0 somehow.
        palette.overlay_at(&r1, 0xF0)?;
        palette.overlay_at(&r0, 0xE0)?;

        // I'm pretty sure this is correct.
        palette.overlay_at(&r3, 0xD0)?;

        palette.overlay_at(&r2, 0xC0)?;
        //palette.overlay_at(&r2, 0xC1)?;

        Ok(palette)
    }
}
