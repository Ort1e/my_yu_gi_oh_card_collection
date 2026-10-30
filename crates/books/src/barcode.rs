//! The ISBN on a photo of a book's barcode (the book form's scan,
//! `routes::api::scan`), read here on the NAS: the phone only sends the
//! photo. Read in the phone's browser, a whole camera photo took more memory
//! than some phones have. CPU work: call it in `spawn_blocking` (2 to 3 s on
//! the NAS for a phone photo, measured 2026-10-30).

use books_domain::isbn::Isbn;
use common::photos::{self, PhotoError};
use image::imageops::{self, FilterType};
use image::GrayImage;
use rxing::BarcodeFormat;

/// The grey copy read first: a far barcode needs its pixels.
const BIG: u32 = 1600;
/// Then a smaller one: a blurry barcode is sharper smaller.
const SMALL: u32 = 800;

/// The ISBN of the photo's book barcode (EAN-13, 978/979), `None` when none
/// is found; an error when `bytes` isn't a photo. Tries the big then the
/// small copy, each as taken and turned a quarter turn (a barcode upside
/// down reads as it is).
pub fn read_isbn(bytes: &[u8]) -> Result<Option<Isbn>, PhotoError> {
    let big = photos::grey(bytes, BIG)?;
    let scale = f64::from(SMALL) / f64::from(big.width().max(big.height()));
    let small = if scale < 1.0 {
        let width = (f64::from(big.width()) * scale).round().max(1.0) as u32;
        let height = (f64::from(big.height()) * scale).round().max(1.0) as u32;
        Some(imageops::resize(&big, width, height, FilterType::Triangle))
    } else {
        None
    };

    for picture in std::iter::once(&big).chain(small.as_ref()) {
        for turned in [None, Some(imageops::rotate90(picture))] {
            if let Some(isbn) = read(turned.as_ref().unwrap_or(picture)) {
                return Ok(Some(isbn));
            }
        }
    }
    Ok(None)
}

/// A book's ISBN in the barcode of `img`, if it has one.
fn read(img: &GrayImage) -> Option<Isbn> {
    let found = rxing::helpers::detect_in_luma_slice(
        img.as_raw(),
        img.width(),
        img.height(),
        Some(BarcodeFormat::EAN_13),
    )
    .ok()?;
    Isbn::parse(found.getText()).ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat, Luma};
    use rxing::{MultiFormatWriter, Writer};

    use super::*;

    /// A photo-like picture (`width` × `height`, grey) with the EAN-13
    /// barcode of `code` in its middle, a quarter of its width, as `format`.
    pub(crate) fn barcode_photo(
        code: &str,
        width: u32,
        height: u32,
        format: ImageFormat,
    ) -> Vec<u8> {
        let mut img = GrayImage::from_pixel(width, height, Luma([150]));
        let bars = MultiFormatWriter
            .encode(
                code,
                &BarcodeFormat::EAN_13,
                (width / 4) as i32,
                (width / 8) as i32,
            )
            .unwrap();
        let (x0, y0) = (
            (width - bars.getWidth()) / 2,
            (height - bars.getHeight()) / 2,
        );
        for y in 0..bars.getHeight() {
            for x in 0..bars.getWidth() {
                let shade = if bars.get(x, y) { 20 } else { 235 };
                img.put_pixel(x0 + x, y0 + y, Luma([shade]));
            }
        }
        let mut out = Vec::new();
        DynamicImage::ImageLuma8(img)
            .write_to(&mut Cursor::new(&mut out), format)
            .unwrap();
        out
    }

    fn turned(photo: &[u8]) -> Vec<u8> {
        let img = image::load_from_memory(photo).unwrap().rotate90();
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn reads_the_isbn_of_a_barcode_photo() {
        let isbn = Isbn::parse("9780553573398").unwrap();
        // a big JPEG (decoded reduced), a small PNG, one turned sideways
        let jpeg = barcode_photo("9780553573398", 4000, 3000, ImageFormat::Jpeg);
        assert_eq!(read_isbn(&jpeg).unwrap(), Some(isbn.clone()));
        let png = barcode_photo("9780553573398", 1200, 900, ImageFormat::Png);
        assert_eq!(read_isbn(&png).unwrap(), Some(isbn.clone()));
        assert_eq!(read_isbn(&turned(&png)).unwrap(), Some(isbn));
    }

    #[test]
    fn no_isbn_without_a_book_barcode() {
        let blank = barcode_photo("9780553573398", 1200, 900, ImageFormat::Png);
        let blank = image::load_from_memory(&blank)
            .unwrap()
            .crop_imm(0, 0, 200, 200);
        let mut png = Vec::new();
        blank
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        assert_eq!(read_isbn(&png).unwrap(), None);
        // an EAN-13 that isn't a book's (not 978/979)
        let food = barcode_photo("3017620422003", 1200, 900, ImageFormat::Png);
        assert_eq!(read_isbn(&food).unwrap(), None);
        assert!(read_isbn(b"not a photo").is_err());
    }
}
