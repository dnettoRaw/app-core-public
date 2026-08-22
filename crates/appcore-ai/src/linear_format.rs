// =============================================================================
//        #######
//     ###       ###     F: linear_format.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiError, AiResult, ArtifactDigest, ArtifactIdentity, CapabilityId};

const MAGIC: &[u8; 8] = b"APCAILN1";
const MAX_LABEL_BYTES: usize = 96;

/// Portable bounded linear classifier artifact consumed by the Candle backend.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeLinearArtifact {
    input_dimensions: usize,
    labels: Vec<String>,
    weights: Vec<f32>,
    biases: Vec<f32>,
}

impl NativeLinearArtifact {
    /// Validates a row-major `[labels, input_dimensions]` weight matrix.
    pub fn new(
        input_dimensions: usize,
        labels: Vec<String>,
        weights: Vec<f32>,
        biases: Vec<f32>,
    ) -> AiResult<Self> {
        let artifact = Self {
            input_dimensions,
            labels,
            weights,
            biases,
        };
        artifact.validate(usize::MAX, usize::MAX)?;
        Ok(artifact)
    }

    /// Number of deterministic input features.
    #[must_use]
    pub fn input_dimensions(&self) -> usize {
        self.input_dimensions
    }

    /// Ordered class labels.
    #[must_use]
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// Row-major classifier weights.
    #[must_use]
    pub fn weights(&self) -> &[f32] {
        &self.weights
    }

    /// Per-class biases.
    #[must_use]
    pub fn biases(&self) -> &[f32] {
        &self.biases
    }

    /// Encodes the current exact version without native code or executable payloads.
    pub fn encode(&self) -> AiResult<Vec<u8>> {
        self.validate(usize::MAX, usize::MAX)?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        push_u32(&mut bytes, self.input_dimensions)?;
        push_u32(&mut bytes, self.labels.len())?;
        for label in &self.labels {
            let size = u16::try_from(label.len())
                .map_err(|_| AiError::InvalidInput("linear label length"))?;
            bytes.extend_from_slice(&size.to_le_bytes());
            bytes.extend_from_slice(label.as_bytes());
        }
        for value in self.weights.iter().chain(&self.biases) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        Ok(bytes)
    }

    /// Builds content identity and explicit provenance requirements for encoded bytes.
    pub fn identity(
        &self,
        publisher: Option<CapabilityId>,
        signature_required: bool,
    ) -> AiResult<ArtifactIdentity> {
        let bytes = self.encode()?;
        Ok(ArtifactIdentity {
            digest: ArtifactDigest::from_bytes(&bytes),
            size_bytes: u64::try_from(bytes.len()).map_err(|_| AiError::Capacity("artifact"))?,
            publisher,
            signature_required,
        })
    }

    /// Decodes exact V1 bytes with caller-owned dimension and class ceilings.
    pub fn decode(bytes: &[u8], max_dimensions: usize, max_classes: usize) -> AiResult<Self> {
        let mut cursor = Cursor::new(bytes);
        if cursor.take(MAGIC.len())? != MAGIC {
            return Err(AiError::Integrity("linear artifact magic"));
        }
        let input_dimensions = cursor.u32()?;
        let class_count = cursor.u32()?;
        let input_dimensions = usize::try_from(input_dimensions)
            .map_err(|_| AiError::InvalidInput("linear dimensions"))?;
        let class_count =
            usize::try_from(class_count).map_err(|_| AiError::InvalidInput("linear classes"))?;
        if input_dimensions == 0
            || input_dimensions > max_dimensions
            || class_count == 0
            || class_count > max_classes
        {
            return Err(AiError::InvalidInput("linear artifact bounds"));
        }
        let mut labels = Vec::with_capacity(class_count);
        for _ in 0..class_count {
            let length = usize::from(cursor.u16()?);
            let label = std::str::from_utf8(cursor.take(length)?)
                .map_err(|_| AiError::Integrity("linear label encoding"))?
                .to_owned();
            labels.push(label);
        }
        let weight_count = input_dimensions
            .checked_mul(class_count)
            .ok_or(AiError::Capacity("linear weights"))?;
        let mut weights = Vec::with_capacity(weight_count);
        for _ in 0..weight_count {
            weights.push(cursor.f32()?);
        }
        let mut biases = Vec::with_capacity(class_count);
        for _ in 0..class_count {
            biases.push(cursor.f32()?);
        }
        if !cursor.is_complete() {
            return Err(AiError::Integrity("linear artifact trailing bytes"));
        }
        let artifact = Self {
            input_dimensions,
            labels,
            weights,
            biases,
        };
        artifact.validate(max_dimensions, max_classes)?;
        Ok(artifact)
    }

    fn validate(&self, max_dimensions: usize, max_classes: usize) -> AiResult<()> {
        if self.input_dimensions == 0
            || self.input_dimensions > max_dimensions
            || self.labels.is_empty()
            || self.labels.len() > max_classes
            || self.biases.len() != self.labels.len()
            || self.weights.len()
                != self
                    .input_dimensions
                    .checked_mul(self.labels.len())
                    .ok_or(AiError::Capacity("linear weights"))?
            || self
                .labels
                .iter()
                .any(|label| label.is_empty() || label.len() > MAX_LABEL_BYTES)
            || self
                .weights
                .iter()
                .chain(&self.biases)
                .any(|value| !value.is_finite())
        {
            return Err(AiError::InvalidInput("linear artifact"));
        }
        Ok(())
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> AiResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(AiError::Integrity("linear artifact length"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(AiError::Integrity("linear artifact truncated"))?;
        self.offset = end;
        Ok(value)
    }

    fn u16(&mut self) -> AiResult<u16> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| AiError::Integrity("linear integer"))?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> AiResult<u32> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| AiError::Integrity("linear integer"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn f32(&mut self) -> AiResult<f32> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| AiError::Integrity("linear float"))?;
        let value = f32::from_le_bytes(bytes);
        if !value.is_finite() {
            return Err(AiError::Integrity("linear non-finite float"));
        }
        Ok(value)
    }

    fn is_complete(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

fn push_u32(bytes: &mut Vec<u8>, value: usize) -> AiResult<()> {
    let value = u32::try_from(value).map_err(|_| AiError::InvalidInput("linear dimensions"))?;
    bytes.extend_from_slice(&value.to_le_bytes());
    Ok(())
}
