//! Find images and pair them with a reference of the same name.
//!
//! A reference folder is matched in this order:
//!
//! 1. The same relative path, including the extension (`enhanced/dive/a.png`
//!    looks for `original/dive/a.png`).
//! 2. The same relative directory and the same file stem, with any supported
//!    extension (`a.png` can match `a.jpg`).
//!
//! Matching is case-sensitive, which is the filesystem rule on Linux. Two
//! single files are compared as a pair even when their names differ.

use std::fs;
use std::path::{Path, PathBuf};

use image::RgbImage;

use crate::error::{Error, Result};

/// Extensions [`load`] will try. Matching ignores case.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp", "gif", "tga",
];

/// One image to evaluate, and the reference it was matched with, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// Image under test.
    pub input: PathBuf,
    /// Matched reference, or `None` when this file is scored on its own.
    pub reference: Option<PathBuf>,
    /// Why a reference folder did not produce a match. Empty when no
    /// reference was requested, or when a match was found.
    pub warning: Option<String>,
}

/// Decode a file as 8-bit RGB.
///
/// JPEG, PNG, WebP, TIFF, BMP, GIF, and TGA are enabled. GIF uses the first
/// frame. Other color types are converted to RGB by the `image` crate.
pub fn load(path: &Path) -> Result<RgbImage> {
    image::open(path)
        .map(|image| image.into_rgb8())
        .map_err(|source| Error::Image {
            path: path.to_path_buf(),
            source,
        })
}

/// `true` when the extension is one this crate decodes.
pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// Build the work list for a file or a folder.
///
/// `recursive` walks subfolders of `input`. A reference folder is searched
/// at the matching relative path; it is not scanned independently. Passing a
/// folder as `input` and a file as `reference` is an error, because there is
/// no file name to match against.
pub fn discover(input: &Path, reference: Option<&Path>, recursive: bool) -> Result<Vec<Job>> {
    if !input.exists() {
        return Err(Error::NotFound(input.to_path_buf()));
    }
    if let Some(reference) = reference
        && !reference.exists()
    {
        return Err(Error::NotFound(reference.to_path_buf()));
    }

    if input.is_file() {
        if !is_image(input) {
            return Err(Error::NoImages(input.to_path_buf()));
        }
        let reference = match reference {
            None => None,
            Some(path) if path.is_file() => Some(path.to_path_buf()),
            Some(dir) => Some(find_named(dir, input, recursive).ok_or_else(|| {
                Error::ReferenceNotFound {
                    name: input
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| input.display().to_string()),
                    directory: dir.to_path_buf(),
                }
            })?),
        };
        return Ok(vec![Job {
            input: input.to_path_buf(),
            reference,
            warning: None,
        }]);
    }

    if let Some(reference) = reference
        && reference.is_file()
    {
        return Err(Error::ReferenceIsFile);
    }

    let mut images = Vec::new();
    collect_images(input, recursive, &mut images)?;
    images.sort();
    if images.is_empty() {
        return Err(Error::NoImages(input.to_path_buf()));
    }

    let mut jobs = Vec::with_capacity(images.len());
    for image in images {
        let (reference_path, warning) = match reference {
            None => (None, None),
            Some(root) => match find_match(input, &image, root) {
                Some(path) => (Some(path), None),
                None => (
                    None,
                    Some(format!(
                        "no reference with the same name as {}",
                        image.strip_prefix(input).unwrap_or(&image).display()
                    )),
                ),
            },
        };
        jobs.push(Job {
            input: image,
            reference: reference_path,
            warning,
        });
    }
    Ok(jobs)
}

fn collect_images(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if recursive {
                collect_images(&path, true, out)?;
            }
            continue;
        }
        if path.is_file() && is_image(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn find_match(input_root: &Path, input_file: &Path, reference_root: &Path) -> Option<PathBuf> {
    let relative = input_file.strip_prefix(input_root).ok()?;
    let direct = reference_root.join(relative);
    if direct.is_file() {
        return Some(direct);
    }
    let parent = relative.parent().unwrap_or(Path::new(""));
    let stem = input_file.file_stem()?;
    let dir = reference_root.join(parent);
    first_stem_match(&dir, stem)
}

fn find_named(reference_root: &Path, input_file: &Path, recursive: bool) -> Option<PathBuf> {
    let name = input_file.file_name()?;
    let direct = reference_root.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let stem = input_file.file_stem()?;
    if let Some(found) = first_stem_match(reference_root, stem) {
        return Some(found);
    }
    if !recursive {
        return None;
    }
    let mut stack = vec![reference_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name() == Some(name) && path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn first_stem_match(dir: &Path, stem: &std::ffi::OsStr) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_stem() == Some(stem) && is_image(&path) {
            matches.push(path);
        }
    }
    matches.sort();
    matches.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn folder_match_accepts_a_different_extension() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("oceanmaster-match-{stamp}"));
        let input = root.join("in");
        let reference = root.join("ref");
        fs::create_dir_all(&input).unwrap();
        fs::create_dir_all(&reference).unwrap();
        let pixels = RgbImage::from_pixel(2, 2, Rgb([1, 2, 3]));
        pixels.save(input.join("frame.png")).unwrap();
        pixels.save(reference.join("frame.jpg")).unwrap();

        let jobs = discover(&input, Some(&reference), false).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            jobs[0].reference.as_deref(),
            Some(reference.join("frame.jpg").as_path())
        );
        assert!(jobs[0].warning.is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
