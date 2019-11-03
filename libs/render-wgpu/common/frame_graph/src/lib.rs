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

pub struct CopyBufferDescriptor {
    pub source: ::wgpu::Buffer,
    pub source_offset: ::wgpu::BufferAddress,
    pub destination: ::std::sync::Arc<::std::boxed::Box<::wgpu::Buffer>>,
    pub destination_offset: ::wgpu::BufferAddress,
    pub copy_size: ::wgpu::BufferAddress,
}

impl CopyBufferDescriptor {
    pub fn new(
        source: ::wgpu::Buffer,
        destination: ::std::sync::Arc<::std::boxed::Box<::wgpu::Buffer>>,
        copy_size: ::wgpu::BufferAddress,
    ) -> Self {
        Self {
            source,
            source_offset: 0,
            destination,
            destination_offset: 0,
            copy_size,
        }
    }
}

#[macro_export]
macro_rules! make_frame_graph {
    (
        $name:ident {
            buffers: { $($buffer_name:ident: $buffer_type:ty),* };
            passes: [
                $( $pass_name:ident: $pass_type:ty { $($input_buffer_name:ident),* } ),*
            ];
        }
    ) => {
        pub struct $name {
            $(
                $buffer_name: ::std::sync::Arc<::std::cell::RefCell<$buffer_type>>
            ),*,
            $(
                $pass_name: $pass_type
            ),*
        }

        impl $name {
            pub fn new(
                gpu: &mut ::gpu::GPU,
                $(
                    $buffer_name: &::std::sync::Arc<::std::cell::RefCell<$buffer_type>>
                ),*
            ) -> ::failure::Fallible<Self> {
                Ok(Self {
                    $(
                        $buffer_name: $buffer_name.to_owned()
                    ),*,
                    $(
                        $pass_name: <$pass_type>::new(
                            gpu,
                            $(
                                &$input_buffer_name.borrow()
                            ),*
                        )?
                    ),*
                })
            }

            pub fn run(&self, gpu: &mut ::gpu::GPU, mut upload_buffers: Vec<$crate::CopyBufferDescriptor>) {
                let mut frame = gpu.begin_frame();
                {
                    for desc in upload_buffers.drain(..) {
                        frame.copy_buffer_to_buffer(
                            &desc.source,
                            desc.source_offset,
                            &desc.destination,
                            desc.destination_offset,
                            desc.copy_size
                        );
                    }

                    let mut rpass = frame.begin_render_pass();
                    $(
                        self.$pass_name.draw(
                            &mut rpass,
                            $(
                                &self.$input_buffer_name.borrow()
                            ),*
                        );
                    )*
                }
                frame.finish();
            }
        }
    };
}
