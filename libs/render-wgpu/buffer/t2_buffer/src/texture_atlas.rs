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

impl TexCoord {
    pub fn new(x: u32, y: u32, img: &DynamicImage) -> Self {
        Self {
            s: x as f32 / img.width() as f32,
            t: y as f32 / img.height() as f32,
        }
    }
}

pub struct Frame {
    pub coord0: TexCoord,
    pub coord1: TexCoord,
}

impl Frame {
    pub fn interp(&self, fs: f32, ft: f32, orientation: &MapOrientation) -> [f32; 2] {
        match orientation {
            MapOrientation::Unk0 => {
                let (s0, s1, t1, t0) = (self.coord0.s, self.coord1.s, self.coord0.t, self.coord1.t);
                [s0 + ((s1 - s0) * fs), t0 + ((t1 - t0) * ft)]
            }
            MapOrientation::Unk1 => {
                let (s0, s1, t0, t1) = (self.coord1.s, self.coord0.s, self.coord1.t, self.coord0.t);
                [s0 + ((s1 - s0) * ft), t0 + ((t1 - t0) * fs)]
            }
            MapOrientation::FlipS => {
                let (s0, s1, t0, t1) = (self.coord1.s, self.coord0.s, self.coord0.t, self.coord1.t);
                [s0 + ((s1 - s0) * fs), t0 + ((t1 - t0) * ft)]
            }
            MapOrientation::RotateCCW => {
                let (s0, s1, t0, t1) = (self.coord0.s, self.coord1.s, self.coord0.t, self.coord1.t);
                [s0 + ((s1 - s0) * ft), t0 + ((t1 - t0) * fs)]
            }
        }
    }
}

