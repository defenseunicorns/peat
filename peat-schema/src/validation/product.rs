//! Product validators (AI/ML Products)
//!
//! Validates Product messages and their content types for Peat Protocol.

use super::{ValidationError, ValidationResult};
use crate::product::v1::{
    AlertProduct, AlertSeverity, AlertType, ChatProduct, ClassificationProduct, DetectionProduct,
    EmbeddingProduct, ImageFormat, ImageProduct, Product, ProductType, SegmentationProduct,
    SummaryProduct, SummaryType, TranscriptionProduct,
};

/// Validate a Product message
///
/// Validates:
/// - product_id is present
/// - product_type is specified (not unspecified)
/// - source_platform is present
/// - timestamp is present
/// - confidence is in valid range (0.0 - 1.0)
/// - content is present and valid for the product type
pub fn validate_product(product: &Product) -> ValidationResult<()> {
    // Check required fields
    if product.product_id.is_empty() {
        return Err(ValidationError::MissingField("product_id".to_string()));
    }

    // Product type must be specified
    if product.product_type == ProductType::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "product_type must be specified".to_string(),
        ));
    }

    if product.source_platform.is_empty() {
        return Err(ValidationError::MissingField("source_platform".to_string()));
    }

    // Timestamp is required
    if product.timestamp.is_none() {
        return Err(ValidationError::MissingField("timestamp".to_string()));
    }

    // Confidence must be in valid range
    if product.confidence < 0.0 || product.confidence > 1.0 {
        return Err(ValidationError::InvalidConfidence(product.confidence));
    }

    // Validate model_source if present
    if let Some(ref source) = product.model_source {
        if source.model_id.is_empty() {
            return Err(ValidationError::MissingField(
                "model_source.model_id".to_string(),
            ));
        }
    }

    // Validate content based on type
    use crate::product::v1::product::Content;
    match &product.content {
        Some(Content::Image(img)) => validate_image_product(img)?,
        Some(Content::Classification(cls)) => validate_classification_product(cls)?,
        Some(Content::Detection(det)) => validate_detection_product(det)?,
        Some(Content::Summary(sum)) => validate_summary_product(sum)?,
        Some(Content::Chat(chat)) => validate_chat_product(chat)?,
        Some(Content::Alert(alert)) => validate_alert_product(alert)?,
        Some(Content::Embedding(emb)) => validate_embedding_product(emb)?,
        Some(Content::Segmentation(seg)) => validate_segmentation_product(seg)?,
        Some(Content::Transcription(trans)) => validate_transcription_product(trans)?,
        None => {
            return Err(ValidationError::MissingField("content".to_string()));
        }
    }

    Ok(())
}

/// Validate an ImageProduct (chipout, thumbnail, etc.)
pub fn validate_image_product(image: &ImageProduct) -> ValidationResult<()> {
    // Format must be specified
    if image.format == ImageFormat::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "image format must be specified".to_string(),
        ));
    }

    // Dimensions must be positive
    if image.width == 0 {
        return Err(ValidationError::InvalidValue(
            "image width must be positive".to_string(),
        ));
    }

    if image.height == 0 {
        return Err(ValidationError::InvalidValue(
            "image height must be positive".to_string(),
        ));
    }

    // Must have image data (one of data, data_base64, url, or blob_hash)
    use crate::product::v1::image_product::ImageData;
    match &image.image_data {
        Some(ImageData::Data(bytes)) => {
            if bytes.is_empty() {
                return Err(ValidationError::InvalidValue(
                    "image data must not be empty".to_string(),
                ));
            }
        }
        Some(ImageData::DataBase64(b64)) => {
            if b64.is_empty() {
                return Err(ValidationError::InvalidValue(
                    "image data_base64 must not be empty".to_string(),
                ));
            }
        }
        Some(ImageData::Url(url)) => {
            if url.is_empty() {
                return Err(ValidationError::InvalidValue(
                    "image url must not be empty".to_string(),
                ));
            }
            if !url.contains("://") {
                return Err(ValidationError::InvalidValue(
                    "image url must be a valid URL with scheme".to_string(),
                ));
            }
        }
        Some(ImageData::BlobHash(hash)) => {
            if hash.is_empty() {
                return Err(ValidationError::InvalidValue(
                    "image blob_hash must not be empty".to_string(),
                ));
            }
        }
        None => {
            return Err(ValidationError::MissingField("image_data".to_string()));
        }
    }

    Ok(())
}

