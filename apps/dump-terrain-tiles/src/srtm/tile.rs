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
use absolute_unit::{degrees, meters, Angle, ArcSeconds, Degrees};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use json::JsonValue;
use memmap::{Mmap, MmapOptions};
use std::{
    fmt,
    fs::File,
    path::{Path, PathBuf},
};

const TILE_SIZE: usize = 3601;

pub struct Tile {
    path: PathBuf,

    // Row major, i16, big-endian, meters
    // Voids are marked with i16::MIN
    // Rows are west to east, columns are north to south.
    // Even though the tile origin is on the south side, the data stored north to south.
    data: Mmap,

    // The individual samples represent one arcsecond of terrain. However the tiles are
    // aligned to the *center* of the lower left sample. This means that the actual terrain
    // covered by the edge samples extends half an arcsecond beyond the box. Thus the "corners"
    // box containing the terrain extends beyond the tile extents somewhat.
    terrain_extent: [Graticule<GeoCenter>; 4],

    // Samples, however, should only be made within the tile boundary, which ends at the centers
    // of the corner samples.
    tile_extent: [Graticule<GeoCenter>; 4],

    // Tile indices are derived by rounding the corners.
    latitude: i16,
    longitude: i16,
}

impl Tile {
    pub fn from_feature(feature: &JsonValue, base_path: &Path) -> Fallible<Self> {
        assert_eq!(feature["type"], "Feature");
        let geometry = &feature["geometry"];
        assert_eq!(geometry["type"], "Polygon");
        let mut all_corners = [Default::default(); 5];
        for coordinates in geometry["coordinates"].members() {
            for (i, corner) in coordinates.members().enumerate() {
                let mut m = corner.members();
                let lon = m.next().unwrap().as_f64().unwrap();
                let lat = m.next().unwrap().as_f64().unwrap();
                all_corners[i] = Graticule::new(degrees!(lat), degrees!(lon), meters!(0));
            }
        }
        assert_eq!(all_corners[0], all_corners[4]);
        let mut corners = [Default::default(); 4];
        corners.copy_from_slice(&all_corners[0..4]);

        let mut tile_extent = [Default::default(); 4];
        for (extent, corner) in tile_extent.iter_mut().zip(&corners) {
            *extent = Graticule::<GeoCenter>::new(
                degrees!(corner.lat::<Degrees>().round()),
                degrees!(corner.lon::<Degrees>().round()),
                meters!(0),
            );
        }

        let latitude = corners[0].lat::<Degrees>().f32().round() as i16;
        let longitude = corners[0].lon::<Degrees>().f32().round() as i16;
        assert_eq!(latitude, corners[0].lat::<Degrees>().ceil() as i16);
        assert_eq!(latitude, corners[2].lat::<Degrees>().floor() as i16 - 1);
        assert_eq!(longitude, corners[0].lon::<Degrees>().ceil() as i16);
        assert_eq!(longitude, corners[2].lon::<Degrees>().floor() as i16 - 1);

        let tile_name = &feature["properties"]["dataFile"];
        let tile_zip_filename = PathBuf::from(tile_name.as_str().unwrap());
        let tile_filename = PathBuf::from(
            PathBuf::from(
                PathBuf::from(tile_zip_filename.file_stem().unwrap())
                    .file_stem()
                    .unwrap(),
            )
            .file_stem()
            .unwrap(),
        )
        .with_extension("hgt");
        let mut path = PathBuf::from(base_path);
        path.push("tiles_unpacked");
        path.push(&tile_filename);

        // println!("path: {:?}: {}x{}", path, latitude, longitude);
        let file = File::open(&path)?;
        let data = unsafe { MmapOptions::new().map(&file)? };

        Ok(Self {
            path,
            data,
            terrain_extent: corners,
            tile_extent,
            latitude,
            longitude,
        })
    }

    fn at(&self, row: usize, col: usize) -> i16 {
        assert!(row < TILE_SIZE);
        assert!(col < TILE_SIZE);
        let sample_offset = row * TILE_SIZE + col;
        let offset = sample_offset * 2;
        let hi = self.data[offset] as u16;
        let lo = self.data[offset + 1] as u16;
        (hi << 8 | lo) as i16
    }

    fn length(&self, x: f64, y: f64) -> f64 {
        (x * x + y * y).sqrt()
    }

