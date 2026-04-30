use serde::{
    Deserializer, Serializer,
    de::{self, SeqAccess, Visitor},
};
use std::fmt;
use time::{
    Date, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset,
    format_description::well_known::Rfc3339,
};

pub fn serialize<S>(value: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let formatted = value.format(&Rfc3339).map_err(serde::ser::Error::custom)?;
    serializer.serialize_str(&formatted)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    struct OffsetDateTimeVisitor;

    impl<'de> Visitor<'de> for OffsetDateTimeVisitor {
        type Value = OffsetDateTime;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an RFC3339 datetime string or legacy datetime tuple")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            OffsetDateTime::parse(value, &Rfc3339).map_err(E::custom)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let year = seq
                .next_element::<i32>()?
                .ok_or_else(|| de::Error::invalid_length(0, &self))?;
            let ordinal = seq
                .next_element::<u16>()?
                .ok_or_else(|| de::Error::invalid_length(1, &self))?;
            let hour = seq
                .next_element::<u8>()?
                .ok_or_else(|| de::Error::invalid_length(2, &self))?;
            let minute = seq
                .next_element::<u8>()?
                .ok_or_else(|| de::Error::invalid_length(3, &self))?;
            let second = seq
                .next_element::<u8>()?
                .ok_or_else(|| de::Error::invalid_length(4, &self))?;
            let nanosecond = seq
                .next_element::<u32>()?
                .ok_or_else(|| de::Error::invalid_length(5, &self))?;
            let offset_hours = seq
                .next_element::<i8>()?
                .ok_or_else(|| de::Error::invalid_length(6, &self))?;
            let offset_minutes = seq
                .next_element::<i8>()?
                .ok_or_else(|| de::Error::invalid_length(7, &self))?;
            let offset_seconds = seq
                .next_element::<i8>()?
                .ok_or_else(|| de::Error::invalid_length(8, &self))?;

            let date = Date::from_ordinal_date(year, ordinal).map_err(de::Error::custom)?;
            let time =
                Time::from_hms_nano(hour, minute, second, nanosecond).map_err(de::Error::custom)?;
            let offset = UtcOffset::from_hms(offset_hours, offset_minutes, offset_seconds)
                .map_err(de::Error::custom)?;

            Ok(PrimitiveDateTime::new(date, time).assume_offset(offset))
        }
    }

    deserializer.deserialize_any(OffsetDateTimeVisitor)
}
