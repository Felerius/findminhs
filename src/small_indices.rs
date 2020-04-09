/// Creates an index struct that uses a `u32` to store the index.
#[macro_export]
macro_rules! create_idx_struct {
    ($name:ident) => {
        #[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
        pub struct $name(u32);

        impl $name {
            #[allow(dead_code)]
            pub const INVALID: Self = Self(u32::max_value());

            pub fn idx(&self) -> usize {
                self.0 as usize
            }
        }

        impl ::std::convert::From<usize> for $name {
            fn from(idx: usize) -> Self {
                debug_assert!(<u32 as ::std::convert::TryFrom<usize>>::try_from(idx).is_ok());
                Self(idx as u32)
            }
        }

        impl ::std::convert::From<u32> for $name {
            fn from(idx: u32) -> Self {
                Self(idx)
            }
        }

        impl ::std::convert::Into<usize> for $name {
            fn into(self) -> usize {
                self.idx()
            }
        }

        impl ::std::convert::Into<u32> for $name {
            fn into(self) -> u32 {
                self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl ::std::default::Default for $name {
            fn default() -> Self {
                Self::INVALID
            }
        }
    };
}
