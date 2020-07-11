// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
mod frame_state_tracker;

pub use crate::frame_state_tracker::FrameStateTracker;

#[macro_export]
macro_rules! make_frame_graph {
    (
        $name:ident {
            buffers: { $($buffer_name:ident: $buffer_type:ty),* };
            precompute: { $($precompute_name:ident),* };
            renderers: [
                $( $renderer_name:ident: $renderer_type:ty { $($input_buffer_name:ident),* } ),*
            ];
        }
    ) => {
        pub struct $name {
            tracker: $crate::FrameStateTracker,
            $(
                $buffer_name: ::std::sync::Arc<::std::cell::RefCell<$buffer_type>>
            ),*,
            $(
                $renderer_name: $renderer_type
            ),*
        }

        impl $name {
            #[allow(clippy::too_many_arguments)]
            pub fn new(
                gpu: &mut ::gpu::GPU,
                $(
                    $buffer_name: &::std::sync::Arc<::std::cell::RefCell<$buffer_type>>
                ),*
            ) -> ::failure::Fallible<Self> {
                let mut graph = Self {
                    tracker: Default::default(),
                    $(
                        $buffer_name: $buffer_name.to_owned()
                    ),*,
                    $(
                        $renderer_name: <$renderer_type>::new(
                            gpu,
                            $(
                                &$input_buffer_name.borrow()
                            ),*
                        )?
                    ),*
                };
                Ok(graph)
            }

            pub fn run(&mut self, gpu: &mut ::gpu::GPU) -> ::failure::Fallible<()> {
                $(
                    let $buffer_name = self.$buffer_name.borrow();
                )*
                let mut frame = gpu.begin_frame()?;
                {
                    for desc in self.tracker.drain_uploads() {
                        frame.copy_buffer_to_buffer(
                            &desc.source,
                            desc.source_offset,
                            &desc.destination,
                            desc.destination_offset,
                            desc.copy_size
                        );
                    }

                    {
                        let cpass = frame.begin_compute_pass();
                        $(
                            let cpass = $precompute_name.precompute(cpass);
                        )*
                    }

                    {
                        let rpass = frame.begin_render_pass();
                        $(
                            let rpass = self.$renderer_name.draw(
                                rpass,
                                $(
                                    &$input_buffer_name
                                ),*
                            );
                        )*
                    }
                }
                frame.finish();

                Ok(())
            }

            pub fn tracker_mut(&mut self) -> &mut $crate::FrameStateTracker {
                &mut self.tracker
            }
        }
    };
}
