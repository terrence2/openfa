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
use anyhow::{anyhow, bail, ensure, Result};
use catalog::Catalog;
use image::Pixel;
use lib::CatalogBuilder;
use pal::Palette;
use pic::{Header, Pic};
use rand::Rng;
use std::{fs, fs::File, io::Write, mem, path::PathBuf};
use structopt::StructOpt;
use zerocopy::AsBytes;

/// A PIC authoring tool for Janes Fighters Anthology.
///
/// Examples:
///   Encode file.png into FILE.PIC, using the default palette (PALETTE.PAL) from
///   the game files in the current directory.
///
///   > picpack -o FILE.PIC file.png
///
///   Encode file.png into FILE.PIC, using the default palette from the game files
///   in the directory specified after -g (--game-path).
///
///   > picpack -g "C:\JANES\FA" -o FILE.PIC file.png
///
///   Use the palette from an existing game asset (only PICs are supported at the
///   moment) with -r (--palette-resource) and write the palette that was used into
///   the new image by passing the -p (--include-palette) flag.
///
///   > picpack -r CHOOSEAC.PIC -p -o FILE.PIC file.png
///
///   Use the -w (--include-row-headers) option flag to create a PIC file suitable
///   for use as a texture. Texture PICs in FA contain a blob of pre-multiplied row
///   offsets, presumably to make texturing operations faster to compute on older
///   computers.
///
///   > picpack -w -o TEXTURE.PIC texture.png
///
///   Use a palette from a file in the file system with -f (--palette-file). You
///   can use `picdump` to extract palette data from existing PIC files and store
///   them into new palette files.
///
///   > picpack -f paldump.PAL -o FILE.PIC file.png
///
/// This tool supports many source image formats. See the complete list at:
///   https://docs.rs/image/0.21.0/image/
#[derive(Debug, StructOpt)]
struct Opt {
    /// Save the new PIC file here
    #[structopt(
        short = "o",
        long = "output",
        default_value = "OUT.PIC",
        parse(from_os_str)
    )]
    output: PathBuf,

    /// Dithering "quality" adjustment. Increasing this will give better color
    /// accuracy at the cost of increased grainyness.
    #[structopt(short = "q", long = "quality", default_value = "5")]
    dither_quality: u8,

    /// Use a palette from a PAL file
    #[structopt(short = "f", long = "palette-file", parse(from_os_str))]
    palette_file: Option<PathBuf>,

    /// Use a palette from assets in the game directory at path
    #[structopt(short = "g", long = "game-path")]
    game_path: Option<PathBuf>,

    /// Use a palette from the given resource
    #[structopt(short = "r", long = "palette-resource")]
    palette_resource: Option<String>,

    /// Include row-head information so that the PIC can be used as a texture.
    #[structopt(short = "w", long = "include-row-heads")]
    include_row_heads: bool,

    /// Include the palette in the image. This may or may not be desirable
    /// depending on the intended usage of the image.
    #[structopt(short = "p", long = "include-palette")]
    include_palette: bool,

    /// Dump a copy of the PIC out as a JPG to help with debugging.
    #[structopt(short = "d", long = "debug")]
    dump_debug: bool,

    /// A source image to encode into a PIC
    #[structopt(parse(from_os_str))]
    source_image: PathBuf,
}

fn load_palette_from_resource(catalog: &Catalog, resource_name: &str) -> Result<Palette> {
    let data = catalog.read_name_sync(resource_name)?;
    if resource_name.to_uppercase().ends_with("PAL") {
        return Palette::from_bytes(&data);
    }
    Pic::from_bytes(&data)?
        .palette()
        .cloned()
        .ok_or_else(|| anyhow!("expected non-palette resource to contain a palette"))
}

fn load_palette(opt: &Opt) -> Result<Palette> {
    if let Some(ref filename) = opt.palette_file {
        let pal_data = fs::read(&filename)?;
        return Palette::from_bytes(&pal_data);
    }

    let resource_name = if let Some(ref s) = opt.palette_resource {
        s
    } else {
        "PALETTE.PAL"
    };

    // FIXME: support a game dir properly
    let (catalog, inputs) = CatalogBuilder::build_and_select(&[resource_name.to_owned()])?;
    if inputs.len() != 1 {
        bail!("expected exactly one input");
    }
    let fid = *inputs.first().expect("one input");
    let meta = catalog.stat_sync(fid)?;
    load_palette_from_resource(&catalog, meta.name())
}

