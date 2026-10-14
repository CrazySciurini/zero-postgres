//! Zero-copy row decoding for fixed-size types.
//!
//! This module provides traits and types for zero-copy decoding of database rows
//! where all fields have fixed wire sizes. This is useful for high-performance
//! scenarios where avoiding allocations is critical.
//!
//! # PostgreSQL Wire Format
//!
//! PostgreSQL's binary protocol includes a 4-byte length prefix before each
//! column value. To enable zero-copy struct casting, use [`LengthPrefixed<T>`]
//! wrapper which accounts for this prefix in its layout.
//!
//! # Requirements
//!
//! - The struct derives `zerocopy::{FromBytes, KnownLayout, Immutable}` alongside `RefFromRow`
//! - Struct must use `#[repr(C, packed)]` for predictable layout
//! - Every field is a `LengthPrefixed<T>` with a big-endian `T: FixedWireSize`
//! - All columns must be `NOT NULL` (no `Option<T>` support)

use zerocopy::{FromBytes, Immutable, KnownLayout};

use crate::protocol::backend::query::{DataRow, FieldDescription};
use crate::{Error, Result};

// Re-export big-endian types for convenience
pub use zerocopy::big_endian::{
    I16 as I16BE, I32 as I32BE, I64 as I64BE, U16 as U16BE, U32 as U32BE, U64 as U64BE,
};

/// Marker trait for types with a fixed wire size in PostgreSQL binary protocol.
///
/// This trait is only implemented for types that have a guaranteed fixed size
/// on the wire. For use with `RefFromRow`, wrap types in [`LengthPrefixed<T>`]
/// to account for PostgreSQL's length prefix.
pub trait FixedWireSize {
    /// The fixed size in bytes on the wire.
    const WIRE_SIZE: usize;
}

// Single-byte types are endian-agnostic
impl FixedWireSize for i8 {
    const WIRE_SIZE: usize = 1;
}
impl FixedWireSize for u8 {
    const WIRE_SIZE: usize = 1;
}

// Big-endian integer types (PostgreSQL wire format / network byte order)
impl FixedWireSize for zerocopy::big_endian::I16 {
    const WIRE_SIZE: usize = 2;
}
impl FixedWireSize for zerocopy::big_endian::U16 {
    const WIRE_SIZE: usize = 2;
}
impl FixedWireSize for zerocopy::big_endian::I32 {
    const WIRE_SIZE: usize = 4;
}
impl FixedWireSize for zerocopy::big_endian::U32 {
    const WIRE_SIZE: usize = 4;
}
impl FixedWireSize for zerocopy::big_endian::I64 {
    const WIRE_SIZE: usize = 8;
}
impl FixedWireSize for zerocopy::big_endian::U64 {
    const WIRE_SIZE: usize = 8;
}

/// A length-prefixed value matching PostgreSQL's binary wire format.
///
/// PostgreSQL's binary protocol prefixes each column value with a 4-byte
/// signed integer length (or -1 for NULL). This wrapper type includes that
/// prefix, allowing zero-copy struct casting from raw row data.
///
/// # Layout
///
/// ```text
/// [length: i32 BE][value: T]
/// ```
///
/// Total size: 4 + size_of::<T>() bytes
///
/// # Example
///
/// ```ignore
/// use zero_postgres::conversion::ref_row::{LengthPrefixed, I64BE};
///
/// // Wire format: [0x00 0x00 0x00 0x08][8 bytes of i64]
/// let data: &[u8] = &[0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 42];
/// let prefixed: &LengthPrefixed<I64BE> = zerocopy::FromBytes::ref_from_bytes(data).unwrap();
/// assert_eq!(prefixed.get(), 42);
/// ```
#[derive(Debug, Clone, Copy, FromBytes, KnownLayout, Immutable)]
#[repr(C, packed)]
pub struct LengthPrefixed<T> {
    /// Length prefix (big-endian i32). Should equal size_of::<T>() for valid data.
    len: zerocopy::big_endian::I32,
    /// The actual value.
    value: T,
}

