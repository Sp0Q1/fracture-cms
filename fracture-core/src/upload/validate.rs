use thiserror::Error;

/// Errors that can occur during file validation.
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("file extension '{0}' is not allowed")]
    DisallowedExtension(String),

    #[error("content type '{0}' is not allowed")]
    DisallowedContentType(String),

    #[error("file content does not match its declared type (magic bytes mismatch)")]
    MagicMismatch,

    #[error("file extension does not match its content")]
    ExtensionContentMismatch,

    #[error("SVG contains potentially dangerous content")]
    UnsafeSvg,
}

/// The result of a successful validation pipeline.
#[derive(Debug, Clone)]
pub struct ValidatedFile {
    /// The validated MIME content type.
    pub content_type: String,
    /// The validated file extension (without leading dot).
    pub extension: String,
    /// The (potentially sanitized) file data.
    pub clean_data: Vec<u8>,
}

/// Allowed file extensions and their corresponding MIME types.
const ALLOWED_EXTENSIONS: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
];

/// Validates uploaded files through a multi-step pipeline.
pub struct ValidationPipeline {
    allowed_content_types: Vec<String>,
}

impl ValidationPipeline {
    /// Creates a new validation pipeline with the given allowed content types.
    #[must_use]
    pub fn new(allowed_content_types: Vec<String>) -> Self {
        Self {
            allowed_content_types,
        }
    }

    /// Runs the full validation pipeline on a file.
    pub fn validate(
        &self,
        filename: &str,
        declared_content_type: &str,
        data: Vec<u8>,
    ) -> Result<ValidatedFile, ValidationError> {
        // Step 1: Extension allowlist
        let extension = self.check_extension(filename)?;

        // Step 2: Content-type allowlist
        self.check_content_type(declared_content_type)?;

        // Step 3: Magic bytes detection
        let detected_type = self.detect_magic_type(&data)?;

        // Step 4: Extension-to-magic consistency
        self.check_consistency(&extension, &detected_type)?;

        // Step 5: SVG sanitization if applicable
        let clean_data = if detected_type == "image/svg+xml" {
            sanitize_svg(&data)?
        } else {
            data
        };

        Ok(ValidatedFile {
            content_type: detected_type,
            extension,
            clean_data,
        })
    }

    /// Checks that the file extension is in the allowlist.
    fn check_extension(&self, filename: &str) -> Result<String, ValidationError> {
        let extension = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();

        if ALLOWED_EXTENSIONS.iter().any(|(ext, _)| *ext == extension) {
            Ok(extension)
        } else {
            Err(ValidationError::DisallowedExtension(extension))
        }
    }

    /// Checks that the declared content type is in the allowlist.
    fn check_content_type(&self, content_type: &str) -> Result<(), ValidationError> {
        // Normalize by taking only the media type (before any parameters)
        let media_type = content_type
            .split(';')
            .next()
            .unwrap_or(content_type)
            .trim()
            .to_ascii_lowercase();

        if self.allowed_content_types.iter().any(|t| t == &media_type) {
            Ok(())
        } else {
            Err(ValidationError::DisallowedContentType(
                media_type.to_string(),
            ))
        }
    }

    /// Detects the file type from magic bytes.
    fn detect_magic_type(&self, data: &[u8]) -> Result<String, ValidationError> {
        // PNG: 89 50 4E 47
        if data.len() >= 4 && data[..4] == [0x89, 0x50, 0x4E, 0x47] {
            return Ok("image/png".to_string());
        }

        // JPEG: FF D8 FF
        if data.len() >= 3 && data[..3] == [0xFF, 0xD8, 0xFF] {
            return Ok("image/jpeg".to_string());
        }

        // GIF: GIF8
        if data.len() >= 4 && &data[..4] == b"GIF8" {
            return Ok("image/gif".to_string());
        }

        // WebP: RIFF....WEBP
        if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
            return Ok("image/webp".to_string());
        }

        // SVG: starts with <?xml or <svg (possibly with leading whitespace/BOM)
        if is_svg(data) {
            return Ok("image/svg+xml".to_string());
        }

        Err(ValidationError::MagicMismatch)
    }

    /// Verifies that the detected content type is consistent with the file extension.
    fn check_consistency(
        &self,
        extension: &str,
        detected_type: &str,
    ) -> Result<(), ValidationError> {
        let expected_type = ALLOWED_EXTENSIONS
            .iter()
            .find(|(ext, _)| *ext == extension)
            .map(|(_, mime)| *mime);

        match expected_type {
            Some(expected) if expected == detected_type => Ok(()),
            // Allow jpg/jpeg both mapping to image/jpeg
            Some("image/jpeg") if detected_type == "image/jpeg" => Ok(()),
            _ => Err(ValidationError::ExtensionContentMismatch),
        }
    }
}

/// Checks if the data looks like an SVG file.
fn is_svg(data: &[u8]) -> bool {
    // Skip BOM if present
    let text = if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &data[3..]
    } else {
        data
    };

    let s = match std::str::from_utf8(text) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let trimmed = s.trim_start();
    trimmed.starts_with("<?xml") || trimmed.starts_with("<svg")
}

