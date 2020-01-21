use chrono_tz::Tz;

use crate::{
    binary::ReadEx,
    errors::Result,
    types::column::{
        array::ArrayColumnData, column_data::ColumnData, date::DateColumnData,
        decimal::DecimalColumnData, fixed_string::FixedStringColumnData, list::List,
        nullable::NullableColumnData, numeric::VectorColumnData, string::StringColumnData,
        ip::{IpColumnData, Ipv4, Ipv6, Uuid},
        BoxColumnWrapper, ColumnWrapper,
    },
    types::decimal::NoBits,
    SqlType,
};

const DEFAULT_CAPACITY: usize = 100;

impl dyn ColumnData {
    pub(crate) fn load_data<W: ColumnWrapper, T: ReadEx>(
        reader: &mut T,
        type_name: &str,
        size: usize,
        tz: Tz,
    ) -> Result<W::Wrapper> {
        Ok(match type_name {
            "UInt8" => W::wrap(VectorColumnData::<u8>::load(reader, size)?),
            "UInt16" => W::wrap(VectorColumnData::<u16>::load(reader, size)?),
            "UInt32" => W::wrap(VectorColumnData::<u32>::load(reader, size)?),
            "UInt64" => W::wrap(VectorColumnData::<u64>::load(reader, size)?),
            "Int8" => W::wrap(VectorColumnData::<i8>::load(reader, size)?),
            "Int16" => W::wrap(VectorColumnData::<i16>::load(reader, size)?),
            "Int32" => W::wrap(VectorColumnData::<i32>::load(reader, size)?),
            "Int64" => W::wrap(VectorColumnData::<i64>::load(reader, size)?),
            "Float32" => W::wrap(VectorColumnData::<f32>::load(reader, size)?),
            "Float64" => W::wrap(VectorColumnData::<f64>::load(reader, size)?),
            "String" => W::wrap(StringColumnData::load(reader, size)?),
            "Date" => W::wrap(DateColumnData::<u16>::load(reader, size, tz)?),
            "DateTime" => W::wrap(DateColumnData::<u32>::load(reader, size, tz)?),
            "IPv4" => W::wrap(IpColumnData::<Ipv4>::load(reader, size)?),
            "IPv6" => W::wrap(IpColumnData::<Ipv6>::load(reader, size)?),
            "UUID" => W::wrap(IpColumnData::<Uuid>::load(reader, size)?),
            _ => {
                if let Some(inner_type) = parse_nullable_type(type_name) {
                    W::wrap(NullableColumnData::load(reader, inner_type, size, tz)?)
                } else if let Some(str_len) = parse_fixed_string(type_name) {
                    W::wrap(FixedStringColumnData::load(reader, size, str_len)?)
                } else if let Some(inner_type) = parse_array_type(type_name) {
                    W::wrap(ArrayColumnData::load(reader, inner_type, size, tz)?)
                } else if let Some((precision, scale, nobits)) = parse_decimal(type_name) {
                    W::wrap(DecimalColumnData::load(
                        reader, precision, scale, nobits, size, tz,
                    )?)
                } else if let Some(_items) = parse_enum8(type_name) {
                    W::wrap(VectorColumnData::<i8>::load(reader, size)?)
                } else if let Some(_items) = parse_enum16(type_name) {
                    W::wrap(VectorColumnData::<i16>::load(reader, size)?)
                } else {
                    let message = format!("Unsupported column type \"{}\".", type_name);
                    return Err(message.into());
                }
            }
        })
    }