impl<T: FixedWireSize> FixedWireSize for LengthPrefixed<T> {
    /// Wire size = 4 bytes (length prefix) + inner type's wire size
    const WIRE_SIZE: usize = 4 + T::WIRE_SIZE;
}

impl<T: Copy> LengthPrefixed<T> {
    /// Get the length prefix value.
    #[inline]
    pub fn len(&self) -> i32 {
        self.len.get()
    }

    /// Check if the length is zero.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len.get() == 0
    }

    /// Check if the length indicates NULL (-1).
    #[inline]
    pub fn is_null(&self) -> bool {
        self.len.get() == -1
    }

    /// Get the inner value.
    ///
    /// Note: This copies the value because packed structs cannot safely
    /// return references to potentially unaligned fields.
    #[inline]
    pub fn get(&self) -> T {
        self.value
    }
}

/// Trait for zero-copy decoding of a row into a fixed-size struct.
///
/// Unlike `FromRow`, this trait returns a reference directly into the buffer
/// without any copying or allocation. This requires:
///
/// 1. All fields are `LengthPrefixed<T>` where `T: FixedWireSize`
/// 2. No NULL values (columns must be `NOT NULL`)
/// 3. Struct has `#[repr(C, packed)]` layout
/// 4. Struct derives `zerocopy::{FromBytes, KnownLayout, Immutable}`
///
/// # Example
///
/// ```
/// use zero_postgres::conversion::ref_row::{I32BE, I64BE, LengthPrefixed};
/// use zero_postgres_derive::RefFromRow;
/// use zerocopy::{FromBytes, Immutable, KnownLayout};
///
/// #[derive(FromBytes, KnownLayout, Immutable, RefFromRow)]
/// #[repr(C, packed)]
/// struct UserStats {
///     user_id: LengthPrefixed<I64BE>,
///     login_count: LengthPrefixed<I32BE>,
/// }
/// ```
///
/// # Compile-fail tests
///
/// Missing zerocopy derives:
/// ```compile_fail,E0277
/// use zero_postgres::conversion::ref_row::{I32BE, LengthPrefixed};
/// use zero_postgres_derive::RefFromRow;
///
/// #[derive(RefFromRow)]
/// #[repr(C, packed)]
/// struct Invalid {
///     value: LengthPrefixed<I32BE>,
/// }
/// ```
///
/// Missing `#[repr(C, packed)]`:
/// ```compile_fail
/// use zero_postgres::conversion::ref_row::{I32BE, LengthPrefixed};
/// use zero_postgres_derive::RefFromRow;
/// use zerocopy::{FromBytes, Immutable, KnownLayout};
///
/// #[derive(FromBytes, KnownLayout, Immutable, RefFromRow)]
/// struct Invalid {
///     value: LengthPrefixed<I32BE>,
/// }
/// ```
///
/// Field without a length prefix:
/// ```compile_fail,E0308
/// use zero_postgres::conversion::ref_row::I32BE;
/// use zero_postgres_derive::RefFromRow;
/// use zerocopy::{FromBytes, Immutable, KnownLayout};
///
/// #[derive(FromBytes, KnownLayout, Immutable, RefFromRow)]
/// #[repr(C, packed)]
/// struct Invalid {
///     value: I32BE,
/// }
/// ```
///
/// Native integer types (must use big-endian wrappers):
/// ```compile_fail,E0277
/// use zero_postgres::conversion::ref_row::LengthPrefixed;
/// use zero_postgres_derive::RefFromRow;
/// use zerocopy::{FromBytes, Immutable, KnownLayout};
///
/// #[derive(FromBytes, KnownLayout, Immutable, RefFromRow)]
/// #[repr(C, packed)]
/// struct Invalid {
///     value: LengthPrefixed<i64>,
/// }
/// ```
pub trait RefFromRow<'a>: Sized {
    /// Decode a row as a zero-copy reference from binary format.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The column count doesn't match the field count
    /// - Any column is NULL (indicated by length = -1)
    /// - Any column's length differs from its field's value size
    fn ref_from_row_binary(cols: &[FieldDescription], row: DataRow<'a>) -> Result<&'a Self>;
}

