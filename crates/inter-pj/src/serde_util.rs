//! Serde helpers shared by the API models.

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
