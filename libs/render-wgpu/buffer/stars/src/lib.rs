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
use failure::Fallible;
//use global_layout::GlobalSets;
use gpu::{GPUConfig, GPU};
use log::trace;
use nalgebra::Vector3;
use star_catalog::Stars;
use std::{collections::HashSet, f32::consts::PI, mem, sync::Arc};

const TAU: f32 = PI * 2f32;
const PI_2: f32 = PI / 2f32;
const RADIUS: f32 = 0.0015f32;

#[derive(Copy, Clone)]
struct BandMetadata {
    index: u32,
    bins_per_row: u32,
    base_index: u32,
    _pad: u32,
}

#[derive(Copy, Clone)]
struct BinPosition {
    index_base: u32,
    num_indexes: u32,
}

#[derive(Copy, Clone)]
struct StarInst {
    ra: f32,
    dec: f32,
    color: [f32; 3],
    radius: f32,
}

macro_rules! mkband {
    ($index:expr, $bins_per_row:expr, $base_index:expr) => {
        BandMetadata {
            index: $index,
            bins_per_row: $bins_per_row,
            base_index: $base_index,
            _pad: 0,
        }
    };
}

const DEC_BINS: usize = 64;
const DEC_BANDS: [BandMetadata; DEC_BINS] = [
    mkband!(0, 1, 0),
    mkband!(1, 8, 1),
    mkband!(2, 12, 9),
    mkband!(3, 16, 21),
    mkband!(4, 32, 37),
    mkband!(5, 32, 69),
    mkband!(6, 32, 101),
    mkband!(7, 32, 133),
    mkband!(8, 64, 165),
    mkband!(9, 64, 229),
    mkband!(10, 64, 293),
    mkband!(11, 64, 357),
    mkband!(12, 84, 421),
    mkband!(13, 84, 505),
    mkband!(14, 84, 589),
    mkband!(15, 84, 673),
    mkband!(16, 84, 757),
    mkband!(17, 84, 841),
    mkband!(18, 128, 925),
    mkband!(19, 128, 1053),
    mkband!(20, 128, 1181),
    mkband!(21, 128, 1309),
    mkband!(22, 128, 1437),
    mkband!(23, 128, 1565),
    mkband!(24, 128, 1693),
    mkband!(25, 128, 1821),
    mkband!(26, 128, 1949),
    mkband!(27, 128, 2077),
    mkband!(28, 128, 2205),
    mkband!(29, 128, 2333),
    mkband!(30, 128, 2461),
    mkband!(31, 128, 2589),
    mkband!(32, 128, 2717),
    mkband!(33, 128, 2845),
    mkband!(34, 128, 2973),
    mkband!(35, 128, 3101),
    mkband!(36, 128, 3229),
    mkband!(37, 128, 3357),
    mkband!(38, 128, 3485),
    mkband!(39, 128, 3613),
    mkband!(40, 128, 3741),
    mkband!(41, 128, 3869),
    mkband!(42, 128, 3997),
    mkband!(43, 128, 4125),
    mkband!(44, 128, 4253),
    mkband!(45, 128, 4381),
    mkband!(46, 84, 4509),
    mkband!(47, 84, 4593),
    mkband!(48, 84, 4677),
    mkband!(49, 84, 4761),
    mkband!(50, 84, 4845),
    mkband!(51, 84, 4929),
    mkband!(52, 64, 5013),
    mkband!(53, 64, 5077),
    mkband!(54, 64, 5141),
    mkband!(55, 64, 5205),
    mkband!(56, 32, 5269),
    mkband!(57, 32, 5301),
    mkband!(58, 32, 5333),
    mkband!(59, 32, 5365),
    mkband!(60, 16, 5397),
    mkband!(61, 12, 5413),
    mkband!(62, 8, 5425),
    mkband!(63, 1, 5433),
];

pub struct StarsBuffers {
    band_buffer: wgpu::Buffer,
    band_buffer_size: u64,
    bin_positions_buffer: wgpu::Buffer,
    bin_positions_buffer_size: u64,
    star_indices_buffer: wgpu::Buffer,
    star_indices_buffer_size: u64,
    star_buffer: wgpu::Buffer,
    star_buffer_size: u64,
}

impl StarsBuffers {
    pub fn band_metadata_bind_group_layout_binding(binding: u32) -> wgpu::BindGroupLayoutBinding {
        wgpu::BindGroupLayoutBinding {
            binding,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::StorageBuffer {
                dynamic: false,
                readonly: true,
            },
        }
    }

