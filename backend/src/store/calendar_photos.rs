//! The photo files of the advent calendars' days (cards and books): one
//! folder per kind of calendar (`advent_calendar_photos`,
//! `book_advent_calendar_photos`), then one per year.
//!
//! `<folder>/<year>/03-ab12cd34.jpg` is the copy shown on the pages (upright,
//! resized, no metadata: `store::photos`), `<folder>/<year>/originals/03-ab12cd34.png`
//! the photo as uploaded. Each save writes new file names, so browsers never
//! show an old photo from their cache, and deletes the previous files.

use std::path::{Path, PathBuf};

use crate::store::photos::{self, PhotoError};
use crate::web::error::{AppError, AppResult};

/// An uploaded photo, checked and copied (CPU work done off the async
/// threads), not written yet.
pub struct Processed {
    original: Vec<u8>,
    extension: &'static str,
    copy: Vec<u8>,
}

/// Check an upload and make its web copy. `Ok(Err(why))`: the photo is
/// refused, `why` is for the user.
pub async fn process(upload: Vec<u8>) -> AppResult<Result<Processed, PhotoError>> {
    tokio::task::spawn_blocking(move || {
        let extension = photos::extension(&upload)?;
        let copy = photos::web_copy(&upload)?;
        Ok(Processed {
            original: upload,
            extension,
            copy,
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("processing a photo: {e}")))
}

/// A photo written to disk: its copy (relative to the media dir, as stored)
/// and every file written (to delete if saving fails).
pub struct NewPhoto {
    pub relative: String,
    pub files: Vec<PathBuf>,
}

/// Write the copy and the original of a processed photo for a day.
pub async fn write(
    media: &Path,
    folder: &str,
    year: i64,
    day: i64,
    photo: &Processed,
) -> AppResult<NewPhoto> {
    write_files(
        media,
        folder,
        year,
        day,
        &photo.copy,
        Some((&photo.original, photo.extension)),
    )
    .await
}

async fn write_files(
    media: &Path,
    folder: &str,
    year: i64,
    day: i64,
    copy: &[u8],
    original: Option<(&[u8], &str)>,
) -> AppResult<NewPhoto> {
    let io = |e: std::io::Error| AppError::Internal(format!("saving a photo: {e}"));
    let name = format!("{day:02}-{:08x}", rand::random::<u32>());
    let dir = media.join(folder).join(year.to_string());
    tokio::fs::create_dir_all(dir.join("originals"))
        .await
        .map_err(io)?;

    let copy_path = dir.join(format!("{name}.jpg"));
    tokio::fs::write(&copy_path, copy).await.map_err(io)?;
    let mut files = vec![copy_path];
    if let Some((bytes, extension)) = original {
        let path = dir.join("originals").join(format!("{name}.{extension}"));
        if let Err(e) = tokio::fs::write(&path, bytes).await {
            remove(&files).await;
            return Err(io(e));
        }
        files.push(path);
    }

    Ok(NewPhoto {
        relative: format!("{folder}/{year}/{name}.jpg"),
        files,
    })
}

/// The files of a photo: its copy (relative to the media dir) and the
/// originals with the same name.
pub fn files(media: &Path, copy: &str) -> Vec<PathBuf> {
    let copy_path = media.join(copy);
    let mut files = vec![copy_path.clone()];
    if let (Some(stem), Some(dir)) = (copy_path.file_stem(), copy_path.parent()) {
        if let Ok(originals) = std::fs::read_dir(dir.join("originals")) {
            files.extend(
                originals
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.file_stem() == Some(stem)),
            );
        }
    }
    files
}

/// Delete files, ignoring the ones already gone.
pub async fn remove(files: &[PathBuf]) {
    for file in files {
        let _ = tokio::fs::remove_file(file).await;
    }
}

/// Turn a day's photo (`old`, relative) a quarter turn: a new copy, the
/// originals renamed with it (they stay as uploaded). Returns the new copy;
/// the caller stores it, then deletes the old copy with [`remove`].
pub async fn rotate(
    media: &Path,
    folder: &str,
    year: i64,
    day: i64,
    old: &str,
    clockwise: bool,
) -> AppResult<String> {
    let bytes = tokio::fs::read(media.join(old))
        .await
        .map_err(|e| AppError::Internal(format!("reading a photo: {e}")))?;
    let turned = tokio::task::spawn_blocking(move || photos::rotate(&bytes, clockwise))
        .await
        .map_err(|e| AppError::Internal(format!("rotating a photo: {e}")))?
        .map_err(|e| AppError::Internal(format!("rotating a photo: {e}")))?;

    // the new copy takes the name of the original, so they stay paired
    let new = write_files(media, folder, year, day, &turned, None).await?;
    let old_path = media.join(old);
    let new_path = media.join(&new.relative);
    let originals = files(media, old).into_iter().filter(|p| p != &old_path);
    for original in originals {
        if let (Some(ext), Some(stem)) = (original.extension(), new_path.file_stem()) {
            let renamed = original.with_file_name(stem).with_extension(ext);
            let _ = tokio::fs::rename(&original, renamed).await;
        }
    }

    Ok(new.relative)
}
