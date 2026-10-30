//! Photos uploaded through the app (the advent calendar). The copy shown on
//! the pages is made upright from the EXIF orientation, resized and
//! re-encoded as JPEG **without metadata** (phone photos may carry the GPS
//! position); the original is kept as uploaded, next to it. Also the public
//! copy's card pictures ([`thumbnail`], WebP), and the book scan's grey copy
//! of a barcode photo ([`grey`]).
//!
//! CPU work: call these in `tokio::task::spawn_blocking`.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};

/// The longest side of the copies, in pixels.
const MAX_SIDE: u32 = 1600;

/// The biggest photo read: 10,000 px a side (a 48 MP phone photo is
/// 8,000 × 6,000) and 300 MB of memory, so a small file claiming a huge
/// picture (a "decompression bomb") is refused instead of filling the
/// NAS's memory.
fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(10_000);
    limits.max_image_height = Some(10_000);
    limits.max_alloc = Some(300 * 1024 * 1024);
    limits
}

/// A reader of `bytes` with [`limits`].
fn reader(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>, PhotoError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| PhotoError(format!("the photo couldn't be read ({e})")))?;
    reader.limits(limits());
    Ok(reader)
}
const JPEG_QUALITY: u8 = 82;
/// The public copy's WebP pictures: measured on the card pictures, as close
/// to the originals as JPEG at [`JPEG_QUALITY`] (DSSIM about 0.001) and a
/// third smaller.
const WEBP_QUALITY: f32 = 85.0;

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
        image::ImageError::Limits(_) => {
            "the photo is too big (at most 10,000 pixels a side)".to_string()
        }
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
    let mut decoder = reader(bytes)?.into_decoder().map_err(refused)?;
    let orientation = decoder.orientation().map_err(refused)?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(refused)?;
    img.apply_orientation(orientation);
    if img.width() > MAX_SIDE || img.height() > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, FilterType::Lanczos3);
    }
    encode(&img)
}

/// A grey copy of a photo, its long side at most `max_side` px, for a
/// barcode reader (the book scan). A JPEG (any phone photo) is decoded
/// straight at 1/2, 1/4 or 1/8 of its size, the smallest still at least
/// `max_side` (on the NAS a 50 MP photo takes 3 s and 60 MB so, 8 s and
/// 300 MB decoded whole); its EXIF orientation is left out (the reader
/// tries it turned). Other formats are decoded whole, within [`limits`].
pub fn grey(bytes: &[u8], max_side: u32) -> Result<image::GrayImage, PhotoError> {
    let img = match reduced_jpeg(bytes, max_side) {
        Some(img) => img?,
        None => reader(bytes)?.decode().map_err(refused)?,
    };
    let img = if img.width() > max_side || img.height() > max_side {
        img.resize(max_side, max_side, FilterType::Triangle)
    } else {
        img
    };
    Ok(img.to_luma8())
}

/// [`grey`]'s reduced decoding of a JPEG: `None` for another format (or a
/// JPEG `jpeg-decoder` can't reduce: CMYK, 12 bits), left to `image`.
fn reduced_jpeg(bytes: &[u8], max_side: u32) -> Option<Result<DynamicImage, PhotoError>> {
    use jpeg_decoder::{Decoder, PixelFormat};

    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let unreadable =
        |e: jpeg_decoder::Error| PhotoError(format!("the photo couldn't be read ({e})"));
    let mut decoder = Decoder::new(Cursor::new(bytes));
    if let Err(e) = decoder.read_info() {
        return Some(Err(unreadable(e)));
    }
    let info = decoder.info()?;
    if !matches!(info.pixel_format, PixelFormat::L8 | PixelFormat::RGB24) {
        return None;
    }
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if width > 10_000 || height > 10_000 {
        return Some(Err(PhotoError(
            "the photo is too big (at most 10,000 pixels a side)".to_string(),
        )));
    }

    let long = width.max(height);
    let factor = [8, 4, 2, 1]
        .into_iter()
        .find(|f| long / f >= max_side)
        .unwrap_or(1);
    let (w, h) = match decoder.scale((width / factor) as u16, (height / factor) as u16) {
        Ok(size) => (u32::from(size.0), u32::from(size.1)),
        Err(e) => return Some(Err(unreadable(e))),
    };
    let pixels = match decoder.decode() {
        Ok(pixels) => pixels,
        Err(e) => return Some(Err(unreadable(e))),
    };
    let wrong = || PhotoError("the photo couldn't be read".to_string());
    Some(match info.pixel_format {
        PixelFormat::L8 => image::GrayImage::from_raw(w, h, pixels)
            .map(DynamicImage::ImageLuma8)
            .ok_or_else(wrong),
        _ => image::RgbImage::from_raw(w, h, pixels)
            .map(DynamicImage::ImageRgb8)
            .ok_or_else(wrong),
    })
}

