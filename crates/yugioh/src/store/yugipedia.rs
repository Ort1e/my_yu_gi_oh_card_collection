//! Yugipedia (https://yugipedia.com): a picture of each printing, for the
//! copies whose art or language ygoprodeck's picture doesn't show (an
//! Extended Art, a French card). Its card galleries are read through the
//! MediaWiki API and cached in `yugipedia_gallery`; which file fits a copy
//! is `yugioh_domain::printing_picture`. The chosen file is `card.picture`,
//! downloaded once into `media/card_pictures/` as a JPEG
//! (`routes::cards::images`).
//!
//! A new copy gets its printing's picture by itself, after its linking
//! ([`spawn_find_pictures`], when there is no doubt); `find-pictures` does it
//! for the whole collection, and the edit page lets a person choose.
//!
//! Every call goes through [`YUGIPEDIA`] (a wiki run by volunteers: one
//! request a second, a User-Agent saying who asks).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use common::db::{is_fresh, today};
use common::http::Throttle;
use serde_json::Value;
use yugioh_domain::printing_picture::{automatic, candidates};
use yugioh_domain::rarity::Rarity;

use crate::store::db::{begin_write, commit_unless_dry_run, Pool};
use crate::web::error::{AppError, AppResult};
use crate::AppState;

const API_URL: &str = "https://yugipedia.com/api.php";
const WIKI_URL: &str = "https://yugipedia.com/wiki/";
const USER_AGENT: &str = "my-yugioh-cards/0.1 (a personal card collection app)";
const TIMEOUT: Duration = Duration::from_secs(30);

/// A gallery is asked again after this many days (new printings, scans).
pub const GALLERY_DAYS: i64 = 30;

pub(crate) static YUGIPEDIA: Throttle = Throttle::new(Duration::from_secs(1));

/// The file names of the gallery of card data `card_data_id`, from the
/// cache when fresh, else from Yugipedia (and cached). `None` when Yugipedia
/// has no gallery for the card; an error when it can't be reached.
pub async fn gallery(
    pool: &Pool,
    http: &reqwest::Client,
    card_data_id: i64,
) -> AppResult<Option<Vec<String>>> {
    if let Some(files) = cached(pool, card_data_id, true).await? {
        return Ok(files);
    }

    let json: String = sqlx::query_scalar("SELECT json_data FROM card_data WHERE id = ?")
        .bind(card_data_id)
        .fetch_one(pool)
        .await?;
    let json: Value = serde_json::from_str(&json)
        .map_err(|e| AppError::Internal(format!("bad card json: {e}")))?;
    let Some(name) = json["name"].as_str() else {
        return Ok(None);
    };

    let files = fetch_gallery(http, name).await?;
    sqlx::query(
        "INSERT INTO yugipedia_gallery (card_data_id, files, checked_at) VALUES (?, ?, ?)
         ON CONFLICT (card_data_id) DO UPDATE SET files = excluded.files,
             checked_at = excluded.checked_at",
    )
    .bind(card_data_id)
    .bind(files.as_ref().map(|f| Value::from(f.clone()).to_string()))
    .bind(today().to_string())
    .execute(pool)
    .await?;
    Ok(files)
}

/// The cached gallery, without asking Yugipedia: `Some(files)` when there is
/// a row (fresh only if `fresh_only`), `None` when it must be asked.
pub async fn cached(
    pool: &Pool,
    card_data_id: i64,
    fresh_only: bool,
) -> AppResult<Option<Option<Vec<String>>>> {
    let row: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT files, checked_at FROM yugipedia_gallery WHERE card_data_id = ?")
            .bind(card_data_id)
            .fetch_optional(pool)
            .await?;
    let Some((files, checked_at)) = row else {
        return Ok(None);
    };
    if fresh_only && !is_fresh(&checked_at, GALLERY_DAYS, today()) {
        return Ok(None);
    }
    Ok(Some(files.and_then(|f| serde_json::from_str(&f).ok())))
}

