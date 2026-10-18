//! Photos uploaded through the app (the advent calendar). The copy shown on
//! the pages is made upright from the EXIF orientation, resized and
//! re-encoded as JPEG **without metadata** (phone photos may carry the GPS
//! position); the original is kept as uploaded, next to it.
//!
//! CPU work: call these in `tokio::task::spawn_blocking`.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

/// The longest side of the copies, in pixels.
const MAX_SIDE: u32 = 1600;
const JPEG_QUALITY: u8 = 82;

/// Why a photo was refused, as shown to the user.
#[derive(Debug, PartialEq, Eq)]
pub struct PhotoError(pub String);

impl std::fmt::Display for PhotoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

const UNSUPPORTED: &str = "this image format isn't supported: use JPEG, PNG or WebP \
     (an iPhone HEIC photo can be exported as JPEG)";

fn refused(e: image::ImageError) -> PhotoError {
    PhotoError(match e {
        image::ImageError::Unsupported(_) => UNSUPPORTED.to_string(),
        e => format!("the photo couldn't be read ({e})"),
    })
}

/// The extension to keep the original under (`"jpg"`, `"png"`, `"webp"`).
pub fn extension(bytes: &[u8]) -> Result<&'static str, PhotoError> {
    match image::guess_format(bytes).map_err(refused)? {
        ImageFormat::Jpeg => Ok("jpg"),
        ImageFormat::Png => Ok("png"),
        ImageFormat::WebP => Ok("webp"),
        _ => Err(PhotoError(UNSUPPORTED.to_string())),
    }
}

/// The copy of an uploaded photo: upright, at most [`MAX_SIDE`] px, JPEG,
/// no metadata.
pub fn web_copy(bytes: &[u8]) -> Result<Vec<u8>, PhotoError> {
    let mut decoder = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| PhotoError(format!("the photo couldn't be read ({e})")))?
        .into_decoder()
        .map_err(refused)?;
    let orientation = decoder.orientation().map_err(refused)?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(refused)?;
    img.apply_orientation(orientation);
    if img.width() > MAX_SIDE || img.height() > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, FilterType::Lanczos3);
    }
    encode(&img)
}

/// A copy turned a quarter turn (clockwise or not), for a photo that came
/// out sideways (no EXIF orientation).
pub fn rotate(bytes: &[u8], clockwise: bool) -> Result<Vec<u8>, PhotoError> {
    let img = image::load_from_memory(bytes).map_err(refused)?;
    encode(&if clockwise {
        img.rotate90()
    } else {
        img.rotate270()
    })
}

fn encode(img: &DynamicImage) -> Result<Vec<u8>, PhotoError> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&img.to_rgb8())
        .map_err(refused)?;
    Ok(out)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A small PNG of `width` × `height`, for tests.
    pub fn png(width: u32, height: u32) -> Vec<u8> {
        let img = DynamicImage::new_rgb8(width, height);
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn size(jpeg: &[u8]) -> (u32, u32) {
        let img = image::load_from_memory(jpeg).unwrap();
        (img.width(), img.height())
    }

    #[test]
    fn copies_are_jpeg_and_at_most_1600_px() {
        let copy = web_copy(&png(1800, 900)).unwrap();
        assert_eq!(image::guess_format(&copy).unwrap(), ImageFormat::Jpeg);
        assert_eq!(size(&copy), (1600, 800));
        assert_eq!(
            size(&web_copy(&png(800, 600)).unwrap()),
            (800, 600),
            "not enlarged"
        );
    }

    #[test]
    fn rotating_swaps_the_sides() {
        let copy = web_copy(&png(400, 300)).unwrap();
        assert_eq!(size(&rotate(&copy, true).unwrap()), (300, 400));
        assert_eq!(size(&rotate(&copy, false).unwrap()), (300, 400));
    }

    #[test]
    fn extensions() {
        assert_eq!(extension(&png(2, 2)), Ok("png"));
        assert_eq!(extension(&web_copy(&png(2, 2)).unwrap()), Ok("jpg"));
        assert!(extension(b"not an image").is_err());
    }

    #[test]
    fn not_an_image_is_refused() {
        assert!(web_copy(b"not an image").is_err());
    }
}
