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
use image;
use lib::Library;
use omnilib::{make_opt_struct, OmniLib};
use pal::Palette;
use pic::{Header, Pic};
use std::{env, fs, fs::File, io::Write, mem, path::PathBuf};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "picpack")] //, about = "")]
/// A PIC authoring tool for Janes Fighters Anthology.
///
/// Examples:
///   Encode file.png into FILE.PIC, using the palette from the game files in the
///   current directory
///
///   > picpack -o FILE.PIC file.png
///
///   Use the -t option flag to create a PIC file suitable for use as a texture.
///   Texture PICs in FA contain a blob of pre-multiplied row offsets, presumably
///   to make texturing operations faster to compute on older computers.
///
///   > picpack -t -o TEXTURE.PIC texture.png
///
///   Encode file.png into FILE.PIC, using the palette from the game files in the
///   directory specified after -g.
///
///   > picpack -g "C:\JANES\FA" -o FILE.PIC file.png
///
///   Use the palette from an existing game asset. Some menu assets do not pack
///   their own palette but depend on the one loaded from the main screen image.
///   This lets the same button images work with multiple different screen textures
///   fluidly.
///
///   > picpack -r CHOOSEAC.PIC -o FILE.PIC file.png
///
///   Use a palette from a file in the file system. You can use picdump to extract
///   palette data from existing PIC files. Depending on the asset, it may or many
///   not be possible to easily extract a palette to use for new image.
///
///   > picpack -f paldump.PAL -o FILE.PIC file.png
///
/// This tool supports many source image formats. See the complete list at:
///   https://docs.rs/image/0.21.0/image/
struct Opt {
    /// Save the new PIC file here
    #[structopt(
        short = "o",
        long = "output",
        default_value = "OUTPUT.PIC",
        parse(from_os_str)
    )]
    output: PathBuf,

    /// Include row-head information so that the PIC can be used as a texture.
    #[structopt(short = "t", long = "texture")]
    is_texture: bool,

    /// Use a palette from a PAL file
    #[structopt(short = "f", long = "palette-file", parse(from_os_str))]
    palette_file: Option<PathBuf>,

    /// Use a palette from assets in the game directory at path
    #[structopt(short = "g", long = "game-path")]
    palette_game_dir: Option<PathBuf>,

    /// Use a palette from the given resource
    #[structopt(short = "r", long = "palette-resource")]
    palette_resource: Option<String>,

    /// Use a palette from the test data directory
    #[structopt(long = "palette-test-resource")]
    palette_test: Option<String>,

    /// A source image to encode into a PIC
    #[structopt(parse(from_os_str))]
    source_image: PathBuf,
}

fn load_palette(opt: &Opt) -> Fallible<Palette> {
    let pal_data = if let Some(ref filename) = opt.palette_file {
        fs::read(&filename)?
    } else if let Some(ref s) = opt.palette_test {
        let omni = OmniLib::new_for_test()?;
        let parts = s.split(":").collect::<Vec<_>>();
        omni.library(parts[0]).load(parts[1])?.into_owned()
    } else {
        let game_dir = if let Some(ref game_dir) = opt.palette_game_dir {
            game_dir.to_owned()
        } else {
            env::current_dir()?
        };
        let lib = Library::from_file_search(&game_dir)?;
        lib.load("PALETTE.PAL")?.into_owned()
    };
    Palette::from_bytes(&pal_data)
}

fn find_closest_in_palette(color: &image::Rgba<u8>, pal: &Palette) -> Fallible<u8> {
    let mut dists = pal
        .iter()
        .enumerate()
        .map(|(i, pal_color)| (distance_squared(color, pal_color), i))
        .collect::<Vec<(u32, usize)>>();
    dists.sort_by_key(|&(d, _)| d);
    let (d, index) = dists.first().unwrap();
    Ok(*index as u8)
}

fn distance_squared(c0: &image::Rgba<u8>, c1: &image::Rgba<u8>) -> u32 {
    let dr = c1[0] as i64 - c0[0] as i64;
    let dg = c1[1] as i64 - c0[1] as i64;
    let db = c1[2] as i64 - c0[2] as i64;
    (dr * dr + dg * dg + db * db) as u32
}

fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    let dynamic_image = image::open(&opt.source_image)?;
    let buffer = dynamic_image.to_rgba();

    let pal = load_palette(&opt)?;

    let dim = buffer.dimensions();
    let pic_header = Header::build(
        0,                               // format: u16,
        dim.0,                           // width: u32,
        dim.1,                           // height: u32,
        mem::size_of::<Header>() as u32, // pixels_offset: u32 as usize,
        dim.0 * dim.1,                   // pixels_size: u32 as usize,
        0,                               // palette_offset: u32 as usize,
        0,                               // palette_size: u32 as usize,
        0,                               // spans_offset: u32 as usize,
        0,                               // spans_size: u32 as usize,
        0,                               // rowheads_offset: u32 as usize,
        0,                               // rowheads_size: u32 as usize
    )?;
    let mut offset = 0;
    let mut pix = Vec::with_capacity((dim.0 * dim.1) as usize);
    for p in buffer.pixels() {
        pix.push(find_closest_in_palette(p, &pal)?);
        offset += 1;
    }

    let mut fp = File::create(&opt.output)?;
    fp.write(pic_header.as_bytes()?)?;
    fp.write(&pix)?;

    // Test by writing back out.
    let omni = OmniLib::new_for_test()?;
    let pal = Palette::from_bytes(&omni.library("FA").load("PALETTE.PAL")?)?;
    let pic = std::fs::read(&opt.output)?;
    let round = Pic::decode(&pal, &pic)?;
    round.save(opt.output.with_extension("png"))?;

    Ok(())
}