/// Sanitizes an SVG by removing dangerous elements and attributes.
///
/// This uses a simple string-based approach:
/// - Removes `<script>...</script>` blocks
/// - Removes `on*="..."` event handler attributes
/// - Removes `javascript:` URLs
/// - Removes `<foreignObject>...</foreignObject>` blocks
/// - Removes `data:` URLs in href/src attributes
fn sanitize_svg(data: &[u8]) -> Result<Vec<u8>, ValidationError> {
    let input = std::str::from_utf8(data).map_err(|_| ValidationError::UnsafeSvg)?;

    let mut result = input.to_string();

    // Remove <script>...</script> blocks (case-insensitive, non-greedy)
    result = remove_tag_blocks(&result, "script");

    // Remove <foreignObject>...</foreignObject> blocks
    result = remove_tag_blocks(&result, "foreignObject");
    result = remove_tag_blocks(&result, "foreignobject");

    // Remove on*="..." event handler attributes
    result = remove_event_handlers(&result);

    // Remove javascript: URLs
    result = remove_javascript_urls(&result);

    // Remove data: URLs in href and src attributes
    result = remove_data_urls(&result);

    Ok(result.into_bytes())
}

/// Removes all occurrences of `<tag ...>...</tag>` from the input.
fn remove_tag_blocks(input: &str, tag: &str) -> String {
    let mut result = input.to_string();
    let open_lower = format!("<{}", tag.to_ascii_lowercase());
    let close_lower = format!("</{}>", tag.to_ascii_lowercase());

    loop {
        let lower = result.to_ascii_lowercase();
        let Some(start) = lower.find(&open_lower) else {
            break;
        };
        let Some(end_offset) = lower[start..].find(&close_lower) else {
            // Malformed: remove from start tag to end of string
            result.truncate(start);
            break;
        };
        let end = start + end_offset + close_lower.len();
        result.replace_range(start..end, "");
    }

    result
}

/// Removes `on*="..."` and `on*='...'` event handler attributes.
fn remove_event_handlers(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Look for on*= pattern inside tags
        if i + 3 < len
            && (chars[i] == 'o' || chars[i] == 'O')
            && (chars[i + 1] == 'n' || chars[i + 1] == 'N')
            && chars[i + 2].is_alphabetic()
        {
            // Check if we're inside a tag (look back for '<' without '>')
            let in_tag = {
                let mut j = i;
                let mut found = false;
                while j > 0 {
                    j -= 1;
                    if chars[j] == '>' {
                        break;
                    }
                    if chars[j] == '<' {
                        found = true;
                        break;
                    }
                }
                found
            };

            if in_tag {
                // Find the '=' sign
                let mut k = i + 2;
                while k < len && chars[k].is_alphanumeric() {
                    k += 1;
                }
                // Skip whitespace
                while k < len && chars[k].is_whitespace() {
                    k += 1;
                }
                if k < len && chars[k] == '=' {
                    k += 1;
                    // Skip whitespace after =
                    while k < len && chars[k].is_whitespace() {
                        k += 1;
                    }
                    // Skip quoted value
                    if k < len && (chars[k] == '"' || chars[k] == '\'') {
                        let quote = chars[k];
                        k += 1;
                        while k < len && chars[k] != quote {
                            k += 1;
                        }
                        if k < len {
                            k += 1; // skip closing quote
                        }
                    }
                    i = k;
                    continue;
                }
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Removes `javascript:` from attribute values.
fn remove_javascript_urls(input: &str) -> String {
    // Case-insensitive replacement
    let mut result = input.to_string();
    loop {
        let lower = result.to_ascii_lowercase();
        if let Some(pos) = lower.find("javascript:") {
            result.replace_range(pos..pos + 11, "");
        } else {
            break;
        }
    }
    result
}

/// Removes `data:` URLs found in href="..." and src="..." attribute values.
fn remove_data_urls(input: &str) -> String {
    let mut result = input.to_string();

    for attr in &["href", "src", "xlink:href"] {
        loop {
            let lower = result.to_ascii_lowercase();
            // Find attr="data:..." or attr='data:...'
            let pattern_dq = format!("{}=\"data:", attr);
            let pattern_sq = format!("{}='data:", attr);

            if let Some(pos) = lower.find(&pattern_dq) {
                // Replace the data: URL value with empty
                let value_start = pos + attr.len() + 2; // after ="
                if let Some(end) = result[value_start..].find('"') {
                    result.replace_range(value_start..value_start + end, "");
                } else {
                    break;
                }
            } else if let Some(pos) = lower.find(&pattern_sq) {
                let value_start = pos + attr.len() + 2; // after ='
                if let Some(end) = result[value_start..].find('\'') {
                    result.replace_range(value_start..value_start + end, "");
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipeline() -> ValidationPipeline {
        ValidationPipeline::new(vec![
            "image/png".to_string(),
            "image/jpeg".to_string(),
            "image/gif".to_string(),
            "image/webp".to_string(),
            "image/svg+xml".to_string(),
        ])
    }

    #[test]
    fn test_png_magic() {
        let p = pipeline();
        let data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let result = p.validate("test.png", "image/png", data);
        assert!(result.is_ok());
        let vf = result.unwrap();
        assert_eq!(vf.content_type, "image/png");
        assert_eq!(vf.extension, "png");
    }

    #[test]
    fn test_disallowed_extension() {
        let p = pipeline();
        let result = p.validate("test.exe", "application/octet-stream", vec![0; 10]);
        assert!(matches!(
            result,
            Err(ValidationError::DisallowedExtension(_))
        ));
    }

    #[test]
    fn test_svg_sanitization() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert('xss')</script><rect onclick="evil()" width="10"/></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let s = String::from_utf8(result).unwrap();
        assert!(!s.contains("<script>"));
        assert!(!s.contains("onclick"));
    }

    #[test]
    fn test_magic_mismatch() {
        let p = pipeline();
        // PNG header but jpg extension
        let data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let result = p.validate("test.jpg", "image/jpeg", data);
        assert!(matches!(
            result,
            Err(ValidationError::ExtensionContentMismatch)
        ));
    }
}