    pub(crate) fn from_type<W: ColumnWrapper>(
        sql_type: SqlType,
        timezone: Tz,
    ) -> Result<W::Wrapper> {
        Ok(match sql_type {
            SqlType::UInt8 => W::wrap(VectorColumnData::<u8>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::UInt16 => W::wrap(VectorColumnData::<u16>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::UInt32 => W::wrap(VectorColumnData::<u32>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::UInt64 => W::wrap(VectorColumnData::<u64>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::Int8 => W::wrap(VectorColumnData::<i8>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::Int16 => W::wrap(VectorColumnData::<i16>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::Int32 => W::wrap(VectorColumnData::<i32>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::Int64 => W::wrap(VectorColumnData::<i64>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::String => W::wrap(StringColumnData::with_capacity(DEFAULT_CAPACITY)),
            SqlType::FixedString(len) => {
                W::wrap(FixedStringColumnData::with_capacity(DEFAULT_CAPACITY, len))
            }
            SqlType::Float32 => W::wrap(VectorColumnData::<f32>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::Float64 => W::wrap(VectorColumnData::<f64>::with_capacity(DEFAULT_CAPACITY)),

            SqlType::Ipv4 => W::wrap(IpColumnData::<Ipv4>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::Ipv6 => W::wrap(IpColumnData::<Ipv6>::with_capacity(DEFAULT_CAPACITY)),
            SqlType::Uuid => W::wrap(IpColumnData::<Uuid>::with_capacity(DEFAULT_CAPACITY)),

            SqlType::Date => W::wrap(DateColumnData::<u16>::with_capacity(
                DEFAULT_CAPACITY,
                timezone,
            )),
            SqlType::DateTime => W::wrap(DateColumnData::<u32>::with_capacity(
                DEFAULT_CAPACITY,
                timezone,
            )),
            SqlType::Nullable(inner_type) => W::wrap(NullableColumnData {
                inner: ColumnData::from_type::<BoxColumnWrapper>(*inner_type, timezone)?,
                nulls: Vec::new(),
            }),
            SqlType::Array(inner_type) => W::wrap(ArrayColumnData {
                inner: ColumnData::from_type::<BoxColumnWrapper>(*inner_type, timezone)?,
                offsets: List::with_capacity(DEFAULT_CAPACITY),
            }),
            SqlType::Decimal(precision, scale) => {
                let nobits = NoBits::from_precision(precision).unwrap();

                let inner_type = match nobits {
                    NoBits::N32 => SqlType::Int32,
                    NoBits::N64 => SqlType::Int64,
                };

                W::wrap(DecimalColumnData {
                    inner: ColumnData::from_type::<BoxColumnWrapper>(inner_type, timezone)?,
                    precision,
                    scale,
                    nobits,
                })
            }
        })
    }
}

fn parse_fixed_string(source: &str) -> Option<usize> {
    if !source.starts_with("FixedString") {
        return None;
    }

    let inner_size = &source[12..source.len() - 1];
    match inner_size.parse::<usize>() {
        Err(_) => None,
        Ok(value) => Some(value),
    }
}

fn parse_nullable_type(source: &str) -> Option<&str> {
    if !source.starts_with("Nullable") {
        return None;
    }

    let inner_type = &source[9..source.len() - 1];

    if inner_type.starts_with("Nullable") {
        return None;
    }

    Some(inner_type)
}

fn parse_array_type(source: &str) -> Option<&str> {
    if !source.starts_with("Array") {
        return None;
    }

    let inner_type = &source[6..source.len() - 1];
    Some(inner_type)
}

fn parse_decimal(source: &str) -> Option<(u8, u8, NoBits)> {
    if source.len() < 12 {
        return None;
    }

    if !source.starts_with("Decimal") {
        return None;
    }

    if source.chars().nth(7) != Some('(') {
        return None;
    }

    if !source.ends_with(')') {
        return None;
    }

    let mut params: Vec<u8> = Vec::with_capacity(2);
    for cell in source[8..source.len() - 1].split(',').map(|s| s.trim()) {
        if let Ok(value) = cell.parse() {
            params.push(value)
        } else {
            return None;
        }
    }

    if params.len() != 2 {
        return None;
    }

    let precision = params[0];
    let scale = params[1];

    if scale > precision {
        return None;
    }

    if let Some(nobits) = NoBits::from_precision(precision) {
        return Some((precision, scale, nobits));
    }

    None
}

fn parse_enum8(source: &str) -> Option<Vec<(String, i8)>> {
    let prefix = "Enum8(";
    let suffix = ")";
    if !source.starts_with(prefix) || !source.ends_with(suffix) {
        return None;
    }
    let _source = &source[prefix.len()..source.len() - suffix.len()];

    Some(vec![]) // TODO
}

fn parse_enum16(source: &str) -> Option<Vec<(String, i8)>> {
    let prefix = "Enum16(";
    let suffix = ")";
    if !source.starts_with(prefix) || !source.ends_with(suffix) {
        return None;
    }
    let _source = &source[prefix.len()..source.len() - suffix.len()];

    Some(vec![]) // TODO
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_decimal() {
        assert_eq!(parse_decimal("Decimal(9, 4)"), Some((9, 4, NoBits::N32)));
        assert_eq!(parse_decimal("Decimal(10, 4)"), Some((10, 4, NoBits::N64)));
        assert_eq!(parse_decimal("Decimal(20, 4)"), None);
        assert_eq!(parse_decimal("Decimal(2000, 4)"), None);
        assert_eq!(parse_decimal("Decimal(3, 4)"), None);
        assert_eq!(parse_decimal("Decimal(20, -4)"), None);
        assert_eq!(parse_decimal("Decimal(0)"), None);
        assert_eq!(parse_decimal("Decimal(1, 2, 3)"), None);
    }

    #[test]
    fn test_parse_array_type() {
        assert_eq!(parse_array_type("Array(UInt8)"), Some("UInt8"));
    }

    #[test]
    fn test_parse_nullable_type() {
        assert_eq!(parse_nullable_type("Nullable(Int8)"), Some("Int8"));
        assert_eq!(parse_nullable_type("Int8"), None);
        assert_eq!(parse_nullable_type("Nullable(Nullable(Int8))"), None);
    }

    #[test]
    fn test_parse_fixed_string() {
        assert_eq!(parse_fixed_string("FixedString(8)"), Some(8_usize));
        assert_eq!(parse_fixed_string("FixedString(zz)"), None);
        assert_eq!(parse_fixed_string("Int8"), None);
    }
}