    pub fn band_metadata_binding(&self, binding: u32) -> wgpu::Binding {
        wgpu::Binding {
            binding,
            resource: wgpu::BindingResource::Buffer {
                buffer: &self.band_buffer,
                range: 0..self.band_buffer_size,
            },
        }
    }

    fn get_perpendicular(v: &Vector3<f32>) -> Vector3<f32> {
        let not_v = if v[2] > v[1] {
            Vector3::new(0f32, 1f32, 0f32)
        } else {
            Vector3::new(0f32, 0f32, 1f32)
        };
        v.cross(&not_v)
    }

    fn ra_d_to_vec(ra: f32, dec: f32) -> Vector3<f32> {
        Vector3::new(dec.cos() * ra.sin(), -dec.sin(), dec.cos() * ra.cos())
    }

    fn vec_to_ra_d(v: &Vector3<f32>) -> (f32, f32) {
        let ra = v.x.atan2(v.z) + PI;
        let w = (v.x * v.x + v.z * v.z).sqrt();
        let dec = v.y.atan2(w);
        (ra, dec)
    }

    fn num_bins() -> usize {
        let last_band = &DEC_BANDS[DEC_BANDS.len() - 1];
        (last_band.base_index + last_band.bins_per_row) as usize
    }

    fn band_for_dec(dec: f32) -> &'static BandMetadata {
        assert!(dec < PI_2);
        assert!(dec >= -PI_2);
        // See implementation in shader for comments.
        let decz = ((dec + PI_2) * 2f32) / TAU;
        let deci = (decz * DEC_BINS as f32) as usize;
        &DEC_BANDS[deci]
    }

    fn bin_for_ra_d(ra: f32, dec: f32) -> usize {
        let band = Self::band_for_dec(dec);
        let raz = ra / TAU;
        let rai = (band.bins_per_row as f32 * raz) as usize;
        band.base_index as usize + rai
    }

    pub fn new(device: &wgpu::Device) -> Fallible<Self> {
        trace!("StarsBuffers::new");

        let mut offset = 0;
        for (i, band) in DEC_BANDS.iter().enumerate() {
            assert_eq!(band.index as usize, i);
            assert_eq!(band.base_index, offset);
            offset += band.bins_per_row;
        }

        let mut star_buf = Vec::new();
        let stars = Stars::new()?;
        for i in 0..stars.catalog_size() {
            let entry = stars.entry(i)?;
            let ra = entry.right_ascension() as f32;
            let dec = entry.declination() as f32;
            let color = entry.color();
            let radius = RADIUS * entry.radius_scale();
            let star = StarInst {
                ra,
                dec,
                color,
                radius,
            };
            star_buf.push(star);
        }

        // Bin all stars.
        let mut bins = Vec::with_capacity(Self::num_bins());
        bins.resize_with(Self::num_bins(), HashSet::new);
        for (star_off, star) in star_buf.iter().enumerate() {
            // Sample bins in a circle at the star's approximate radius.
            let star_ray = Self::ra_d_to_vec(star.ra, star.dec);
            let perp = Self::get_perpendicular(&star_ray) * RADIUS * 2f32;
            let norm_star_ray = nalgebra::Unit::new_normalize(star_ray);
            const N_TAPS: usize = 4;
            for i in 0..N_TAPS {
                let ang = i as f32 * TAU / N_TAPS as f32;
                let rot = nalgebra::UnitQuaternion::from_axis_angle(&norm_star_ray, ang);
                let sample = (star_ray + (rot * perp)).normalize();
                let (sample_ra, sample_dec) = Self::vec_to_ra_d(&sample);
                bins[Self::bin_for_ra_d(sample_ra, sample_dec)].insert(star_off as u32);
            }
            bins[Self::bin_for_ra_d(star.ra, star.dec)].insert(star_off as u32);
        }

        // Now that we have sorted all stars into all bins they might affect,
        // build the index and bin position buffers.
        let mut bin_positions = Vec::new();
        let mut indices = Vec::new();
        for bin_indices in &bins {
            let bin_base = indices.len();
            let bin_len = bin_indices.len();

            let pos = BinPosition {
                index_base: bin_base as u32,
                num_indexes: bin_len as u32,
            };
            bin_positions.push(pos);
            for index in bin_indices {
                indices.push(*index as u32);
            }
        }

        let band_buffer_size = (mem::size_of::<BandMetadata>() * DEC_BANDS.len()) as u64;
        println!(
            "uploading declination bands buffer with {} bytes",
            band_buffer_size
        );
        let band_buffer = device
            .create_buffer_mapped::<BandMetadata>(DEC_BANDS.len(), wgpu::BufferUsage::STORAGE_READ)
            .fill_from_slice(&DEC_BANDS);

        let bin_positions_buffer_size =
            (mem::size_of::<BinPosition>() * bin_positions.len()) as u64;
        println!(
            "uploading bin position buffer with {} bytes",
            bin_positions_buffer_size
        );
        let bin_positions_buffer = device
            .create_buffer_mapped::<BinPosition>(
                bin_positions.len(),
                wgpu::BufferUsage::STORAGE_READ,
            )
            .fill_from_slice(&bin_positions);

        let star_indices_buffer_size = (mem::size_of::<u32>() * indices.len()) as u64;
        println!(
            "uploading star index buffer with {} bytes",
            star_indices_buffer_size
        );
        let star_indices_buffer = device
            .create_buffer_mapped::<u32>(indices.len(), wgpu::BufferUsage::STORAGE_READ)
            .fill_from_slice(&indices);

        let star_buffer_size = (mem::size_of::<StarInst>() * star_buf.len()) as u64;
        println!("uploading star buffer with {} bytes", star_buffer_size);
        let star_buffer = device
            .create_buffer_mapped::<StarInst>(star_buf.len(), wgpu::BufferUsage::STORAGE_READ)
            .fill_from_slice(&star_buf);

        Ok(Self {
            band_buffer,
            band_buffer_size,
            bin_positions_buffer,
            bin_positions_buffer_size,
            star_indices_buffer,
            star_indices_buffer_size,
            star_buffer,
            star_buffer_size,
        })
    }
}

