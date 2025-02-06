use core::default::Default;
use core::marker::PhantomData;

use crate::*;

use alloy_sol_types::{SolType, SolValue};

extern crate alloc;
use alloc::vec::Vec;

mod mapping;
pub use mapping::Mapping;

mod slot;
pub use slot::Slot;

///  STORAGE TYPES:
///  > Must implement the following traits:
///     - `StorageLayout`: Allows the `storage` macro to allocate a storage slot.
///  > Must implement one of the following traits, for external consumption:
///     - `DirectStorage`:  Exposes read and write capabilities of values that are directly accessed.
///     - `KeyValueStorage`:  Exposes read and write capabilities of values that are accesed by key.
///  > Unless it is a wrapper type (like `Mapping`) it must implement the following traits:
///     - `StorageStorable`: Allows db storage reads and writes with abi de/encoding.


// TODO: enhance `storage` macro to handle complex types (like tuples or custom structs)
/// A trait for storage types that require a dedicated slot in the storage layout
pub trait StorageLayout {
    fn allocate(limb0: u64, limb1: u64, limb2: u64, limb3: u64) -> Self;
}

/// Internal trait, for low-level storage operations.
pub trait StorageStorable {
    type Value: SolValue
        + core::convert::From<<<Self::Value as SolValue>::SolType as SolType>::RustType>;

    fn __read(key: U256) -> Self::Value;
    fn __write(key: U256, value: Self::Value);
}

/// Public interface for direct storage types (like `Slot`)
pub trait DirectStorage<V>
where
    Self: StorageStorable<Value = V>,
{
    fn read(&self) -> V;
    fn write(&mut self, value: V);
}

/// Public interface for key-value storage types (like `Mapping`)
pub trait KeyValueStorage<K> {
    type ReadValue;
    type WriteValue;

    fn read(&self, key: K) -> Self::ReadValue;
    fn write(&mut self, key: K, value: Self::WriteValue);
}
