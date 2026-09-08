// =============================================================================
//        #######
//     ###       ###     F: base64_bytes.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Stable textual byte representation used by V2 JSON chunk frames.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::de::{SeqAccess, Visitor};
use serde::{Deserializer, Serializer};
use std::fmt::{Display, Formatter};

const BASE64_INPUT_CHUNK_BYTES: usize = 3 * 1024;
const BASE64_OUTPUT_CHUNK_BYTES: usize = 4 * 1024;

pub(super) fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if serializer.is_human_readable() {
        serializer.collect_str(&Base64Display(bytes))
    } else {
        serializer.serialize_bytes(bytes)
    }
}

struct Base64Display<'a>(&'a [u8]);

impl Display for Base64Display<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let mut output = [0_u8; BASE64_OUTPUT_CHUNK_BYTES];
        for input in self.0.chunks(BASE64_INPUT_CHUNK_BYTES) {
            let written = STANDARD
                .encode_slice(input, &mut output)
                .map_err(|_| std::fmt::Error)?;
            let encoded = std::str::from_utf8(&output[..written]).map_err(|_| std::fmt::Error)?;
            formatter.write_str(encoded)?;
        }
        Ok(())
    }
}

pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    if deserializer.is_human_readable() {
        return deserializer.deserialize_str(Base64StringVisitor);
    }
    deserializer.deserialize_bytes(BytesVisitor)
}

struct Base64StringVisitor;

impl Visitor<'_> for Base64StringVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a standard base64 string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        STANDARD.decode(value).map_err(serde::de::Error::custom)
    }

    fn visit_borrowed_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(value)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(&value)
    }
}

struct BytesVisitor;

impl<'de> Visitor<'de> for BytesVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a bounded byte string")
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value.to_vec())
    }

    fn visit_borrowed_bytes<E>(self, value: &'de [u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value.to_vec())
    }

    fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut bytes = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(256 * 1024));
        while let Some(byte) = sequence.next_element()? {
            bytes.push(byte);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct EncodedBytes(#[serde(with = "super")] Vec<u8>);

    #[test]
    fn chunked_display_preserves_exact_json_and_round_trip() {
        for length in [0, 1, 2, 3, 3_071, 3_072, 3_073, 65_536] {
            let bytes = (0..length)
                .map(|index| u8::try_from(index % 251).unwrap())
                .collect::<Vec<_>>();
            let encoded = serde_json::to_string(&EncodedBytes(bytes.clone())).unwrap();
            let expected = format!("\"{}\"", STANDARD.encode(&bytes));

            assert_eq!(encoded, expected);
            assert_eq!(
                serde_json::from_str::<EncodedBytes>(&encoded).unwrap(),
                EncodedBytes(bytes)
            );
        }
    }
}