/*
const TAU: f32 = PI * 2f32;
const PI_2: f32 = PI / 2f32;
const RADIUS: f32 = 0.0015f32;

/*
mod fs {
    vulkano_shaders::shader! {
    ty: "fragment",
    include: ["./libs/render-vulkano"],
    src: "
        #version 450
        #include <buffer/stars/src/include_stars.glsl>
        #include <buffer/stars/src/descriptorset_stars.glsl>
        void main() {}
        "
    }
}
*/

pub struct StarsBuffers {
    descriptorset: Arc<dyn DescriptorSet + Send + Sync>,
}

macro_rules! mkband {
    ($index:expr, $bins_per_row:expr, $base_index:expr) => {
        fs::ty::BandMetadata {
            index: $index,
            bins_per_row: $bins_per_row,
            base_index: $base_index,
        }
    };
}

const DEC_BINS: usize = 64;
const DEC_BANDS: [fs::ty::BandMetadata; 64] = [
    mkband!(0, 1, 0),
    mkband!(1, 8, 1),
    mkband!(2, 12, 9),
    mkband!(3, 16, 21),
    mkband!(4, 32, 37),
    mkband!(5, 32, 69),
    mkband!(6, 32, 101),
    mkband!(7, 32, 133),
    mkband!(8, 64, 165),
    mkband!(9, 64, 229),
    mkband!(10, 64, 293),
    mkband!(11, 64, 357),
    mkband!(12, 84, 421),
    mkband!(13, 84, 505),
    mkband!(14, 84, 589),
    mkband!(15, 84, 673),
    mkband!(16, 84, 757),
    mkband!(17, 84, 841),
    mkband!(18, 128, 925),
    mkband!(19, 128, 1053),
    mkband!(20, 128, 1181),
    mkband!(21, 128, 1309),
    mkband!(22, 128, 1437),
    mkband!(23, 128, 1565),
    mkband!(24, 128, 1693),
    mkband!(25, 128, 1821),
    mkband!(26, 128, 1949),
    mkband!(27, 128, 2077),
    mkband!(28, 128, 2205),
    mkband!(29, 128, 2333),
    mkband!(30, 128, 2461),
    mkband!(31, 128, 2589),
    mkband!(32, 128, 2717),
    mkband!(33, 128, 2845),
    mkband!(34, 128, 2973),
    mkband!(35, 128, 3101),
    mkband!(36, 128, 3229),
    mkband!(37, 128, 3357),
    mkband!(38, 128, 3485),
    mkband!(39, 128, 3613),
    mkband!(40, 128, 3741),
    mkband!(41, 128, 3869),
    mkband!(42, 128, 3997),
    mkband!(43, 128, 4125),
    mkband!(44, 128, 4253),
    mkband!(45, 128, 4381),
    mkband!(46, 84, 4509),
    mkband!(47, 84, 4593),
    mkband!(48, 84, 4677),
    mkband!(49, 84, 4761),
    mkband!(50, 84, 4845),
    mkband!(51, 84, 4929),
    mkband!(52, 64, 5013),
    mkband!(53, 64, 5077),
    mkband!(54, 64, 5141),
    mkband!(55, 64, 5205),
    mkband!(56, 32, 5269),
    mkband!(57, 32, 5301),
    mkband!(58, 32, 5333),
    mkband!(59, 32, 5365),
    mkband!(60, 16, 5397),
    mkband!(61, 12, 5413),
    mkband!(62, 8, 5425),
    mkband!(63, 1, 5433),
];

