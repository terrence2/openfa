// This file is part of packed_struct.
//
// packed_struct is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// packed_struct is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with packed_struct.  If not, see <http://www.gnu.org/licenses/>.
pub use failure::{ensure, Error};

#[macro_export]
macro_rules! _make_packed_struct_accessor {
    ($field:ident, $field_name:ident, $field_ty:ty, $output_ty:ty) => {
        fn $field_name(&self) -> $output_ty {
            self.$field as $output_ty
        }
    };

    ($field:ident, $field_name:ident, $field_ty:ty, ) => {
        fn $field_name(&self) -> $field_ty {
            self.$field as $field_ty
        }
    }
}

#[macro_export]
macro_rules! packed_struct {
    ($name:ident {
        $( $field:ident => $field_name:ident : $field_ty:ty $(as $field_name_ty:ty),* ),+
    }) => {
        #[repr(C)]
        #[repr(packed)]
        pub struct $name {
            $(
                $field: $field_ty
            ),+
        }

        impl $name {
            $(
                $crate::_make_packed_struct_accessor!($field, $field_name, $field_ty, $($field_name_ty),*);
            )+

            pub fn overlay(buf: &[u8]) -> Result<&$name, $crate::Error> {
                $crate::ensure!(buf.len() >= std::mem::size_of::<$name>(), "buffer to short to overlay $name");
                let ptr: *const $name = buf.as_ptr() as *const _;
                let r: &$name = unsafe { &*ptr };
                Ok(r)
            }

            pub fn as_bytes(&self) -> Result<&[u8], $crate::Error> {
                let ptr: *const u8 = self as *const Self as *const u8;
                let buf: &[u8] = unsafe { std::slice::from_raw_parts(ptr, std::mem::size_of::<Self>()) }; 
                Ok(buf)
            }

            #[allow(clippy::too_many_arguments)]
            pub fn build(
                $(
                    $field_name: $field_ty
                ),+
            ) -> Result<$name, $crate::Error> {
                Ok($name {
                    $(
                        $field: $field_name
                    ),+
                })
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.debug_struct(stringify!($name))
                    $(.field(stringify!($field_name), &self.$field_name()))*
                    .finish()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    packed_struct!(TestStruct {
        _0 => foo: u8 as usize,
        _1 => bar: u32,
        _2 => baz: u16 as u8
    });

    #[test]
    fn it_has_accessors() {
        let buf: &[u8] = &vec![42, 1, 0, 0, 0, 0, 1];
        let ts = TestStruct::overlay(buf).unwrap();
        assert_eq!(ts.foo(), 42usize);
        assert_eq!(ts.bar(), 1u32);
        assert_eq!(ts.baz(), 0u8);
    }

    #[test]
    fn it_can_debug() {
        let buf: &[u8] = &vec![42, 1, 0, 0, 0, 0, 1];
        let ts = TestStruct::overlay(buf).unwrap();
        format!("{:?}", ts);
    }
}
