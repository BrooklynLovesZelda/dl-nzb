//! File name deobfuscation
//!
//! This module provides functionality to detect and rename obfuscated files
//! to more meaningful names based on the NZB name.

use super::file_extension;
use crate::error::{DlNzbError, PostProcessingError};
use std::fs;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, DlNzbError>;

/// Check if a filename looks obfuscated (random/meaningless)
fn is_probably_obfuscated(filename: &str) -> bool {
    // Remove extension for analysis
    let name_without_ext = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    // Check for patterns that suggest obfuscation
    let lowercase = name_without_ext.to_lowercase();

    // Too short to be meaningful
    if name_without_ext.len() < 5 {
        return true;
    }

    // Check for excessive special characters or numbers
    let special_chars = name_without_ext
        .chars()
        .filter(|c| !c.is_alphanumeric() && *c != ' ' && *c != '-' && *c != '_')
        .count();
    let digits = name_without_ext.chars().filter(|c| c.is_numeric()).count();
    let alpha = name_without_ext
        .chars()
        .filter(|c| c.is_alphabetic())
        .count();

    // More than 50% special chars or digits suggests obfuscation
    if special_chars > name_without_ext.len() / 2 {
        return true;
    }
    if digits > name_without_ext.len() / 2 && alpha < 3 {
        return true;
    }

    // Check for hex-like patterns (long strings of hex chars)
    let hex_chars = name_without_ext
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .count();
    if hex_chars > name_without_ext.len() * 3 / 4 && name_without_ext.len() > 8 {
        return true;
    }

    // Check for common obfuscation patterns
    if lowercase.starts_with("f7f8f9")
        || lowercase.contains("yenc")
        || lowercase.matches(char::is_numeric).count() > 10
    {
        return true;
    }

    // Check for lack of vowels (random consonant strings)
    let vowels = name_without_ext
        .chars()
        .filter(|c| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u'))
        .count();
    if alpha > 8 && vowels < alpha / 4 {
        return true;
    }

    false
}

/// Get the file extension including the dot
fn get_ext(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s))
        .unwrap_or_default()
}

/// Get the base name without extension
fn get_basename(path: &Path) -> PathBuf {
    path.with_extension("")
}

/// Get file size in bytes
fn get_file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Find the biggest file in the list
fn get_biggest_file(files: &[PathBuf]) -> Option<(PathBuf, u64)> {
    files
        .iter()
        .map(|f| (f.clone(), get_file_size(f)))
        .max_by_key(|(_, size)| *size)
}

/// Generate a unique filename by appending numbers if needed
fn get_unique_filename(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    for i in 1..1000 {
        let new_name = if ext.is_empty() {
            format!("{}_{}", stem, i)
        } else {
            format!("{}_{}.{}", stem, i, ext)
        };
        let new_path = parent.join(new_name);
        if !new_path.exists() {
            return new_path;
        }
    }

    path.to_path_buf()
}

/// Rename a file, returning the new path
fn rename_file(old_path: &Path, new_path: &Path) -> Result<PathBuf> {
    fs::rename(old_path, new_path).map_err(|e| PostProcessingError::FileRenameError {
        from: old_path.to_path_buf(),
        to: new_path.to_path_buf(),
        source: e,
    })?;
    Ok(new_path.to_path_buf())
}

/// Sanitize a name to be filesystem-safe
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

pub struct DeobfuscateResult {
    pub files_renamed: usize,
    pub extensions_fixed: usize,
}

