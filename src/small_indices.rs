use rustc_hash::{FxHashMap, FxHashSet};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

pub trait SmallIdx:
    Sized
    + Copy
    + Default
    + Debug
    + Display
    + Hash
    + Ord
    + Into<usize>
    + Into<u32>
    + From<usize>
    + From<u32>
    + Serialize
    + DeserializeOwned
{
    const INVALID: Self;

    fn idx(&self) -> usize;

    #[allow(dead_code)]
    fn valid(&self) -> bool {
        *self != Self::INVALID
    }

    #[allow(dead_code)]
    fn idx_if_valid(&self) -> Option<usize> {
        if self.valid() {
            Some(self.idx())
        } else {
            None
        }
    }
}

/// Creates an index struct that uses a `u32` to store the index.
#[macro_export]
macro_rules! create_idx_struct {
    ($vis:vis $name:ident) => {
        #[derive(
            Copy,
            Clone,
            Debug,
            Hash,
            Eq,
            PartialEq,
            Ord,
            PartialOrd,
            ::serde::Serialize,
            ::serde::Deserialize,
        )]
        #[serde(transparent)]
        $vis struct $name(u32);

        impl $crate::small_indices::SmallIdx for $name {
            #[allow(dead_code)]
            const INVALID: Self = Self(u32::max_value());

            fn idx(&self) -> usize {
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

        impl ::std::convert::From<$name> for usize {
            fn from(i: $name) -> Self {
                use $crate::small_indices::SmallIdx;
                i.idx()
            }
        }

        impl ::std::convert::From<$name> for u32 {
            fn from(i: $name) -> Self {
                i.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl ::std::default::Default for $name {
            fn default() -> Self {
                use $crate::small_indices::SmallIdx;
                Self::INVALID
            }
        }
    };
}

/// Hash map with optimized hash function for small indices.
pub type IdxHashMap<I, V> = FxHashMap<I, V>;

/// Hash set with optimized hash function for small indices.
pub type IdxHashSet<I> = FxHashSet<I>;