    // Does linear weighting between the 4 nearest samples.
    #[allow(unused)]
    pub fn sample_linear(&self, grat: &Graticule<GeoCenter>) -> f32 {
        assert!(grat.lon::<Degrees>() > self.tile_extent[0].lon());
        assert!(grat.lon::<Degrees>() > self.tile_extent[1].lon());
        assert!(grat.lon::<Degrees>() < self.tile_extent[2].lon());
        assert!(grat.lon::<Degrees>() < self.tile_extent[3].lon());
        assert!(grat.lat::<Degrees>() > self.tile_extent[0].lat());
        assert!(grat.lat::<Degrees>() < self.tile_extent[1].lat());
        assert!(grat.lat::<Degrees>() < self.tile_extent[2].lat());
        assert!(grat.lat::<Degrees>() > self.tile_extent[3].lat());

        let row_abs = (grat.lat::<ArcSeconds>() - self.tile_extent[0].lat::<ArcSeconds>()).f64();
        let row_snap = row_abs.floor();
        let row_delta = row_abs - row_snap;
        let row0 = row_snap as usize;
        assert!(row0 + 1 < TILE_SIZE);

        let col_abs = (grat.lon::<ArcSeconds>() - self.tile_extent[0].lon::<ArcSeconds>()).f64();
        let col_snap = col_abs.floor();
        let col_delta = col_abs - col_snap;
        let col0 = col_snap as usize;
        assert!(col0 + 1 < TILE_SIZE);

        let a = self.at(row0, col0) as f64;
        let b = self.at(row0 + 1, col0) as f64;
        let c = self.at(row0 + 1, col0 + 1) as f64;
        let d = self.at(row0, col0 + 1) as f64;

        let mut af = self.length(row_delta, col_delta);
        let mut bf = self.length(1f64 - row_delta, col_delta);
        let mut cf = self.length(1f64 - row_delta, 1f64 - col_delta);
        let mut df = self.length(row_delta, 1f64 - col_delta);

        let mag = af + bf + cf + df;
        af /= mag;
        bf /= mag;
        cf /= mag;
        df /= mag;

        let sample = a * af + b * bf + c * cf + d * df;
        sample as f32
    }

    // If our sampling algorithm is careful to line up on arcsecond boundaries, then we can get
    // away with a much cheaper lookup (about 2x faster).
    #[allow(unused)]
    pub fn sample_nearest(&self, grat: &Graticule<GeoCenter>) -> i16 {
        // println!(
        //     "  TSN: {} in {}, {}, {}, {}",
        //     grat,
        //     self.tile_extent[0],
        //     self.tile_extent[1],
        //     self.tile_extent[2],
        //     self.tile_extent[3]
        // );
        assert!(grat.lon::<Degrees>() >= self.tile_extent[0].lon());
        assert!(grat.lon::<Degrees>() < self.tile_extent[1].lon());
        assert!(grat.lon::<Degrees>() < self.tile_extent[2].lon());
        assert!(grat.lon::<Degrees>() >= self.tile_extent[3].lon());
        assert!(grat.lat::<Degrees>() >= self.tile_extent[0].lat());
        assert!(grat.lat::<Degrees>() >= self.tile_extent[1].lat());
        assert!(grat.lat::<Degrees>() < self.tile_extent[2].lat());
        assert!(grat.lat::<Degrees>() < self.tile_extent[3].lat());

        let row = TILE_SIZE
            - (grat.lat::<ArcSeconds>() - self.tile_extent[0].lat::<ArcSeconds>()).round() as usize
            - 1;
        let col =
            (grat.lon::<ArcSeconds>() - self.tile_extent[0].lon::<ArcSeconds>()).round() as usize;
        assert!(row < TILE_SIZE);
        assert!(col < TILE_SIZE);

        self.at(row, col)
    }

    pub fn index(a: Angle<Degrees>) -> i16 {
        let f = a.f64().floor();
        assert!(f >= -180.0);
        assert!(f <= 180.0);
        f as i16
    }

    pub fn latitude(&self) -> i16 {
        self.latitude
    }

    pub fn longitude(&self) -> i16 {
        self.longitude
    }

    #[allow(unused)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[allow(unused)]
    pub fn corners(&self) -> &[Graticule<GeoCenter>; 4] {
        &self.terrain_extent
    }
}

impl fmt::Display for Tile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "({} - {})",
            self.terrain_extent[0], self.terrain_extent[2]
        )
    }
}
