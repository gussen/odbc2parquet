//! Strategies for fetching data where ODBC and parquet type are binary identical.

use std::marker::PhantomData;

use anyhow::Error;
use odbc_api::buffers::{AnyColumnView, BufferDescription, Item};
use parquet::{
    basic::{ConvertedType, Repetition, Type as PhysicalType},
    column::writer::{get_typed_column_writer_mut, ColumnWriter},
    data_type::DataType,
    schema::types::Type,
};

use crate::parquet_buffer::{BufferedDataType, ParquetBuffer};

use super::ColumnFetchStrategy;

/// Copy identical optional data from ODBC to Parquet.
pub struct IdenticalOptional<Pdt> {
    converted_type: ConvertedType,
    precision: Option<i32>,
    _parquet_data_type: PhantomData<Pdt>,
}

/// Columnar fetch strategy to be applied if Parquet and Odbc value type are binary identical.
/// Generic argument is a parquet data type.
impl<Pdt> IdenticalOptional<Pdt>
where
    Pdt: DataType
{
    pub fn new() -> Self {
        Self::with_converted_type(ConvertedType::NONE)
    }

    /// Odbc buffer and parquet type are identical, but we want to annotate the parquet column with
    /// a specific converted type (aka. former logical type).
    pub fn with_converted_type(converted_type: ConvertedType) -> Self {
        Self {
            converted_type,
            precision: None,
            _parquet_data_type: PhantomData,
        }
    }

    /// For decimal types with a Scale of zero we can have a binary identical ODBC parquet
    /// representation as either 32 or 64 bit integers.
    pub fn decimal_with_precision(precision: i32, prefer_int_over_decimal: bool) -> Self {
        let physical_type = Pdt::get_physical_type();
        if prefer_int_over_decimal {
            match physical_type {
                PhysicalType::INT32 => {
                    Self {
                        converted_type: ConvertedType::INT_32,
                        precision: None,
                        _parquet_data_type: PhantomData,
                    }
                }
                PhysicalType::INT64 => {
                    Self {
                        converted_type: ConvertedType::INT_64,
                        precision: None,
                        _parquet_data_type: PhantomData,
                    }
                }
                _ => panic!("Only INT32 and INT64 are allowed to represent Decimal with scale 0")
            }
        } else {
            Self {
                converted_type: ConvertedType::DECIMAL,
                precision: Some(precision),
                _parquet_data_type: PhantomData,
            }
        }
    }
}

impl<Pdt> ColumnFetchStrategy for IdenticalOptional<Pdt>
where
    Pdt: DataType,
    Pdt::T: Item + BufferedDataType,
{
    fn parquet_type(&self, name: &str) -> Type {
        let physical_type = Pdt::get_physical_type();
        let mut builder = Type::primitive_type_builder(name, physical_type)
            .with_repetition(Repetition::OPTIONAL)
            .with_converted_type(self.converted_type);
        if let Some(precision) = self.precision {
            builder = builder.with_scale(0).with_precision(precision);
        }
        builder.build().unwrap()
    }

    fn buffer_description(&self) -> BufferDescription {
        BufferDescription {
            kind: Pdt::T::BUFFER_KIND,
            nullable: true,
        }
    }

    fn copy_odbc_to_parquet(
        &self,
        parquet_buffer: &mut ParquetBuffer,
        column_writer: &mut ColumnWriter,
        column_view: AnyColumnView,
    ) -> Result<(), Error> {
        let it = Pdt::T::as_nullable_slice(column_view).unwrap();
        let column_writer = get_typed_column_writer_mut::<Pdt>(column_writer);
        parquet_buffer.write_optional(column_writer, it.map(|opt_ref| opt_ref.copied()))?;
        Ok(())
    }
}

/// Optimized strategy if ODBC and Parquet type are identical, and we know the data source not to
/// contain any NULLs.
pub struct IdenticalRequired<Pdt> {
    converted_type: ConvertedType,
    precision: Option<i32>,
    _parquet_data_type: PhantomData<Pdt>,
}