impl StarsBuffers {
    pub fn new(
        //pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        trace!("StarsBuffers::new");
        Ok(Self {
            descriptorset: Self::upload_stars(pipeline.clone(), window)?,
        })
    }

    fn get_perpendicular(v: &Vector3<f32>) -> Vector3<f32> {
        let not_v = if v[2] > v[1] {
            Vector3::new(0f32, 1f32, 0f32)
        } else {
            Vector3::new(0f32, 0f32, 1f32)
        };
        v.cross(&not_v)
    }

    fn ra_d_to_vec(ra: f32, dec: f32) -> Vector3<f32> {
        Vector3::new(dec.cos() * ra.sin(), -dec.sin(), dec.cos() * ra.cos())
    }

    fn vec_to_ra_d(v: &Vector3<f32>) -> (f32, f32) {
        let ra = v.x.atan2(v.z) + PI;
        let w = (v.x * v.x + v.z * v.z).sqrt();
        let dec = v.y.atan2(w);
        (ra, dec)
    }

    fn num_bins() -> usize {
        let last_band = &DEC_BANDS[DEC_BANDS.len() - 1];
        (last_band.base_index + last_band.bins_per_row) as usize
    }

    fn band_for_dec(dec: f32) -> &'static fs::ty::BandMetadata {
        assert!(dec < PI_2);
        assert!(dec >= -PI_2);
        // See implementation in shader for comments.
        let decz = ((dec + PI_2) * 2f32) / TAU;
        let deci = (decz * DEC_BINS as f32) as usize;
        &DEC_BANDS[deci]
    }

    fn bin_for_ra_d(ra: f32, dec: f32) -> usize {
        let band = Self::band_for_dec(dec);
        let raz = ra / TAU;
        let rai = (band.bins_per_row as f32 * raz) as usize;
        band.base_index as usize + rai
    }

    pub fn upload_stars(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Arc<dyn DescriptorSet + Send + Sync>> {
        let mut offset = 0;
        for (i, band) in DEC_BANDS.iter().enumerate() {
            assert_eq!(band.index as usize, i);
            assert_eq!(band.base_index, offset);
            offset += band.bins_per_row;
        }

        let mut star_buf = Vec::new();
        let stars = Stars::new()?;
        for i in 0..stars.catalog_size() {
            let entry = stars.entry(i)?;
            let ra = entry.right_ascension() as f32;
            let dec = entry.declination() as f32;
            let color = entry.color();
            let radius = RADIUS * entry.radius_scale();
            let star = fs::ty::StarInst {
                ra,
                dec,
                color,
                radius,
            };
            star_buf.push(star);
        }

        // Bin all stars.
        let mut bins = Vec::with_capacity(Self::num_bins());
        bins.resize_with(Self::num_bins(), HashSet::new);
        for (star_off, star) in star_buf.iter().enumerate() {
            // Sample bins in a circle at the star's approximate radius.
            let star_ray = Self::ra_d_to_vec(star.ra, star.dec);
            let perp = Self::get_perpendicular(&star_ray) * RADIUS * 2f32;
            let norm_star_ray = nalgebra::Unit::new_normalize(star_ray);
            const N_TAPS: usize = 4;
            for i in 0..N_TAPS {
                let ang = i as f32 * TAU / N_TAPS as f32;
                let rot = nalgebra::UnitQuaternion::from_axis_angle(&norm_star_ray, ang);
                let sample = (star_ray + (rot * perp)).normalize();
                let (sample_ra, sample_dec) = Self::vec_to_ra_d(&sample);
                bins[Self::bin_for_ra_d(sample_ra, sample_dec)].insert(star_off as u32);
            }
            bins[Self::bin_for_ra_d(star.ra, star.dec)].insert(star_off as u32);
        }

        // Now that we have sorted all stars into all bins they might affect,
        // build the index and bin position buffers.
        let mut bin_positions = Vec::new();
        let mut indices = Vec::new();
        for bin_indices in &bins {
            let bin_base = indices.len();
            let bin_len = bin_indices.len();

            let pos = fs::ty::BinPosition {
                index_base: bin_base as u32,
                num_indexes: bin_len as u32,
            };
            bin_positions.push(pos);
            for index in bin_indices {
                indices.push(*index as u32);
            }
        }

        // FIXME: relocate these to the GPU proper

        println!(
            "uploading declination bands buffer with {} bytes",
            std::mem::size_of::<fs::ty::BandMetadata>() * DEC_BANDS.len()
        );
        let band_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            DEC_BANDS.iter().cloned(),
        )?;

        println!(
            "uploading bin position buffer with {} bytes",
            std::mem::size_of::<fs::ty::BinPosition>() * bin_positions.len()
        );
        let bin_pos_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            bin_positions.into_iter(),
        )?;

        println!(
            "uploading star index buffer with {} bytes",
            std::mem::size_of::<u32>() * indices.len()
        );
        let star_index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        println!(
            "uploading star buffer with {} bytes",
            std::mem::size_of::<fs::ty::StarInst>() * star_buf.len()
        );
        let star_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            star_buf.into_iter(),
        )?;

        let pds: Arc<dyn DescriptorSet + Send + Sync> = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), GlobalSets::Stars.into())
                .add_buffer(band_buffer.clone())?
                .add_buffer(bin_pos_buffer.clone())?
                .add_buffer(star_index_buffer.clone())?
                .add_buffer(star_buffer.clone())?
                .build()?,
        );

        Ok(pds)
    }

    pub fn descriptor_set(&self) -> Arc<dyn DescriptorSet + Send + Sync> {
        self.descriptorset.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::StarsBuffers as SB;
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn ra_d_to_bin_transform() -> Fallible<()> {
        for i in 0..32 {
            assert_eq!(SB::bin_for_ra_d(i as f32 / 32f32, -PI_2), 0);
            assert_eq!(SB::bin_for_ra_d(i as f32 / 32f32, PI_2 - 0.0001), 5433);
        }
        for i in 0..DEC_BINS {
            let f = i as f32 / DEC_BINS as f32;
            let dec = -PI_2 + f * PI + 0.001;
            let band = SB::band_for_dec(dec);
            assert_eq!(band.index, i as u32);

            for j in 0..band.bins_per_row {
                let g = j as f32 / band.bins_per_row as f32;
                let ra = g * TAU + 0.001;
                let bin_idx = SB::bin_for_ra_d(ra, dec);
                assert_eq!(bin_idx as u32, band.base_index + j);
            }
        }
        Ok(())
    }

    #[test]
    fn vec_to_rad_to_vec() -> Fallible<()> {
        let stars = Stars::new()?;
        for i in 0..stars.catalog_size() {
            let entry = stars.entry(i)?;
            let band = SB::band_for_dec(entry.declination());
            if entry.declination() < 0f32 {
                assert!(band.index < 32);
            } else {
                assert!(band.index >= 32);
            }
            let v = SB::ra_d_to_vec(entry.right_ascension(), entry.declination());
            let (ra, dec) = SB::vec_to_ra_d(&v);
            assert_relative_eq!(ra, entry.right_ascension(), epsilon = 0.00001);
            assert_relative_eq!(dec, entry.declination(), epsilon = 0.00001);
        }

        Ok(())
    }

    fn get_perpendicular(v: &Vector3<f32>) -> Vector3<f32> {
        let not_v = if v[2] > v[1] {
            Vector3::new(0f32, 1f32, 0f32)
        } else {
            Vector3::new(0f32, 0f32, 1f32)
        };
        v.cross(&not_v)
    }

    #[test]
    fn fast_taps() -> Fallible<()> {
        let v = Vector3::new(0f32, 0f32, 1f32);
        let perp = get_perpendicular(&v) * 0.001;
        let vn = nalgebra::Unit::new_normalize(v);

        for i in 0..8 {
            let ang = i as f32 * TAU / 8f32;
            let rot = nalgebra::UnitQuaternion::from_axis_angle(&vn, ang);
            let sample = v + rot * perp;
            println!("PERP: {}", sample);
        }

        Ok(())
    }
}
*/
