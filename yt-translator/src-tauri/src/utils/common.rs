//! Common utility functions used across the application

/// Sanitize filename to be safe for all operating systems.
/// Converts the filename to lowercase and replaces special characters with underscores.
/// 
/// # Arguments
/// * `input` - The filename to sanitize
/// 
/// # Returns
/// * A sanitized filename (lowercase with special characters replaced)
pub fn sanitize_filename(input: &str) -> String {
    let invalid_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', ' ', '\t'];
    let mut result = input.to_lowercase(); // Преобразуем в нижний регистр
    for c in invalid_chars {
        result = result.replace(c, "_");
    }
    result
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