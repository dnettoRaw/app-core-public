// =============================================================================
//        #######
//     ###       ###     F: request_debug.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiContent, AiInput, AiMetadata, AiOutput, AiRequest, AiResponse, AiScore};
use std::fmt::{Debug, Formatter};

impl Debug for AiContent {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(value) => formatter
                .debug_struct("Text")
                .field("redacted_bytes", &value.len())
                .finish(),
            Self::Message(message) => message.fmt(formatter),
            Self::Binary { media_type, bytes } => formatter
                .debug_struct("Binary")
                .field("media_type", media_type)
                .field("redacted_bytes", &bytes.len())
                .finish(),
        }
    }
}

impl Debug for AiInput {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let bytes = self.parts().iter().fold(0usize, |sum, part| {
            let size = match part {
                AiContent::Text(value) => value.len(),
                AiContent::Message(message) => message.content.len(),
                AiContent::Binary { media_type, bytes } => {
                    media_type.len().saturating_add(bytes.len())
                }
            };
            sum.saturating_add(size)
        });
        formatter
            .debug_struct("AiInput")
            .field("parts", &self.parts().len())
            .field("redacted_bytes", &bytes)
            .finish()
    }
}

impl Debug for AiRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AiRequest")
            .field("task", &self.task)
            .field("input", &self.input)
            .field("options", &self.options)
            .finish()
    }
}

impl Debug for AiScore {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AiScore")
            .field("redacted_label_bytes", &self.label.len())
            .field("score", &self.score)
            .finish()
    }
}

impl Debug for AiOutput {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(value) => formatter
                .debug_struct("Text")
                .field("redacted_bytes", &value.len())
                .finish(),
            Self::Scores(values) => formatter
                .debug_struct("Scores")
                .field("redacted_items", &values.len())
                .finish(),
            Self::Embedding(values) => formatter
                .debug_struct("Embedding")
                .field("redacted_dimensions", &values.len())
                .finish(),
            Self::ToolCalls(values) => formatter
                .debug_struct("ToolCalls")
                .field("redacted_items", &values.len())
                .finish(),
            Self::Binary { media_type, bytes } => formatter
                .debug_struct("Binary")
                .field("media_type", media_type)
                .field("redacted_bytes", &bytes.len())
                .finish(),
        }
    }
}

impl Debug for AiMetadata {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AiMetadata")
            .field("key", &self.key)
            .field("redacted_value_bytes", &self.value.len())
            .finish()
    }
}

impl Debug for AiResponse {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AiResponse")
            .field("output", &self.output)
            .field("metadata", &self.metadata)
            .field("decision", &self.decision)
            .finish()
    }
}