/// Checks every column of `row` against `wire_sizes`, then casts the row to `T`.
#[doc(hidden)]
pub fn ref_from_data_row<'a, T>(row: DataRow<'a>, wire_sizes: &[usize]) -> Result<&'a T>
where
    T: FromBytes + KnownLayout + Immutable,
{
    if row.len() != wire_sizes.len() {
        return Err(Error::Decode(format!(
            "RefFromRow expects {} columns, got {}",
            wire_sizes.len(),
            row.len()
        )));
    }

    let data = row.raw_data();
    let mut rest = data;
    for (index, &wire_size) in wire_sizes.iter().enumerate() {
        let truncated = || Error::Decode(format!("RefFromRow: column {index} is truncated"));
        let (len, value_and_rest) = rest.split_first_chunk::<4>().ok_or_else(truncated)?;
        let len = i32::from_be_bytes(*len);
        if len == -1 {
            return Err(Error::Decode(format!("RefFromRow: column {index} is NULL")));
        }
        let value_size = wire_size.saturating_sub(4);
        if usize::try_from(len).ok() != Some(value_size) {
            return Err(Error::Decode(format!(
                "RefFromRow: column {index} has {len} bytes, expected {value_size}"
            )));
        }
        rest = value_and_rest.get(value_size..).ok_or_else(truncated)?;
    }
    if !rest.is_empty() {
        return Err(Error::Decode(format!(
            "RefFromRow: {} unexpected bytes after the last column",
            rest.len()
        )));
    }

    T::ref_from_bytes(data).map_err(|e| Error::LibraryBug(format!("RefFromRow cast: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_prefixed_i32() {
        // Wire format: length=4 (BE), value=42 (BE)
        let data: &[u8] = &[0, 0, 0, 4, 0, 0, 0, 42];
        let prefixed: &LengthPrefixed<I32BE> = zerocopy::FromBytes::ref_from_bytes(data).unwrap();

        assert_eq!(prefixed.len(), 4);
        assert!(!prefixed.is_null());
        assert_eq!(prefixed.get().get(), 42);
    }

    #[test]
    fn length_prefixed_i64() {
        // Wire format: length=8 (BE), value=12345678901234 (BE)
        let value: i64 = 12345678901234;
        let mut data = [0u8; 12];
        data[0..4].copy_from_slice(&8_i32.to_be_bytes());
        data[4..12].copy_from_slice(&value.to_be_bytes());

        let prefixed: &LengthPrefixed<I64BE> = zerocopy::FromBytes::ref_from_bytes(&data).unwrap();

        assert_eq!(prefixed.len(), 8);
        assert_eq!(prefixed.get().get(), value);
    }

    #[test]
    fn wire_size() {
        assert_eq!(<LengthPrefixed<I32BE> as FixedWireSize>::WIRE_SIZE, 8);
        assert_eq!(<LengthPrefixed<I64BE> as FixedWireSize>::WIRE_SIZE, 12);
        assert_eq!(<LengthPrefixed<I16BE> as FixedWireSize>::WIRE_SIZE, 6);
    }

    #[test]
    fn contiguous_struct() {
        // Simulate a row with two columns: INT4 (42) and INT8 (12345)
        // Wire format: [len=4][val=42][len=8][val=12345]
        let mut data = [0u8; 20]; // 8 + 12 = 20 bytes
        // First column: INT4
        data[0..4].copy_from_slice(&4_i32.to_be_bytes());
        data[4..8].copy_from_slice(&42_i32.to_be_bytes());
        // Second column: INT8
        data[8..12].copy_from_slice(&8_i32.to_be_bytes());
        data[12..20].copy_from_slice(&12345_i64.to_be_bytes());

        #[derive(FromBytes, KnownLayout, Immutable)]
        #[repr(C, packed)]
        struct TestRow {
            col1: LengthPrefixed<I32BE>,
            col2: LengthPrefixed<I64BE>,
        }

        let row: &TestRow = zerocopy::FromBytes::ref_from_bytes(&data).unwrap();
        assert_eq!(row.col1.get().get(), 42);
        assert_eq!(row.col2.get().get(), 12345);
    }
}