/// Deobfuscate files in a directory
///
/// This function:
/// 1. Adds missing extensions to files based on magic bytes
/// 2. Renames the largest obfuscated file to a meaningful name
/// 3. Renames related files (same basename) to match
pub fn deobfuscate_files(directory: &Path, useful_name: &str) -> Result<DeobfuscateResult> {
    let mut files_renamed = 0;
    let mut extensions_fixed = 0;

    // Get all files in directory (not recursively)
    let mut file_list: Vec<PathBuf> = fs::read_dir(directory)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();

    if file_list.is_empty() {
        return Ok(DeobfuscateResult {
            files_renamed: 0,
            extensions_fixed: 0,
        });
    }

    // Check for DVD/Bluray directories - skip deobfuscation if found
    for file in &file_list {
        if let Some(parent) = file.parent() {
            let parent_str = parent.to_string_lossy();
            for ignored in file_extension::IGNORED_MOVIE_FOLDERS {
                if parent_str.contains(&format!("{}/", ignored))
                    || parent_str.contains(&format!("\\{}", ignored))
                {
                    tracing::debug!(
                        "Skipping deobfuscation due to DVD/Bluray directory: {}",
                        parent_str
                    );
                    return Ok(DeobfuscateResult {
                        files_renamed: 0,
                        extensions_fixed: 0,
                    });
                }
            }
        }
    }

    // Step 1: Fix missing extensions
    let mut new_file_list = Vec::new();
    for file in &file_list {
        if file_extension::has_popular_extension(file) {
            // Extension looks fine
            new_file_list.push(file.clone());
        } else if let Some(new_ext) = file_extension::what_is_most_likely_extension(file) {
            // Detected file type - add extension
            let new_path = file.with_extension(&new_ext[1..]); // Remove leading dot
            let new_path = get_unique_filename(&new_path);

            tracing::debug!(
                "Adding extension: {} -> {}",
                file.display(),
                new_path.display()
            );
            match rename_file(file, &new_path) {
                Ok(renamed) => {
                    new_file_list.push(renamed);
                    extensions_fixed += 1;
                }
                Err(e) => {
                    tracing::debug!("Failed to rename {}: {}", file.display(), e);
                    new_file_list.push(file.clone());
                }
            }
        } else {
            new_file_list.push(file.clone());
        }
    }
    file_list = new_file_list;

    // Step 2: Find biggest file and check if it needs deobfuscation
    let Some((biggest_file, biggest_size)) = get_biggest_file(&file_list) else {
        return Ok(DeobfuscateResult {
            files_renamed,
            extensions_fixed,
        });
    };

    // Check if biggest file should be excluded
    let ext = get_ext(&biggest_file);
    if file_extension::EXCLUDED_FILE_EXTS.contains(&ext.as_str()) {
        tracing::debug!(
            "Biggest file {} excluded due to extension",
            biggest_file.display()
        );
        return Ok(DeobfuscateResult {
            files_renamed,
            extensions_fixed,
        });
    }

    // Check if filename looks obfuscated
    let filename = biggest_file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if !is_probably_obfuscated(filename) {
        tracing::debug!(
            "Biggest file {} doesn't look obfuscated",
            biggest_file.display()
        );
        return Ok(DeobfuscateResult {
            files_renamed,
            extensions_fixed,
        });
    }

    // Check if it's significantly bigger than the second biggest file
    let second_biggest_size = file_list
        .iter()
        .filter(|f| *f != &biggest_file)
        .map(|f| get_file_size(f))
        .max()
        .unwrap_or(0);

    // Only rename if biggest is at least 1.5x bigger than second biggest
    if second_biggest_size > 0 && biggest_size < second_biggest_size * 3 / 2 {
        tracing::debug!(
            "Biggest file ({} bytes) not significantly larger than second biggest ({} bytes)",
            biggest_size,
            second_biggest_size
        );
        return Ok(DeobfuscateResult {
            files_renamed,
            extensions_fixed,
        });
    }

    // Step 3: Rename the biggest file
    let sanitized_name = sanitize_name(useful_name);
    let new_name = format!("{}{}", sanitized_name, ext);
    let new_path = biggest_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&new_name);
    let new_path = get_unique_filename(&new_path);

    tracing::debug!(
        "Deobfuscating: {} -> {}",
        biggest_file.display(),
        new_path.display()
    );

    match rename_file(&biggest_file, &new_path) {
        Ok(_) => {
            files_renamed += 1;
        }
        Err(e) => {
            tracing::debug!("Failed to rename {}: {}", biggest_file.display(), e);
            return Ok(DeobfuscateResult {
                files_renamed,
                extensions_fixed,
            });
        }
    }

    // Step 4: Find and rename related files (same basename)
    let basename = get_basename(&biggest_file);
    let basename_str = basename.to_string_lossy();

    for file in &file_list {
        if *file == biggest_file {
            continue;
        }

        let file_basename = get_basename(file);
        let file_basename_str = file_basename.to_string_lossy();

        // Check if this file shares the same basename
        if file_basename_str == basename_str {
            let remaining = file
                .to_string_lossy()
                .replace(&basename_str.to_string(), "");

            let new_name = format!("{}{}", sanitized_name, remaining);
            let new_path = file
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&new_name);
            let new_path = get_unique_filename(&new_path);

            tracing::debug!(
                "Deobfuscating related: {} -> {}",
                file.display(),
                new_path.display()
            );

            match rename_file(file, &new_path) {
                Ok(_) => files_renamed += 1,
                Err(e) => tracing::debug!("Failed to rename {}: {}", file.display(), e),
            }
        }
    }

    Ok(DeobfuscateResult {
        files_renamed,
        extensions_fixed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_obfuscated() {
        assert!(is_probably_obfuscated("f7f8f9abc123.mkv"));
        assert!(is_probably_obfuscated("a1b2c3d4e5f6.iso"));
        assert!(is_probably_obfuscated("xkcd.tmp"));
        assert!(!is_probably_obfuscated("Great_Movie_2023.mkv"));
        assert!(!is_probably_obfuscated("My.Document.pdf"));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("File/Name:Test"), "File_Name_Test");
        assert_eq!(sanitize_name("Normal_File-123"), "Normal_File-123");
    }
}