fn find_closest_dithered(top: &[(usize, usize)]) -> usize {
    let sum = top.iter().fold(0, |acc, (x, _)| acc + x);
    let inverted = top
        .iter()
        .map(|(x, i)| (sum.checked_div(*x).unwrap_or(sum), i))
        .collect::<Vec<_>>();
    let sum = inverted.iter().fold(0, |acc, (x, _)| acc + x);
    let f: usize = rand::thread_rng().gen_range(0, sum.max(1));
    let mut acc = 0;
    for (x, i) in inverted {
        acc += x;
        if f < acc {
            return *i;
        }
    }
    top[0].1
}

fn find_closest_in_palette(color: image::Rgba<u8>, pal: &Palette, quality: u8) -> u8 {
    let mut dists = pal
        .iter()
        .enumerate()
        .map(|(i, pal_color)| (distance_squared(color, pal_color.to_rgb()), i))
        .collect::<Vec<(usize, usize)>>();
    dists.sort_by_key(|&(d, _)| d);
    let index = find_closest_dithered(&dists[0..quality as usize]);
    index as u8
}

fn distance_squared(c: image::Rgba<u8>, p: image::Rgb<u8>) -> usize {
    if c[3] < 255 {
        if p[0] == 0xFF && p[1] == 0 && p[2] == 0xFF {
            return 0;
        }
        return usize::max_value();
    }
    let dr = p[0] as isize - c[0] as isize;
    let dg = p[1] as isize - c[1] as isize;
    let db = p[2] as isize - c[2] as isize;
    (dr * dr + dg * dg + db * db) as usize
}

fn compute_pixels(buffer: image::RgbaImage, pal: &Palette, quality: u8) -> Vec<u8> {
    let dim = buffer.dimensions();
    let mut pix = Vec::with_capacity((dim.0 * dim.1) as usize);
    for c in buffer.pixels() {
        let index = find_closest_in_palette(*c, pal, quality);
        pix.push(index);
    }
    pix
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    ensure!(
        opt.dither_quality >= 1,
        "dither quality must be between 1 and 255"
    );

    let dynamic_image = image::open(&opt.source_image)?;
    let buffer = dynamic_image.to_rgba8();
    let dim = buffer.dimensions();

    let pal = load_palette(&opt)?;
    let pal_bytes = pal.as_bytes();

    let pix_offset = mem::size_of::<Header>() as u32;
    let pix_size = (dim.0 * dim.1) as u32;
    let (pal_offset, rh_offset) = if opt.include_palette {
        let pal_offset = pix_offset + pix_size;
        if opt.include_row_heads {
            (pal_offset, pal_offset + pal_bytes.len() as u32)
        } else {
            (pal_offset, 0)
        }
    } else if opt.include_row_heads {
        (0, pix_offset + pix_size)
    } else {
        (0, 0)
    };
    let pal_size = if pal_offset != 0 {
        pal_bytes.len() as u32
    } else {
        0
    };
    let rh_size = if rh_offset != 0 { dim.1 * 4 } else { 0 };

    let pic_header = Header::build(
        0,          // format: u16,
        dim.0,      // width: u32,
        dim.1,      // height: u32,
        pix_offset, // pixels_offset: u32 as usize,
        pix_size,   // pixels_size: u32 as usize,
        pal_offset, // palette_offset: u32 as usize,
        pal_size,   // palette_size: u32 as usize,
        0,          // spans_offset: u32 as usize,
        0,          // spans_size: u32 as usize,
        rh_offset,  // rowheads_offset: u32 as usize,
        rh_size,    // rowheads_size: u32 as usize
    )?;
    let pix = compute_pixels(buffer, &pal, opt.dither_quality);

    let mut fp = File::create(&opt.output)?;
    fp.write_all(pic_header.as_bytes())?;
    fp.write_all(&pix)?;
    if opt.include_palette {
        fp.write_all(&pal_bytes)?;
    }
    if opt.include_row_heads {
        let mut off: u32 = pix_offset;
        for _ in 0..dim.1 {
            let bytes: [u8; 4] = unsafe { mem::transmute(off.to_le()) };
            fp.write_all(&bytes)?;
            off += dim.1;
        }
    }

    if opt.dump_debug {
        let pic_data = std::fs::read(&opt.output)?;
        let roundtrip = Pic::decode(&pal, &pic_data)?;
        roundtrip.save(opt.output.with_extension("jpg"))?;
    }

    Ok(())
}
