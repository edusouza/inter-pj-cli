//! Serde helpers shared by the API models.

use chrono::NaiveDate;

/// Parses the date formats found in the APIs' responses: `AAAA-MM-DD`
/// (possibly followed by a time, as in `2022-07-06 11:12:29.810`) and
/// `DD/MM/AAAA`.
pub(crate) fn parse_date(raw: &str) -> Option<NaiveDate> {
    let raw = raw.trim();
    let iso = raw.get(..10).unwrap_or(raw);
    NaiveDate::parse_from_str(iso, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(raw, "%d/%m/%Y"))
        .ok()
}

/// Defines an enum with the codes the API documents for a field, plus
/// `Outro(String)`, so codes added later do not break deserialization.
/// Generates `DOCUMENTADOS`, `as_str` and `From<&str>`.
macro_rules! api_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$doc:meta])* $variant:ident => $api:literal, )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $( $(#[$doc])* $variant, )*
            /// A code the API does not document, kept as received.
            Outro(String),
        }

        impl $name {
            /// Every code documented by the API.
            pub const DOCUMENTADOS: &'static [$name] = &[$($name::$variant,)*];

            /// Code used by the API (e.g. `PIX`).
            pub fn as_str(&self) -> &str {
                match self {
                    $( Self::$variant => $api, )*
                    Self::Outro(raw) => raw,
                }
            }
        }

        impl From<&str> for $name {
            fn from(raw: &str) -> Self {
                match raw.trim() {
                    $( $api => Self::$variant, )*
                    other => Self::Outro(other.to_owned()),
                }
            }
        }
    };
}
pub(crate) use api_enum;

/// Implements `Display`, `Serialize` and `Deserialize` for code enums through
/// their `as_str` and `From<&str>`.
macro_rules! string_serde {
    ($($ty:ty),*) => {$(
        impl std::fmt::Display for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(
                &self,
                serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> std::result::Result<Self, D::Error> {
                let raw = <String as serde::Deserialize>::deserialize(deserializer)?;
                Ok(Self::from(raw.as_str()))
            }
        }
    )*};
}
pub(crate) use string_serde;

/// Deserializers that tolerate the type variations seen across endpoints
/// (numbers sent as strings and vice versa).
pub(crate) mod lenient {
    use std::str::FromStr;

