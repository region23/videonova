//! Common utility functions used across the application.
//! This module contains general-purpose functions that are used by multiple other modules.

use std::path::Path;

/// Sanitize filename to be safe for all operating systems.
/// 
/// This function:
/// - Converts the filename to lowercase
/// - Replaces special characters with underscores
/// - Handles spaces and tabs
/// 
/// # Arguments
/// * `input` - The filename to sanitize
/// 
/// # Returns
/// A sanitized filename that is safe to use across different operating systems
/// 
/// # Examples
/// ```
/// use videonova::utils::common::sanitize_filename;
/// 
/// assert_eq!(sanitize_filename("Hello World"), "hello_world");
/// assert_eq!(sanitize_filename("File:Name?With*Special<Chars>"), "file_name_with_special_chars_");
/// ```
pub fn sanitize_filename(input: &str) -> String {
    let invalid_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', ' ', '\t'];
    let mut result = input.to_lowercase();
    for c in invalid_chars {
        result = result.replace(c, "_");
    }
    result
}

/// Check if a file exists and has valid content (non-zero size).
/// 
/// This function performs an asynchronous check of the file's existence and size.
/// It returns `true` only if the file exists, is a regular file (not a directory),
/// and has a size greater than 0 bytes.
/// 
/// # Arguments
/// * `path` - Path to the file to check
/// 
/// # Returns
/// `true` if the file exists and has content, `false` otherwise
/// 
/// # Examples
/// ```
/// use std::path::Path;
/// use videonova::utils::common::check_file_exists_and_valid;
/// 
/// // Assuming the file exists and has content
/// let path = Path::new("example.txt");
/// assert!(check_file_exists_and_valid(path).await);
/// ```
pub async fn check_file_exists_and_valid(path: &Path) -> bool {
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        if metadata.is_file() && metadata.len() > 0 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Hello World"), "hello_world");
        assert_eq!(sanitize_filename("File:Name?With*Special<Chars>"), "file_name_with_special_chars_");
        assert_eq!(sanitize_filename("UPPERCASE"), "uppercase");
        assert_eq!(sanitize_filename("path/to/file"), "path_to_file");
        assert_eq!(sanitize_filename("file name with\ttabs"), "file_name_with_tabs");
    }
} 