/// Validate a ClassificationProduct
pub fn validate_classification_product(cls: &ClassificationProduct) -> ValidationResult<()> {
    if cls.label.is_empty() {
        return Err(ValidationError::MissingField("label".to_string()));
    }

    if cls.confidence < 0.0 || cls.confidence > 1.0 {
        return Err(ValidationError::InvalidConfidence(cls.confidence));
    }

    // Validate top_k scores
    for score in &cls.top_k {
        if score.score < 0.0 || score.score > 1.0 {
            return Err(ValidationError::InvalidConfidence(score.score));
        }
    }

    Ok(())
}

/// Validate a DetectionProduct
pub fn validate_detection_product(det: &DetectionProduct) -> ValidationResult<()> {
    if det.label.is_empty() {
        return Err(ValidationError::MissingField("label".to_string()));
    }

    if det.confidence < 0.0 || det.confidence > 1.0 {
        return Err(ValidationError::InvalidConfidence(det.confidence));
    }

    // Bounding box should have 4 elements [x, y, width, height]
    if det.bbox.len() != 4 {
        return Err(ValidationError::InvalidValue(format!(
            "bbox must have 4 elements, got {}",
            det.bbox.len()
        )));
    }

    // Frame size should have 2 elements [width, height]
    if det.frame_size.len() != 2 {
        return Err(ValidationError::InvalidValue(format!(
            "frame_size must have 2 elements, got {}",
            det.frame_size.len()
        )));
    }

    Ok(())
}

/// Validate a SummaryProduct
pub fn validate_summary_product(summary: &SummaryProduct) -> ValidationResult<()> {
    if summary.text.is_empty() {
        return Err(ValidationError::MissingField("text".to_string()));
    }

    // Summary type must be specified
    if summary.summary_type == SummaryType::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "summary_type must be specified".to_string(),
        ));
    }

    Ok(())
}

/// Validate a ChatProduct
pub fn validate_chat_product(chat: &ChatProduct) -> ValidationResult<()> {
    if chat.response.is_empty() {
        return Err(ValidationError::MissingField("response".to_string()));
    }

    if chat.model_name.is_empty() {
        return Err(ValidationError::MissingField("model_name".to_string()));
    }

    // Temperature should be non-negative
    if chat.temperature < 0.0 {
        return Err(ValidationError::InvalidValue(
            "temperature must be non-negative".to_string(),
        ));
    }

    // top_p should be in [0, 1]
    if chat.top_p < 0.0 || chat.top_p > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "top_p {} must be between 0.0 and 1.0",
            chat.top_p
        )));
    }

    Ok(())
}

/// Validate an AlertProduct
pub fn validate_alert_product(alert: &AlertProduct) -> ValidationResult<()> {
    // Alert type must be specified
    if alert.alert_type == AlertType::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "alert_type must be specified".to_string(),
        ));
    }

    // Severity must be specified
    if alert.severity == AlertSeverity::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "severity must be specified".to_string(),
        ));
    }

    if alert.message.is_empty() {
        return Err(ValidationError::MissingField("message".to_string()));
    }

    Ok(())
}

/// Validate an EmbeddingProduct
pub fn validate_embedding_product(emb: &EmbeddingProduct) -> ValidationResult<()> {
    if emb.vector.is_empty() {
        return Err(ValidationError::MissingField("vector".to_string()));
    }

    if emb.dimensions == 0 {
        return Err(ValidationError::InvalidValue(
            "dimensions must be positive".to_string(),
        ));
    }

    // Vector length should match dimensions
    if emb.vector.len() != emb.dimensions as usize {
        return Err(ValidationError::ConstraintViolation(format!(
            "vector length {} does not match dimensions {}",
            emb.vector.len(),
            emb.dimensions
        )));
    }

    if emb.embedding_model.is_empty() {
        return Err(ValidationError::MissingField("embedding_model".to_string()));
    }

    Ok(())
}

/// Validate a SegmentationProduct
pub fn validate_segmentation_product(seg: &SegmentationProduct) -> ValidationResult<()> {
    if seg.mask_data.is_empty() {
        return Err(ValidationError::MissingField("mask_data".to_string()));
    }

    if seg.width == 0 {
        return Err(ValidationError::InvalidValue(
            "width must be positive".to_string(),
        ));
    }

    if seg.height == 0 {
        return Err(ValidationError::InvalidValue(
            "height must be positive".to_string(),
        ));
    }

    Ok(())
}