/// A copy at most `width` px wide, never enlarged, as lossy WebP with its
/// transparency (the Pokémon cards' corners): the public export's card
/// pictures.
pub fn thumbnail(bytes: &[u8], width: u32) -> Result<Vec<u8>, PhotoError> {
    let mut img = reader(bytes)?.decode().map_err(refused)?;
    if img.width() > width {
        img = img.resize(width, u32::MAX, FilterType::Lanczos3);
    }
    let (width, height) = (img.width(), img.height());
    let pixels;
    let encoder = if img.color().has_alpha() {
        pixels = img.to_rgba8().into_raw();
        webp::Encoder::from_rgba(&pixels, width, height)
    } else {
        pixels = img.to_rgb8().into_raw();
        webp::Encoder::from_rgb(&pixels, width, height)
    };
    let webp = encoder
        .encode_simple(false, WEBP_QUALITY)
        .map_err(|e| PhotoError(format!("the picture couldn't be written ({e:?})")))?;
    Ok(webp.to_vec())
}

/// A copy turned a quarter turn (clockwise or not), for a photo that came
/// out sideways (no EXIF orientation).
pub fn rotate(bytes: &[u8], clockwise: bool) -> Result<Vec<u8>, PhotoError> {
    let img = reader(bytes)?.decode().map_err(refused)?;
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

/// A small PNG of `width` × `height`, for tests (here and in the
/// collections' advent calendar tests).
#[cfg(any(test, feature = "test-support"))]
pub fn test_png(width: u32, height: u32) -> Vec<u8> {
    let img = DynamicImage::new_rgb8(width, height);
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).unwrap();
    out.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        test_png(width, height)
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
    fn thumbnails_are_webp_and_never_enlarged() {
        let copy = thumbnail(&png(1200, 1650), 600).unwrap();
        assert_eq!(image::guess_format(&copy).unwrap(), ImageFormat::WebP);
        assert_eq!(size(&copy), (600, 825));
        assert_eq!(size(&thumbnail(&png(421, 614), 600).unwrap()), (421, 614));
    }

    #[test]
    fn thumbnails_keep_the_transparency() {
        let mut img = image::RgbaImage::new(100, 140);
        img.put_pixel(50, 70, image::Rgba([255, 0, 0, 255]));
        let mut original = Cursor::new(Vec::new());
        img.write_to(&mut original, ImageFormat::Png).unwrap();

        let copy = image::load_from_memory(&thumbnail(original.get_ref(), 600).unwrap()).unwrap();
        assert!(copy.color().has_alpha());
        assert_eq!(
            copy.to_rgba8().get_pixel(0, 0)[3],
            0,
            "a corner stays transparent"
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

    /// A PNG's checksum (CRC-32), to forge a header.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    #[test]
    fn a_huge_picture_in_a_small_file_is_refused() {
        // a valid 1 × 1 PNG whose header (IHDR) then claims 50,000 × 50,000
        let mut bomb = png(1, 1);
        let ihdr = bomb.windows(4).position(|w| w == b"IHDR").unwrap();
        bomb[ihdr + 4..ihdr + 8].copy_from_slice(&50_000u32.to_be_bytes());
        bomb[ihdr + 8..ihdr + 12].copy_from_slice(&50_000u32.to_be_bytes());
        let crc = crc32(&bomb[ihdr..ihdr + 17]);
        bomb[ihdr + 17..ihdr + 21].copy_from_slice(&crc.to_be_bytes());

        let error = web_copy(&bomb).unwrap_err();
        assert!(error.0.contains("too big"), "{error}");
        assert!(rotate(&bomb, true).is_err());
    }

    #[test]
    fn grey_copies_reduced() {
        // a JPEG of 6400 × 4800: decoded at 1/4 (1600 × 1200), not whole
        let img = DynamicImage::new_rgb8(6400, 4800);
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 80)
            .encode_image(&img.to_rgb8())
            .unwrap();
        let grey_copy = grey(&jpeg, 1600).unwrap();
        assert_eq!((grey_copy.width(), grey_copy.height()), (1600, 1200));

        // another format, decoded whole; never enlarged
        let grey_copy = grey(&png(300, 200), 1600).unwrap();
        assert_eq!((grey_copy.width(), grey_copy.height()), (300, 200));
        let grey_copy = grey(&png(3000, 2000), 1600).unwrap();
        assert_eq!((grey_copy.width(), grey_copy.height()), (1600, 1067));

        assert!(grey(b"\xFF\xD8 not a jpeg", 1600).is_err());
        assert!(grey(b"not a photo", 1600).is_err());
    }
}