/// The pictures of `Card Gallery:<name>` that exist, every page of them
/// (500 a page). The gallery also names scans never uploaded (red links,
/// `missing`) or deleted since (no `imagerepository`): left out. `None`
/// when the gallery has no picture (or doesn't exist).
async fn fetch_gallery(http: &reqwest::Client, name: &str) -> AppResult<Option<Vec<String>>> {
    let title = format!("Card Gallery:{name}");
    let mut files = BTreeSet::new();
    // MediaWiki's continuation: its `continue` values sent back as they are
    let mut next: Vec<(String, String)> = Vec::new();
    for _ in 0..MAX_REQUESTS {
        let mut query: Vec<(String, String)> = [
            ("action", "query"),
            ("format", "json"),
            ("generator", "images"),
            ("gimlimit", "500"),
            ("prop", "imageinfo"),
            ("iiprop", "size"),
            ("redirects", "1"),
            ("titles", title.as_str()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        query.append(&mut next);

        YUGIPEDIA.wait().await;
        let fail = |e: reqwest::Error| AppError::Internal(format!("Yugipedia: {e}"));
        let json: Value = http
            .get(API_URL)
            .query(&query)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .timeout(TIMEOUT)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(fail)?
            .json()
            .await
            .map_err(fail)?;

        // each picture of the gallery is a page of the answer
        let pages = json["query"]["pages"].as_object().into_iter().flatten();
        for (_, page) in pages {
            let stored = page["imagerepository"]
                .as_str()
                .is_some_and(|r| !r.is_empty());
            if page.get("missing").is_some() || !stored {
                continue;
            }
            if let Some(file) = page["title"].as_str() {
                let file = file.strip_prefix("File:").unwrap_or(file);
                files.insert(file.to_string());
            }
        }

        let Some(more) = json["continue"].as_object() else {
            break;
        };
        next = more
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
            .collect();
    }
    Ok(Some(files.into_iter().collect::<Vec<_>>()).filter(|f| !f.is_empty()))
}

/// Requests for one gallery at most (a few thousand pictures): a guard
/// against a continuation that never ends.
const MAX_REQUESTS: usize = 20;

/// Where a gallery file is downloaded from.
pub fn file_url(file: &str) -> String {
    let mut url = url::Url::parse(WIKI_URL).expect("a valid URL");
    url.path_segments_mut()
        .expect("an http URL")
        .pop_if_empty()
        .push("Special:FilePath")
        .push(file);
    url.to_string()
}

/// Where a gallery file is kept, as a JPEG: `media/card_pictures/<name>.jpg`.
pub fn file_path(state: &AppState, file: &str) -> PathBuf {
    state.media_dir.join("card_pictures").join(file_name(file))
}

/// The cached file's name: the gallery's, letters, digits and `-` kept.
pub fn file_name(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let stem: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{stem}.jpg")
}

#[derive(sqlx::FromRow)]
struct Copy {
    id: i64,
    card_data_id: i64,
    code: String,
    rarity: Option<String>,
}

/// What [`find_pictures`] did (or would do).
#[derive(Debug, Default)]
pub struct Report {
    /// Galleries asked to Yugipedia.
    pub asked: usize,
    /// Copies given their printing's picture.
    pub found: usize,
    /// Copies with several artworks to choose from (their ids).
    pub to_choose: Vec<i64>,
    /// Copies whose gallery is still to ask (the limit, or Yugipedia down).
    pub waiting: usize,
}

/// Give copies their printing's picture when there is no doubt
/// (`printing_picture::automatic`): the copies `card_ids` (new ones, after
/// their linking), or all of them (`find-pictures`). Only those with an
/// automatic picture still at ygoprodeck's, a code and their card data. At
/// most `limit` galleries are asked; a `dry_run` asks none and saves
/// nothing.
pub async fn find_pictures(
    state: &AppState,
    card_ids: Option<&[i64]>,
    dry_run: bool,
    limit: usize,
) -> AppResult<Report> {
    let pool = &state.pool;
    let mut copies: Vec<Copy> = sqlx::query_as(
        "SELECT id, card_data_id, code, rarity FROM card
         WHERE picture IS NULL AND picture_manual = 0
             AND card_data_id IS NOT NULL AND code IS NOT NULL AND trim(code) <> ''
         ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    if let Some(ids) = card_ids {
        copies.retain(|copy| ids.contains(&copy.id));
    }
    let mut by_data: BTreeMap<i64, Vec<Copy>> = BTreeMap::new();
    for copy in copies {
        by_data.entry(copy.card_data_id).or_default().push(copy);
    }

    // network calls before the write transaction (they write the cache)
    let mut report = Report::default();
    let mut reachable = true;
    let mut found = Vec::new();
    for (card_data_id, copies) in by_data {
        let files = match cached(pool, card_data_id, true).await? {
            Some(files) => files,
            None if dry_run || !reachable || report.asked >= limit => {
                report.waiting += copies.len();
                continue;
            }
            None => {
                report.asked += 1;
                match gallery(pool, &state.http, card_data_id).await {
                    Ok(files) => files,
                    Err(e) => {
                        tracing::warn!(card_data_id, "Yugipedia gallery: {e}");
                        reachable = false;
                        report.waiting += copies.len();
                        continue;
                    }
                }
            }
        };
        let Some(files) = files else {
            continue;
        };

        for copy in copies {
            let rarity = copy.rarity.as_deref().and_then(Rarity::from_label);
            match automatic(&files, &copy.code, rarity) {
                Some(file) => found.push((copy.id, file.to_string())),
                None if candidates(&files, &copy.code, rarity).len() > 1 => {
                    report.to_choose.push(copy.id)
                }
                None => {}
            }
        }
    }

    let mut tx = begin_write(pool).await?;
    for (id, file) in &found {
        sqlx::query(
            "UPDATE card SET picture = ? WHERE id = ? AND picture IS NULL AND picture_manual = 0",
        )
        .bind(file)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    commit_unless_dry_run(tx, dry_run).await?;
    report.found = found.len();
    Ok(report)
}

/// In the background: [`find_pictures`] for these copies (a new card, a new
/// code or rarity), logged when it found any.
pub fn spawn_find_pictures(state: AppState, card_ids: Vec<i64>) {
    if card_ids.is_empty() {
        return;
    }
    tokio::spawn(async move {
        match find_pictures(&state, Some(&card_ids), false, usize::MAX).await {
            Ok(report) if report.found > 0 => {
                tracing::info!(found = report.found, "printings' pictures found")
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("finding pictures: {e:?}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::TestApp;

    #[tokio::test]
    async fn gives_a_picture_only_without_doubt() {
        let t = TestApp::new().await;
        // galleries cached: nothing asked to Yugipedia
        t.exec(
            r#"INSERT INTO yugipedia_gallery (card_data_id, files, checked_at) VALUES
                 (1, '["DarkMagician-LOB-FR-UR-1E.png", "DarkMagician-LOB-FR-UR-UE.png",
                       "DarkMagician-SDY-FR-UR-1E.png", "DarkMagician-SDY-FR-UR-1E-AA.png"]',
                  date('now')),
                 (2, NULL, date('now'));
             INSERT INTO card (id, name, en_name, code, rarity, card_data_id, is_proxy, picture,
                               picture_manual) VALUES
                 (1, 'one artwork', 'dark magician', 'LOB-FR005', 'Ultra Rare', 1, 0, NULL, 0),
                 (2, 'two artworks', 'dark magician', 'SDY-FR005', 'Ultra Rare', 1, 0, NULL, 0),
                 (3, 'chosen by hand', 'dark magician', 'LOB-FR005', 'Ultra Rare', 1, 0, NULL, 1),
                 (4, 'no gallery', 'kuriboh', 'MRD-FR071', NULL, 2, 0, NULL, 0),
                 (5, 'no code', 'dark magician', NULL, NULL, 1, 0, NULL, 0);"#,
        )
        .await;
        let pictures =
            || t.text("SELECT group_concat(id || ':' || coalesce(picture, '-'), ' ') FROM card");

        let report = find_pictures(&t.state, None, true, 0).await.unwrap();
        assert_eq!((report.found, report.asked, report.waiting), (1, 0, 0));
        assert_eq!(pictures().await.unwrap(), "1:- 2:- 3:- 4:- 5:-", "dry run");

        let report = find_pictures(&t.state, None, false, 0).await.unwrap();
        assert_eq!(report.found, 1);
        assert_eq!(report.to_choose, [2]);
        assert_eq!(
            pictures().await.unwrap(),
            "1:DarkMagician-LOB-FR-UR-1E.png 2:- 3:- 4:- 5:-"
        );

        // a gallery too old waits for a lookup (none allowed here)
        t.exec(
            "UPDATE yugipedia_gallery SET checked_at = '2020-01-01';
             UPDATE card SET picture = NULL WHERE id = 1;",
        )
        .await;
        let report = find_pictures(&t.state, None, false, 0).await.unwrap();
        assert_eq!((report.found, report.waiting), (0, 3));

        // only the copies asked for
        t.exec("UPDATE yugipedia_gallery SET checked_at = date('now')")
            .await;
        let report = find_pictures(&t.state, Some(&[2, 4]), false, 0)
            .await
            .unwrap();
        assert_eq!(
            (report.found, report.to_choose.as_slice()),
            (0, [2].as_slice())
        );
    }

    #[test]
    fn file_names_and_urls() {
        assert_eq!(
            file_name("WitnessoftheAncient-CORI-FR-UR-1E-EA.png"),
            "WitnessoftheAncient-CORI-FR-UR-1E-EA.jpg"
        );
        assert_eq!(file_name("a/../b c.png"), "a____b_c.jpg");
        assert_eq!(
            file_url("Ash & Co-X.png"),
            "https://yugipedia.com/wiki/Special:FilePath/Ash%20&%20Co-X.png"
        );
    }
}