impl<Pdt> IdenticalRequired<Pdt>
where
    Pdt: DataType
{
    pub fn new() -> Self {
        Self::with_converted_type(ConvertedType::NONE)
    }

    /// Odbc buffer and parquet type are identical, but we want to annotate the parquet column with
    /// a specific converted type (aka. former logical type).
    pub fn with_converted_type(converted_type: ConvertedType) -> Self {
        Self {
            converted_type,
            precision: None,
            _parquet_data_type: PhantomData,
        }
    }

    /// For decimal types with a Scale of zero we can have a binary identical ODBC parquet
    /// representation as either 32 or 64 bit integers.
    pub fn decimal_with_precision(precision: i32, prefer_int_over_decimal: bool) -> Self {
        let physical_type = Pdt::get_physical_type();
        if prefer_int_over_decimal {
            match physical_type {
                PhysicalType::INT32 => {
                    Self {
                        converted_type: ConvertedType::INT_32,
                        precision: None,
                        _parquet_data_type: PhantomData,
                    }
                }
                PhysicalType::INT64 => {
                    Self {
                        converted_type: ConvertedType::INT_64,
                        precision: None,
                        _parquet_data_type: PhantomData,
                    }
                }
                _ => panic!("Only INT32 and INT64 are allowed to represent Decimal with scale 0")
            }
        } else {
            Self {
                converted_type: ConvertedType::DECIMAL,
                precision: Some(precision),
                _parquet_data_type: PhantomData,
            }
        }
    }
}

impl<Pdt> ColumnFetchStrategy for IdenticalRequired<Pdt>
where
    Pdt: DataType,
    Pdt::T: Item + BufferedDataType,
{
    fn parquet_type(&self, name: &str) -> Type {
        let physical_type = Pdt::get_physical_type();
        let mut builder = Type::primitive_type_builder(name, physical_type)
            .with_repetition(Repetition::REQUIRED)
            .with_converted_type(self.converted_type);
        if let Some(precision) = self.precision {
            builder = builder.with_scale(0).with_precision(precision);
        }
        builder.build().unwrap()
    }

    fn buffer_description(&self) -> BufferDescription {
        BufferDescription {
            kind: Pdt::T::BUFFER_KIND,
            nullable: false,
        }
    }

    fn copy_odbc_to_parquet(
        &self,
        _parquet_buffer: &mut ParquetBuffer,
        column_writer: &mut ColumnWriter,
        column_view: AnyColumnView,
    ) -> Result<(), Error> {
        // We do not require to buffer the values, as they must neither be transformed, nor contain
        // any gaps due to null, we can use the ODBC buffer directly to write the batch.

        let values = Pdt::T::as_slice(column_view).unwrap();
        let column_writer = get_typed_column_writer_mut::<Pdt>(column_writer);
        column_writer.write_batch(values, None, None)?;
        Ok(())
    }
}

pub fn fetch_identical<Pdt>(is_optional: bool) -> Box<dyn ColumnFetchStrategy>
where
    Pdt: DataType,
    Pdt::T: Item + BufferedDataType,
{
    if is_optional {
        Box::new(IdenticalOptional::<Pdt>::new())
    } else {
        Box::new(IdenticalRequired::<Pdt>::new())
    }
}

pub fn fetch_identical_with_converted_type<Pdt>(
    is_optional: bool,
    converted_type: ConvertedType,
) -> Box<dyn ColumnFetchStrategy>
where
    Pdt: DataType,
    Pdt::T: Item + BufferedDataType,
{
    if is_optional {
        Box::new(IdenticalOptional::<Pdt>::with_converted_type(
            converted_type,
        ))
    } else {
        Box::new(IdenticalRequired::<Pdt>::with_converted_type(
            converted_type,
        ))
    }
}

pub fn fetch_decimal_as_identical_with_precision<Pdt>(
    is_optional: bool,
    precision: i32,
    prefer_int_over_decimal: bool,
) -> Box<dyn ColumnFetchStrategy>
where
    Pdt: DataType,
    Pdt::T: Item + BufferedDataType,
{
    if is_optional {
        Box::new(IdenticalOptional::<Pdt>::decimal_with_precision(precision, prefer_int_over_decimal))
    } else {
        Box::new(IdenticalRequired::<Pdt>::decimal_with_precision(precision, prefer_int_over_decimal))
    }
}
