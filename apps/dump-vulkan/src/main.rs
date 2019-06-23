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
extern crate failure;
extern crate vulkano;

use failure::Fallible;
use vulkano::instance::{Instance, InstanceExtensions, PhysicalDevice};

macro_rules! show_feature {
    ($feats:ident, $feat:ident) => {
        if $feats.$feat {
            println!("\t\t\x1b[32m{}\x1b[0m", stringify!($feat));
        } else {
            println!("\t\t\x1b[31m{}\x1b[0m", stringify!($feat));
        }
    };
}

#[allow(clippy::cognitive_complexity)] // This metric is just wrong in this case.
fn main() -> Fallible<()> {
    let instance = Instance::new(None, &InstanceExtensions::none(), None)?;
    for physical in PhysicalDevice::enumerate(&instance) {
        println!("{}", physical.name());
        println!("\tType: {:?}", physical.ty());
        println!("\tAPI Version: {}", physical.api_version());
        println!("\tDriver Version: {}", physical.driver_version());
        println!(
            "\tAddress: {:4X}:{:4X}",
            physical.pci_device_id(),
            physical.pci_vendor_id()
        );
        let mut uuid = String::new();
        for c in physical.uuid() {
            uuid += &format!("{:02X}", c);
        }
        println!("\tUUID: {}", uuid);

        println!("\tQueue Families:");
        for family in physical.queue_families() {
            let mut features = Vec::new();
            if family.supports_graphics() {
                features.push("graphics");
            }
            if family.supports_compute() {
                features.push("compute");
            }
            if family.supports_transfers() {
                features.push("transfers");
            }
            if family.supports_sparse_binding() {
                features.push("sparse_binding");
            }
            println!(
                "\t\t{}) count:{}, features:{}",
                family.id(),
                family.queues_count(),
                features.join("/")
            );
        }

        println!("\tMemory Heap:");
        for heap in physical.memory_heaps() {
            println!(
                "\t\t{}) {}GiB on {}",
                heap.id(),
                heap.size() / (1024 * 1024 * 1024usize),
                if heap.is_device_local() {
                    "device"
                } else {
                    "host"
                }
            );
        }

        println!("\tMemory Types:");
        for memtype in physical.memory_types() {
            let mut features = Vec::new();
            if memtype.is_host_visible() {
                features.push("visible");
            }
            if memtype.is_host_coherent() {
                features.push("coherent");
            }
            if memtype.is_host_cached() {
                features.push("cached");
            }
            if memtype.is_lazily_allocated() {
                features.push("lazy");
            }
            println!(
                "\t\t{}) on {} {}",
                memtype.id(),
                if memtype.is_device_local() {
                    "device"
                } else {
                    "host"
                },
                features.join("+"),
            );
        }

        println!("\tFeatures:");
        let features = physical.supported_features();
        show_feature!(features, robust_buffer_access);
        show_feature!(features, full_draw_index_uint32);
        show_feature!(features, image_cube_array);
        show_feature!(features, independent_blend);
        show_feature!(features, geometry_shader);
        show_feature!(features, tessellation_shader);
        show_feature!(features, sample_rate_shading);
        show_feature!(features, dual_src_blend);
        show_feature!(features, logic_op);
        show_feature!(features, multi_draw_indirect);
        show_feature!(features, draw_indirect_first_instance);
        show_feature!(features, depth_clamp);
        show_feature!(features, depth_bias_clamp);
        show_feature!(features, fill_mode_non_solid);
        show_feature!(features, depth_bounds);
        show_feature!(features, wide_lines);
        show_feature!(features, large_points);
        show_feature!(features, alpha_to_one);
        show_feature!(features, multi_viewport);
        show_feature!(features, sampler_anisotropy);
        show_feature!(features, texture_compression_etc2);
        show_feature!(features, texture_compression_astc_ldr);
        show_feature!(features, texture_compression_bc);
        show_feature!(features, occlusion_query_precise);
        show_feature!(features, pipeline_statistics_query);
        show_feature!(features, vertex_pipeline_stores_and_atomics);
        show_feature!(features, fragment_stores_and_atomics);
        show_feature!(features, shader_tessellation_and_geometry_point_size);
        show_feature!(features, shader_image_gather_extended);
        show_feature!(features, shader_storage_image_extended_formats);
        show_feature!(features, shader_storage_image_multisample);
        show_feature!(features, shader_storage_image_read_without_format);
        show_feature!(features, shader_storage_image_write_without_format);
        show_feature!(features, shader_uniform_buffer_array_dynamic_indexing);
        show_feature!(features, shader_sampled_image_array_dynamic_indexing);
        show_feature!(features, shader_storage_buffer_array_dynamic_indexing);
        show_feature!(features, shader_storage_image_array_dynamic_indexing);
        show_feature!(features, shader_clip_distance);
        show_feature!(features, shader_cull_distance);
        show_feature!(features, shader_f3264);
        show_feature!(features, shader_int64);
        show_feature!(features, shader_int16);
        show_feature!(features, shader_resource_residency);
        show_feature!(features, shader_resource_min_lod);
        show_feature!(features, sparse_binding);
        show_feature!(features, sparse_residency_buffer);
        show_feature!(features, sparse_residency_image2d);
        show_feature!(features, sparse_residency_image3d);
        show_feature!(features, sparse_residency2_samples);
        show_feature!(features, sparse_residency4_samples);
        show_feature!(features, sparse_residency8_samples);
        show_feature!(features, sparse_residency16_samples);
        show_feature!(features, sparse_residency_aliased);
        show_feature!(features, variable_multisample_rate);
        show_feature!(features, inherited_queries);

        println!("\tLimits:");
        let l = physical.limits();
        println!("\t\tmax_image_dimension_1d: {}", l.max_image_dimension_1d());
        println!("\t\tmax_image_dimension_2d: {}", l.max_image_dimension_2d());
        println!("\t\tmax_image_dimension_3d: {}", l.max_image_dimension_3d());
        println!(
            "\t\tmax_image_dimension_cube: {}",
            l.max_image_dimension_cube()
        );
        println!("\t\tmax_image_array_layers: {}", l.max_image_array_layers());
        println!(
            "\t\tmax_texel_buffer_elements: {}",
            l.max_texel_buffer_elements()
        );
        println!(
            "\t\tmax_uniform_buffer_range: {}",
            l.max_uniform_buffer_range()
        );
        println!(
            "\t\tmax_storage_buffer_range: {}",
            l.max_storage_buffer_range()
        );
        println!(
            "\t\tmax_push_constants_size: {}",
            l.max_push_constants_size()
        );
        println!(
            "\t\tmax_memory_allocation_count: {}",
            l.max_memory_allocation_count()
        );
        println!(
            "\t\tmax_sampler_allocation_count: {}",
            l.max_sampler_allocation_count()
        );
        println!(
            "\t\tbuffer_image_granularity: {}",
            l.buffer_image_granularity()
        );
        println!(
            "\t\tsparse_address_space_size: {}",
            l.sparse_address_space_size()
        );
        println!(
            "\t\tmax_bound_descriptor_sets: {}",
            l.max_bound_descriptor_sets()
        );
        println!(
            "\t\tmax_per_stage_descriptor_samplers: {}",
            l.max_per_stage_descriptor_samplers()
        );
        println!(
            "\t\tmax_per_stage_descriptor_uniform_buffers: {}",
            l.max_per_stage_descriptor_uniform_buffers()
        );
        println!(
            "\t\tmax_per_stage_descriptor_storage_buffers: {}",
            l.max_per_stage_descriptor_storage_buffers()
        );
        println!(
            "\t\tmax_per_stage_descriptor_sampled_images: {}",
            l.max_per_stage_descriptor_sampled_images()
        );
        println!(
            "\t\tmax_per_stage_descriptor_storage_images: {}",
            l.max_per_stage_descriptor_storage_images()
        );
        println!(
            "\t\tmax_per_stage_descriptor_input_attachments: {}",
            l.max_per_stage_descriptor_input_attachments()
        );
        println!(
            "\t\tmax_per_stage_resources: {}",
            l.max_per_stage_resources()
        );
        println!(
            "\t\tmax_descriptor_set_samplers: {}",
            l.max_descriptor_set_samplers()
        );
        println!(
            "\t\tmax_descriptor_set_uniform_buffers: {}",
            l.max_descriptor_set_uniform_buffers()
        );
        println!(
            "\t\tmax_descriptor_set_uniform_buffers_dynamic: {}",
            l.max_descriptor_set_uniform_buffers_dynamic()
        );
        println!(
            "\t\tmax_descriptor_set_storage_buffers: {}",
            l.max_descriptor_set_storage_buffers()
        );
        println!(
            "\t\tmax_descriptor_set_storage_buffers_dynamic: {}",
            l.max_descriptor_set_storage_buffers_dynamic()
        );
        println!(
            "\t\tmax_descriptor_set_sampled_images: {}",
            l.max_descriptor_set_sampled_images()
        );
        println!(
            "\t\tmax_descriptor_set_storage_images: {}",
            l.max_descriptor_set_storage_images()
        );
        println!(
            "\t\tmax_descriptor_set_input_attachments: {}",
            l.max_descriptor_set_input_attachments()
        );
        println!(
            "\t\tmax_vertex_input_attributes: {}",
            l.max_vertex_input_attributes()
        );
        println!(
            "\t\tmax_vertex_input_bindings: {}",
            l.max_vertex_input_bindings()
        );
        println!(
            "\t\tmax_vertex_input_attribute_offset: {}",
            l.max_vertex_input_attribute_offset()
        );
        println!(
            "\t\tmax_vertex_input_binding_stride: {}",
            l.max_vertex_input_binding_stride()
        );
        println!(
            "\t\tmax_vertex_output_components: {}",
            l.max_vertex_output_components()
        );
        println!(
            "\t\tmax_tessellation_generation_level: {}",
            l.max_tessellation_generation_level()
        );
        println!(
            "\t\tmax_tessellation_patch_size: {}",
            l.max_tessellation_patch_size()
        );
        println!(
            "\t\tmax_tessellation_control_per_vertex_input_components: {}",
            l.max_tessellation_control_per_vertex_input_components()
        );
        println!(
            "\t\tmax_tessellation_control_per_vertex_output_components: {}",
            l.max_tessellation_control_per_vertex_output_components()
        );
        println!(
            "\t\tmax_tessellation_control_per_patch_output_components: {}",
            l.max_tessellation_control_per_patch_output_components()
        );
        println!(
            "\t\tmax_tessellation_control_total_output_components: {}",
            l.max_tessellation_control_total_output_components()
        );
        println!(
            "\t\tmax_tessellation_evaluation_input_components: {}",
            l.max_tessellation_evaluation_input_components()
        );
        println!(
            "\t\tmax_tessellation_evaluation_output_components: {}",
            l.max_tessellation_evaluation_output_components()
        );
        println!(
            "\t\tmax_geometry_shader_invocations: {}",
            l.max_geometry_shader_invocations()
        );
        println!(
            "\t\tmax_geometry_input_components: {}",
            l.max_geometry_input_components()
        );
        println!(
            "\t\tmax_geometry_output_components: {}",
            l.max_geometry_output_components()
        );
        println!(
            "\t\tmax_geometry_output_vertices: {}",
            l.max_geometry_output_vertices()
        );
        println!(
            "\t\tmax_geometry_total_output_components: {}",
            l.max_geometry_total_output_components()
        );
        println!(
            "\t\tmax_fragment_input_components: {}",
            l.max_fragment_input_components()
        );
        println!(
            "\t\tmax_fragment_output_attachments: {}",
            l.max_fragment_output_attachments()
        );
        println!(
            "\t\tmax_fragment_dual_src_attachments: {}",
            l.max_fragment_dual_src_attachments()
        );
        println!(
            "\t\tmax_fragment_combined_output_resources: {}",
            l.max_fragment_combined_output_resources()
        );
        println!(
            "\t\tmax_compute_shared_memory_size: {}",
            l.max_compute_shared_memory_size()
        );
        println!(
            "\t\tmax_compute_work_group_count: {:?}",
            l.max_compute_work_group_count()
        );
        println!(
            "\t\tmax_compute_work_group_invocations: {}",
            l.max_compute_work_group_invocations()
        );
        println!(
            "\t\tmax_compute_work_group_size: {:?}",
            l.max_compute_work_group_size()
        );
        println!(
            "\t\tsub_pixel_precision_bits: {}",
            l.sub_pixel_precision_bits()
        );
        println!(
            "\t\tsub_texel_precision_bits: {}",
            l.sub_texel_precision_bits()
        );
        println!("\t\tmipmap_precision_bits: {}", l.mipmap_precision_bits());
        println!(
            "\t\tmax_draw_indexed_index_value: {}",
            l.max_draw_indexed_index_value()
        );
        println!(
            "\t\tmax_draw_indirect_count: {}",
            l.max_draw_indirect_count()
        );
        println!("\t\tmax_sampler_lod_bias: {}", l.max_sampler_lod_bias());
        println!("\t\tmax_sampler_anisotropy: {}", l.max_sampler_anisotropy());
        println!("\t\tmax_viewports: {}", l.max_viewports());
        println!(
            "\t\tmax_viewport_dimensions: {:?}",
            l.max_viewport_dimensions()
        );
        println!("\t\tviewport_bounds_range: {:?}", l.viewport_bounds_range());
        println!(
            "\t\tviewport_sub_pixel_bits: {}",
            l.viewport_sub_pixel_bits()
        );
        println!(
            "\t\tmin_memory_map_alignment: {}",
            l.min_memory_map_alignment()
        );
        println!(
            "\t\tmin_texel_buffer_offset_alignment: {}",
            l.min_texel_buffer_offset_alignment()
        );
        println!(
            "\t\tmin_uniform_buffer_offset_alignment: {}",
            l.min_uniform_buffer_offset_alignment()
        );
        println!(
            "\t\tmin_storage_buffer_offset_alignment: {}",
            l.min_storage_buffer_offset_alignment()
        );
        println!("\t\tmin_texel_offset: {}", l.min_texel_offset());
        println!("\t\tmax_texel_offset: {}", l.max_texel_offset());
        println!(
            "\t\tmin_texel_gather_offset: {}",
            l.min_texel_gather_offset()
        );
        println!(
            "\t\tmax_texel_gather_offset: {}",
            l.max_texel_gather_offset()
        );
        println!(
            "\t\tmin_interpolation_offset: {}",
            l.min_interpolation_offset()
        );
        println!(
            "\t\tmax_interpolation_offset: {}",
            l.max_interpolation_offset()
        );
        println!(
            "\t\tsub_pixel_interpolation_offset_bits: {}",
            l.sub_pixel_interpolation_offset_bits()
        );
        println!("\t\tmax_framebuffer_width: {}", l.max_framebuffer_width());
        println!("\t\tmax_framebuffer_height: {}", l.max_framebuffer_height());
        println!("\t\tmax_framebuffer_layers: {}", l.max_framebuffer_layers());
        println!(
            "\t\tframebuffer_color_sample_counts: {}",
            l.framebuffer_color_sample_counts()
        );
        println!(
            "\t\tframebuffer_depth_sample_counts: {}",
            l.framebuffer_depth_sample_counts()
        );
        println!(
            "\t\tframebuffer_stencil_sample_counts: {}",
            l.framebuffer_stencil_sample_counts()
        );
        println!(
            "\t\tframebuffer_no_attachments_sample_counts: {}",
            l.framebuffer_no_attachments_sample_counts()
        );
        println!("\t\tmax_color_attachments: {}", l.max_color_attachments());
        println!(
            "\t\tsampled_image_color_sample_counts: {}",
            l.sampled_image_color_sample_counts()
        );
        println!(
            "\t\tsampled_image_integer_sample_counts: {}",
            l.sampled_image_integer_sample_counts()
        );
        println!(
            "\t\tsampled_image_depth_sample_counts: {}",
            l.sampled_image_depth_sample_counts()
        );
        println!(
            "\t\tsampled_image_stencil_sample_counts: {}",
            l.sampled_image_stencil_sample_counts()
        );
        println!(
            "\t\tstorage_image_sample_counts: {}",
            l.storage_image_sample_counts()
        );
        println!("\t\tmax_sample_mask_words: {}", l.max_sample_mask_words());
        println!(
            "\t\ttimestamp_compute_and_graphics: {}",
            l.timestamp_compute_and_graphics()
        );
        println!("\t\ttimestamp_period: {}", l.timestamp_period());
        println!("\t\tmax_clip_distances: {}", l.max_clip_distances());
        println!("\t\tmax_cull_distances: {}", l.max_cull_distances());
        println!(
            "\t\tmax_combined_clip_and_cull_distances: {}",
            l.max_combined_clip_and_cull_distances()
        );
        println!(
            "\t\tdiscrete_queue_priorities: {}",
            l.discrete_queue_priorities()
        );
        println!("\t\tpoint_size_range: {:?}", l.point_size_range());
        println!("\t\tline_width_range: {:?}", l.line_width_range());
        println!("\t\tpoint_size_granularity: {}", l.point_size_granularity());
        println!("\t\tline_width_granularity: {}", l.line_width_granularity());
        println!("\t\tstrict_lines: {}", l.strict_lines());
        println!(
            "\t\tstandard_sample_locations: {}",
            l.standard_sample_locations()
        );
        println!(
            "\t\toptimal_buffer_copy_offset_alignment: {}",
            l.optimal_buffer_copy_offset_alignment()
        );
        println!(
            "\t\toptimal_buffer_copy_row_pitch_alignment: {}",
            l.optimal_buffer_copy_row_pitch_alignment()
        );
        println!("\t\tnon_coherent_atom_size: {}", l.non_coherent_atom_size());
    }
    Ok(())
}
