// =============================================================================
//        #######
//     ###       ###     F: modality.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

/// Backend-neutral class of content consumed or produced by a model.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum AiModality {
    /// UTF-8 text, including extracted document text.
    Text,
    /// A still image such as PNG, JPEG or WebP.
    Image,
    /// A document container such as PDF.
    Document,
    /// Audio content.
    Audio,
    /// Video content.
    Video,
    /// Valid bounded content whose modality is provider-specific.
    Opaque,
}

impl AiModality {
    /// Classifies a previously validated media type without decoding its bytes.
    #[must_use]
    pub fn from_media_type(media_type: &str) -> Self {
        if media_type.starts_with("image/") {
            Self::Image
        } else if media_type.starts_with("audio/") {
            Self::Audio
        } else if media_type.starts_with("video/") {
            Self::Video
        } else if media_type == "application/pdf"
            || media_type.starts_with("application/vnd.oasis.opendocument.")
            || media_type.starts_with("application/vnd.openxmlformats-officedocument.")
        {
            Self::Document
        } else if media_type.starts_with("text/") {
            Self::Text
        } else {
            Self::Opaque
        }
    }
}
