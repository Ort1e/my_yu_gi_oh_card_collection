//! The day form of the advent calendars, cards (`routes::advent`) and books
//! (`routes::books::advent`): a field naming the day's item and a photo,
//! posted as multipart to `…/:year/days/:day`.

use axum::extract::multipart::MultipartError;
use axum::extract::Multipart;
use axum::response::{IntoResponse, Redirect, Response};

use crate::calendar_photos::{self, Processed};
use crate::web::error::{AppError, AppResult};

/// A day's form as posted.
pub struct DayForm {
    /// The item field (`card`, `book`), trimmed; empty keeps the current one.
    pub item: String,
    /// The photo's bytes, empty when none was chosen.
    pub photo: Vec<u8>,
}

impl DayForm {
    /// Read the form, its item from the field `item_field`.
    pub async fn read(mut multipart: Multipart, item_field: &str) -> AppResult<DayForm> {
        let read = |e: MultipartError| AppError::Internal(format!("reading the form: {e}"));
        let mut form = DayForm {
            item: String::new(),
            photo: Vec::new(),
        };

        while let Some(field) = multipart.next_field().await.map_err(read)? {
            match field.name() {
                Some(name) if name == item_field => {
                    form.item = field.text().await.map_err(read)?.trim().to_string();
                }
                Some("photo") => form.photo = field.bytes().await.map_err(read)?.to_vec(),
                _ => {}
            }
        }
        Ok(form)
    }

    /// The photo, processed (upright, resized, without metadata), `None`
    /// without one. A photo that can't be read adds an error.
    pub async fn photo(
        &mut self,
        day: i64,
        errors: &mut Vec<String>,
    ) -> AppResult<Option<Processed>> {
        if self.photo.is_empty() {
            return Ok(None);
        }

        match calendar_photos::process(std::mem::take(&mut self.photo)).await? {
            Ok(processed) => Ok(Some(processed)),
            Err(why) => {
                errors.push(format!("Day {day}: {why}."));
                Ok(None)
            }
        }
    }
}

/// A calendar has days 1 to 25 (the 25th: Christmas); others are not found.
pub fn check_day(day: i64) -> AppResult<()> {
    if (1..=25).contains(&day) {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// Back to the day on the calendar's edit page (`base`: `/advent-calendar`,
/// `/books/advent-calendar`).
pub fn back_to(base: &str, year: i64, day: i64) -> Response {
    Redirect::to(&format!("{base}/{year}/edit#day-{day}")).into_response()
}