// Size of a texture patch.
const PATCH_SIZE: u32 = 256;
const HALF_SIZE: u32 = 128;

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
            Self::pack_trivial(sources)
        } else {
            Self::pack_complex(sources)
        }
    }

    // Most terrains all use 256x256 images, so
    fn pack_trivial(sources: Vec<(TLoc, DynamicImage)>) -> Fallible<Self> {
        let num_across = (sources.len() as f64).sqrt().ceil() as u32;
        let extra = num_across * num_across - sources.len() as u32;
        let num_down = num_across - (extra / num_across);

        let atlas_width = (num_across * PATCH_SIZE) + num_across + 1;
        let atlas_height = (num_down * PATCH_SIZE) + num_down + 1;

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
            let coord0 = TexCoord::new(cursor_x, cursor_y, &img);
            let coord1 = TexCoord::new(cursor_x + PATCH_SIZE, cursor_y + PATCH_SIZE, &img);
            frames.insert(tloc.to_owned(), Frame { coord0, coord1 });
            img.copy_from(src, cursor_x, cursor_y);

            cursor_x += PATCH_SIZE + 1;
            if cursor_x >= atlas_width {
                cursor_x = 1;
                cursor_y += PATCH_SIZE + 1;
            }
        }

        Ok(Self { img, frames })
    }

    fn pack_complex(mut sources: Vec<(TLoc, DynamicImage)>) -> Fallible<Self> {
        sources.sort_unstable_by(|a, b| a.1.width().cmp(&b.1.width()).reverse());
        let count256 = sources.iter().filter(|(_, img)| img.width() == 256).count();
        let count128 = sources.len() - count256;

        ensure!(
            count128 % 4 == 0,
            "expected count of 128x128 images to be divisible by 4"
        );
        let square_count = count256 + (count128 / 4);
        let num_across = (square_count as f64).sqrt().ceil() as u32;
        let extra = num_across * num_across - square_count as u32;
        let num_down = num_across - (extra / num_across);

        let atlas_width = (num_across * (PATCH_SIZE + 2)) + 2;
        let atlas_height = (num_down * (PATCH_SIZE + 2)) + 2;

        trace!(
            "t2::TextureAtlas::complex: {} 128px, {} 256px, {} total squares, {} across, {}x{} pixels",
            count128,
            count256,
            square_count,
            num_across,
            atlas_width,
            atlas_height
        );

        let mut img = DynamicImage::new_rgba8(atlas_width, atlas_height);
        let mut frames = HashMap::new();
        let mut cursor_x = 1;
        let mut cursor_y = 1;
        for (tloc, src) in &sources[..count256] {
            ensure!(src.width() == 256, "in 256 partition");
            let coord0 = TexCoord::new(cursor_x, cursor_y, &img);
            let coord1 = TexCoord::new(cursor_x + PATCH_SIZE, cursor_y + PATCH_SIZE, &img);
            frames.insert(tloc.to_owned(), Frame { coord0, coord1 });
            img.copy_from(src, cursor_x, cursor_y);
            cursor_x += PATCH_SIZE + 2;
            if (cursor_x + 1) >= atlas_width {
                cursor_x = 1;
                cursor_y += PATCH_SIZE + 2;
            }
        }

        let mut offset128 = 0;
        for (tloc, src) in &sources[count256..] {
            ensure!(src.width() == 128, "in 128 partition");
            let mut target_x = cursor_x;
            let mut target_y = cursor_y;
            match offset128 {
                0 => {}
                1 => target_x += HALF_SIZE + 1,
                2 => target_y += HALF_SIZE + 1,
                3 => {
                    target_x += HALF_SIZE + 1;
                    target_y += HALF_SIZE + 1;
                    cursor_x += PATCH_SIZE + 2;
                }
                _ => bail!("offset128 out of range"),
            }
            offset128 = (offset128 + 1) % 4;

            let coord0 = TexCoord::new(target_x, target_y, &img);
            let coord1 = TexCoord::new(target_x + HALF_SIZE, target_y + HALF_SIZE, &img);
            frames.insert(tloc.to_owned(), Frame { coord0, coord1 });
            img.copy_from(src, target_x, target_y);
            if (cursor_x + 1) >= atlas_width {
                cursor_x = 1;
                cursor_y += PATCH_SIZE + 2;
            }
        }

        // img.save("texture_atlas.png")?;
        Ok(Self { img, frames })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use lay::Layer;
    use mm::MissionMap;
    use omnilib::OmniLib;
    use pal::Palette;
    use pic::Pic;
    use std::path::Path;
    use xt::TypeManager;

    #[test]
    fn test_t2_texture_atlas() -> Fallible<()> {
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
                omni.path(game, name)
                    .unwrap_or_else(|_| "<unknown>".to_owned())
            );
            let lib = omni.library(game);
            let types = TypeManager::new(lib.clone());
            let contents = lib.load_text(name)?;
            let mm = MissionMap::from_str(&contents, &types, &lib)?;
            let system_palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;
            let layer = Layer::from_bytes(&lib.load(&mm.layer_name())?, &system_palette)?;

            let mut pic_data = HashMap::new();
            let base_name = mm.get_base_texture_name()?;
            for tmap in mm.texture_maps() {
                if !pic_data.contains_key(&tmap.loc) {
                    let name = tmap.loc.pic_file(&base_name);
                    pic_data.insert(tmap.loc.clone(), lib.load(&name)?.to_vec());
                }
            }
            let base_palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;
            let palette = load_palette(&base_palette, &layer, mm.layer_index())?;

            // Load all images with our new palette.
            let mut pics = Vec::new();
            for (tloc, data) in &pic_data {
                let pic = Pic::decode(&palette, data)?;
                pics.push((tloc.clone(), pic));
            }

            let atlas = TextureAtlas::new(pics)?;
            atlas.img.save(&format!(
                "../../../../dump/t2_atlas/atlas-{}-{}.png",
                game, base_name
            ))?;
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
        let layer_data = layer.for_index(layer_index + 2)?;
        let r0 = layer_data.slice(0x00, 0x10)?;
        let r1 = layer_data.slice(0x10, 0x20)?;
        let r2 = layer_data.slice(0x20, 0x30)?;
        let r3 = layer_data.slice(0x30, 0x40)?;

        palette.overlay_at(&r0, 0xE0 - 1)?;
        palette.overlay_at(&r1, 0xF0 - 1)?;
        palette.overlay_at(&r2, 0xC0)?;
        palette.overlay_at(&r3, 0xD0)?;

        Ok(palette)
    }
}