    use rust_decimal::Decimal;
    use serde::de::Error;
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    /// Text that may also arrive as a number or a boolean. Kept as sent,
    /// empty strings included.
    pub(crate) fn string<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<String>, D::Error> {
        match Option::<Value>::deserialize(deserializer)? {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) => Ok(Some(text)),
            Some(Value::Number(number)) => Ok(Some(number.to_string())),
            Some(Value::Bool(flag)) => Ok(Some(flag.to_string())),
            Some(_) => Err(D::Error::custom("esperado texto")),
        }
    }

    /// Monetary value sent as a JSON number or as a string (`"100.50"`).
    /// Blank strings mean "not informed".
    pub(crate) fn decimal<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Decimal>, D::Error> {
        let text = match Option::<Value>::deserialize(deserializer)? {
            None | Some(Value::Null) => return Ok(None),
            // serde_json prints the shortest representation that round-trips,
            // so 2850.55 stays exactly 2850.55.
            Some(Value::Number(number)) => number.to_string(),
            Some(Value::String(text)) if text.trim().is_empty() => return Ok(None),
            Some(Value::String(text)) => text.trim().to_owned(),
            Some(_) => return Err(D::Error::custom("valor monetário inválido")),
        };
        Decimal::from_str(&text)
            .or_else(|_| Decimal::from_scientific(&text))
            .map(Some)
            .map_err(|_| D::Error::custom(format!("valor monetário inválido: \"{text}\"")))
    }

    /// List that may also arrive as `null`.
    pub(crate) fn vec<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<Vec<T>, D::Error> {
        Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
    }

    /// Counter sent as a number or a numeric string; other values are ignored.
    pub(crate) fn u64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
        Ok(match Option::<Value>::deserialize(deserializer)? {
            Some(Value::Number(number)) => number.as_u64(),
            Some(Value::String(text)) => text.trim().parse().ok(),
            _ => None,
        })
    }

    /// Flag sent as a boolean or as `"true"`/`"false"`; other values are ignored.
    pub(crate) fn bool<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<bool>, D::Error> {
        Ok(match Option::<Value>::deserialize(deserializer)? {
            Some(Value::Bool(flag)) => Some(flag),
            Some(Value::String(text)) => match text.trim() {
                t if t.eq_ignore_ascii_case("true") => Some(true),
                t if t.eq_ignore_ascii_case("false") => Some(false),
                _ => None,
            },
            _ => None,
        })
    }

    #[cfg(test)]
    mod tests {
        use rust_decimal::Decimal;
        use serde::Deserialize;

        #[derive(Debug, Deserialize)]
        struct Campos {
            #[serde(default, deserialize_with = "super::string")]
            texto: Option<String>,
            #[serde(default, deserialize_with = "super::decimal")]
            valor: Option<Decimal>,
            #[serde(default, deserialize_with = "super::u64")]
            total: Option<u64>,
            #[serde(default, deserialize_with = "super::bool")]
            mais: Option<bool>,
        }

        fn campos(json: &str) -> Campos {
            serde_json::from_str(json).unwrap()
        }

        #[test]
        fn text_accepts_scalars_and_keeps_empty_strings() {
            assert_eq!(campos(r#"{"texto":"abc"}"#).texto.as_deref(), Some("abc"));
            assert_eq!(campos(r#"{"texto":""}"#).texto.as_deref(), Some(""));
            assert_eq!(campos(r#"{"texto":42}"#).texto.as_deref(), Some("42"));
            assert_eq!(campos(r#"{"texto":null}"#).texto, None);
            assert_eq!(campos("{}").texto, None);
            assert!(serde_json::from_str::<Campos>(r#"{"texto":{"a":1}}"#).is_err());
        }

        #[test]
        fn decimals_are_exact_from_numbers_and_strings() {
            let dec = |s: &str| Some(s.parse::<Decimal>().unwrap());
            assert_eq!(campos(r#"{"valor":"100.50"}"#).valor, dec("100.50"));
            assert_eq!(campos(r#"{"valor":" -7.1 "}"#).valor, dec("-7.1"));
            assert_eq!(campos(r#"{"valor":2850.55}"#).valor, dec("2850.55"));
            assert_eq!(campos(r#"{"valor":1e3}"#).valor, dec("1000"));
            assert_eq!(campos(r#"{"valor":""}"#).valor, None);
            let err = serde_json::from_str::<Campos>(r#"{"valor":"R$ 10"}"#).unwrap_err();
            assert!(
                err.to_string().contains("valor monetário inválido"),
                "{err}"
            );
        }

        #[test]
        fn counters_and_flags_are_lenient() {
            assert_eq!(campos(r#"{"total":"12"}"#).total, Some(12));
            assert_eq!(campos(r#"{"total":12}"#).total, Some(12));
            assert_eq!(campos(r#"{"total":"x"}"#).total, None);
            assert_eq!(campos(r#"{"mais":"true"}"#).mais, Some(true));
            assert_eq!(campos(r#"{"mais":false}"#).mais, Some(false));
            assert_eq!(campos(r#"{"mais":1}"#).mais, None);
        }
    }
}

/// Serializes monetary values as JSON numbers (the API's own representation)
/// instead of the string representation `rust_decimal` uses by default.
///
/// Whole values are written as integers (`1000`), others through their
/// shortest round-trip `f64` representation (`2850.55`).
///
/// Deserialization uses `rust_decimal`'s default implementation, which accepts
/// both numbers and strings and converts floats through their shortest
/// round-trip representation (so `2850.55` stays exactly `2850.55`).
pub(crate) mod decimal_as_number {
    use rust_decimal::Decimal;
    use rust_decimal::prelude::ToPrimitive;
    use serde::Serializer;
    use serde::ser::Error;

    pub(crate) fn serialize<S: Serializer>(
        value: &Decimal,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if value.fract().is_zero()
            && let Some(integer) = value.to_i64()
        {
            return serializer.serialize_i64(integer);
        }
        let float = value
            .to_f64()
            .ok_or_else(|| S::Error::custom("valor monetário fora do intervalo representável"))?;
        serializer.serialize_f64(float)
    }

    #[allow(clippy::ref_option)] // signature imposed by `serialize_with`
    pub(crate) fn serialize_option<S: Serializer>(
        value: &Option<Decimal>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serialize(value, serializer),
            None => serializer.serialize_none(),
        }
    }

    #[cfg(test)]
    mod tests {
        use std::str::FromStr;

        use rust_decimal::Decimal;

        #[derive(serde::Serialize)]
        struct Wrapper(#[serde(serialize_with = "super::serialize")] Decimal);

        fn json(value: &str) -> String {
            serde_json::to_string(&Wrapper(Decimal::from_str(value).unwrap())).unwrap()
        }

        #[test]
        fn whole_values_are_integers() {
            assert_eq!(json("1000"), "1000");
            assert_eq!(json("0"), "0");
            assert_eq!(json("-5.00"), "-5");
        }

        #[test]
        fn fractional_values_keep_their_digits() {
            assert_eq!(json("2850.55"), "2850.55");
            assert_eq!(json("0.1"), "0.1");
            assert_eq!(json("-12.30"), "-12.3");
            assert_eq!(json("99999999999.99"), "99999999999.99");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_date_formats() {
        let esperado = NaiveDate::from_ymd_opt(2022, 7, 6);
        assert_eq!(parse_date("2022-07-06"), esperado);
        assert_eq!(parse_date("2022-07-06 11:12:29.810"), esperado);
        assert_eq!(parse_date("2022-07-06T11:12:29-03:00"), esperado);
        assert_eq!(parse_date("06/07/2022"), esperado);
        assert_eq!(parse_date(" 2022-07-06 "), esperado);
        assert_eq!(parse_date(""), None);
        assert_eq!(parse_date("ontem"), None);
        assert_eq!(parse_date("2022-13-01"), None);
    }
}