/// Validate a TranscriptionProduct
pub fn validate_transcription_product(trans: &TranscriptionProduct) -> ValidationResult<()> {
    if trans.text.is_empty() {
        return Err(ValidationError::MissingField("text".to_string()));
    }

    if trans.language.is_empty() {
        return Err(ValidationError::MissingField("language".to_string()));
    }

    if trans.confidence < 0.0 || trans.confidence > 1.0 {
        return Err(ValidationError::InvalidConfidence(trans.confidence));
    }

    if trans.duration_seconds < 0.0 {
        return Err(ValidationError::InvalidValue(
            "duration_seconds must be non-negative".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::v1::Timestamp;
    use crate::product::v1::product::Content;

    fn valid_detection_product() -> Product {
        Product {
            product_id: "det-001".to_string(),
            product_type: ProductType::Detection as i32,
            source_platform: "Alpha-3".to_string(),
            timestamp: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
            confidence: 0.92,
            model_source: None,
            track_id: String::new(),
            position: None,
            content: Some(Content::Detection(DetectionProduct {
                label: "person".to_string(),
                confidence: 0.92,
                bbox: vec![100, 200, 50, 100],
                frame_size: vec![1920, 1080],
                frame_number: 0,
                detection_index: 0,
            })),
            attributes_json: String::new(),
        }
    }

    #[test]
    fn test_valid_detection_product() {
        let product = valid_detection_product();
        assert!(validate_product(&product).is_ok());
    }

    #[test]
    fn test_missing_product_id() {
        let mut product = valid_detection_product();
        product.product_id = String::new();
        let err = validate_product(&product).unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(f) if f == "product_id"));
    }

    #[test]
    fn test_unspecified_product_type() {
        let mut product = valid_detection_product();
        product.product_type = ProductType::Unspecified as i32;
        let err = validate_product(&product).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidValue(_)));
    }

    #[test]
    fn test_missing_source_platform() {
        let mut product = valid_detection_product();
        product.source_platform = String::new();
        let err = validate_product(&product).unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(f) if f == "source_platform"));
    }

    #[test]
    fn test_invalid_confidence() {
        let mut product = valid_detection_product();
        product.confidence = 1.5;
        let err = validate_product(&product).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidConfidence(_)));
    }

    #[test]
    fn test_missing_content() {
        let mut product = valid_detection_product();
        product.content = None;
        let err = validate_product(&product).unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(f) if f == "content"));
    }

    #[test]
    fn test_invalid_bbox_length() {
        let mut product = valid_detection_product();
        product.content = Some(Content::Detection(DetectionProduct {
            label: "person".to_string(),
            confidence: 0.92,
            bbox: vec![100, 200], // Should have 4 elements
            frame_size: vec![1920, 1080],
            frame_number: 0,
            detection_index: 0,
        }));
        let err = validate_product(&product).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidValue(_)));
    }

    #[test]
    fn test_valid_classification_product() {
        let product = Product {
            product_id: "cls-001".to_string(),
            product_type: ProductType::Classification as i32,
            source_platform: "Alpha-3".to_string(),
            timestamp: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
            confidence: 0.95,
            model_source: None,
            track_id: String::new(),
            position: None,
            content: Some(Content::Classification(ClassificationProduct {
                label: "vehicle".to_string(),
                confidence: 0.95,
                top_k: vec![],
                taxonomy: "coco".to_string(),
            })),
            attributes_json: String::new(),
        };
        assert!(validate_product(&product).is_ok());
    }

    #[test]
    fn test_valid_embedding_product() {
        let product = Product {
            product_id: "emb-001".to_string(),
            product_type: ProductType::Embedding as i32,
            source_platform: "Alpha-3".to_string(),
            timestamp: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
            confidence: 1.0,
            model_source: None,
            track_id: String::new(),
            position: None,
            content: Some(Content::Embedding(EmbeddingProduct {
                vector: vec![0.1, 0.2, 0.3, 0.4],
                dimensions: 4,
                embedding_model: "test-model".to_string(),
                source_hash: String::new(),
                normalized: false,
            })),
            attributes_json: String::new(),
        };
        assert!(validate_product(&product).is_ok());
    }

    #[test]
    fn test_embedding_dimension_mismatch() {
        let product = Product {
            product_id: "emb-001".to_string(),
            product_type: ProductType::Embedding as i32,
            source_platform: "Alpha-3".to_string(),
            timestamp: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
            confidence: 1.0,
            model_source: None,
            track_id: String::new(),
            position: None,
            content: Some(Content::Embedding(EmbeddingProduct {
                vector: vec![0.1, 0.2, 0.3, 0.4],
                dimensions: 8, // Mismatch with vector length
                embedding_model: "test-model".to_string(),
                source_hash: String::new(),
                normalized: false,
            })),
            attributes_json: String::new(),
        };
        let err = validate_product(&product).unwrap_err();
        assert!(matches!(err, ValidationError::ConstraintViolation(_)));
    }
}
