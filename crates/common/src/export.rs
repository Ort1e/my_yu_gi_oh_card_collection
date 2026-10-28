//! The public copy of the collections (`export-public`): the app's own
//! public pages saved as static files, for a web hosting where nothing runs.
//!
//! [`write_site`] asks the app for its pages, in-process, while
//! [`public_copy`] is true: the templates then leave out what is private
//! (`{% if !common::export::public_copy() %}`: the private menus, the edit
//! buttons and filters, lots and prices paid, notes). It starts from
//! [`Site::start`] and follows the links, only to the addresses
//! [`Site::public`] accepts (never `/lots`, `/sellers`…). Each page is saved
//! under a flat name, its links rewritten to these files (a link to
//! anything else becomes `#`):
//!
//! ```text
//! index.html                the first start page, again
//! cards.html                /cards
//! cards-page-2.html         /cards?page=2 (only `page` and `view` are kept)
//! cards-12.html             /cards/12
//! img/card-image-12.webp    /card-image/12, 480 px wide
//! static/…                  the stylesheets and scripts the pages use
//! ```

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use axum::body::Body;
use axum::http::{header, Request};
use axum::Router;
use regex::{Captures, Regex};
use tower::ServiceExt;
use url::Url;

use crate::photos;

/// The file that marks a folder as an export: only such a folder (or a
/// missing or empty one) is emptied and written again.
const MARKER: &str = ".public-export";

/// The largest width of the pictures' copies (the grids, the card pages,
/// the picture seen large on hover or full screen). The OVH free plan has
/// 100 MB: the ~640 pictures (2026-10) are about 49 MB as WebP at 480 px
/// (73 MB as JPEG; 600 px would be 67 MB as WebP, 101 MB as JPEG).
const PICTURE_WIDTH: u32 = 480;

/// The query parameters a copied page keeps: the others (filters) aren't
/// in the copy.
const KEPT_PARAMETERS: [&str; 2] = ["page", "view"];

tokio::task_local! {
    static PUBLIC_COPY: ();
}

/// Whether the page being rendered goes into the public copy: the
/// templates leave out what is private then.
pub fn public_copy() -> bool {
    PUBLIC_COPY.try_with(|_| ()).is_ok()
}

/// What to copy.
pub struct Site<'a> {
    /// The app, with the login off (it is asked here, not over the network).
    pub app: Router,
    /// The pages to start from; the first one is also `index.html`.
    pub start: &'a [&'a str],
    /// Whether an address (its path) may be in the copy: pages and pictures.
    pub public: fn(&str) -> bool,
    /// Where `/static` is (`static`).
    pub static_dir: &'a Path,
}

/// What [`write_site`] wrote.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Written {
    pub pages: usize,
    pub pictures: usize,
    /// Pages or pictures the app couldn't give (their links became `#`).
    pub missing: usize,
    pub bytes: u64,
}

/// Write the copy into `out` (emptied first; refused if it holds something
/// else than an earlier export).
pub async fn write_site(site: Site<'_>, out: &Path) -> anyhow::Result<Written> {
    prepare(out)?;
    let mut written = Written::default();

    // address → its file in the copy
    let mut files: HashMap<String, String> = HashMap::new();
    let mut pages: Vec<(String, String, String)> = Vec::new();
    let mut statics: BTreeSet<String> = BTreeSet::new();
    let mut missing: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = site.start.iter().map(|s| s.to_string()).collect();

    while let Some(address) = queue.pop_front() {
        if files.contains_key(&address) || missing.contains(&address) {
            continue;
        }

        let request = Request::get(&address).body(Body::empty())?;
        let (kind, bytes) = PUBLIC_COPY
            .scope((), async {
                let response = site.app.clone().oneshot(request).await?;
                if !response.status().is_success() {
                    return Ok::<_, anyhow::Error>(None);
                }
                let kind = response
                    .headers()
                    .get(header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
                Ok(Some((kind, bytes)))
            })
            .await?
            .unzip();
        let (Some(kind), Some(bytes)) = (kind, bytes) else {
            tracing::warn!(address, "not in the copy: the app doesn't give it");
            missing.insert(address);
            continue;
        };

        if kind.starts_with("image/") {
            let name = format!("img/{}.webp", flat_name(&address));
            let copy =
                tokio::task::spawn_blocking(move || photos::thumbnail(&bytes, PICTURE_WIDTH))
                    .await?;
            match copy {
                Ok(copy) => {
                    std::fs::create_dir_all(out.join("img"))?;
                    std::fs::write(out.join(&name), copy)?;
                    written.pictures += 1;
                    files.insert(address, name);
                }
                Err(error) => {
                    tracing::warn!(address, %error, "unreadable picture");
                    missing.insert(address);
                }
            }
            continue;
        }

        let html = String::from_utf8(bytes.to_vec())?;
        for link in links(&html) {
            match target(&address, &link) {
                Target::Static(path) => {
                    statics.insert(path);
                }
                Target::App(next) if (site.public)(path_of(&next)) => queue.push_back(next),
                _ => {}
            }
        }
        let name = format!("{}.html", flat_name(&address));
        files.insert(address.clone(), name.clone());
        pages.push((address, name, html));
    }
    written.missing = missing.len();

    for path in &statics {
        let relative = path.trim_start_matches("/static/");
        let to = out.join("static").join(relative);
        std::fs::create_dir_all(to.parent().expect("a file in static/"))?;
        if let Err(error) = std::fs::copy(site.static_dir.join(relative), &to) {
            tracing::warn!(path, %error, "static file not copied");
        }
    }

    for (i, (address, name, html)) in pages.iter().enumerate() {
        let html = rewrite(html, address, &files);
        std::fs::write(out.join(name), &html)?;
        if i == 0 {
            std::fs::write(out.join("index.html"), &html)?;
        }
        written.pages += 1;
    }

    written.bytes = folder_size(out)?;
    Ok(written)
}

/// Where a link goes.
#[derive(Debug, PartialEq, Eq)]
enum Target {
    /// Another site, an anchor, `mailto:`…: kept as it is.
    Outside,
    /// A file of `/static`: `/static/css/style.css`.
    Static(String),
    /// A page or picture of the app: its path and kept query.
    App(String),
}

static ATTRIBUTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b(href|src)="([^"]*)""#).expect("a valid regex"));

/// The links of a page (its `href` and `src`, decoded).
fn links(html: &str) -> Vec<String> {
    ATTRIBUTE
        .captures_iter(html)
        .map(|c| decode(&c[2]))
        .collect()
}

/// The page's links pointed at the copy's files, `#` when not in it.
fn rewrite(html: &str, address: &str, files: &HashMap<String, String>) -> String {
    ATTRIBUTE
        .replace_all(html, |c: &Captures| {
            let link = decode(&c[2]);
            let new = match target(address, &link) {
                Target::Outside => return c[0].to_string(),
                Target::Static(path) => path.trim_start_matches('/').to_string(),
                Target::App(next) => files.get(&next).cloned().unwrap_or_else(|| "#".into()),
            };
            format!(r#"{}="{new}""#, &c[1])
        })
        .into_owned()
}

/// Askama escapes `&`, `/`, quotes in attributes: their link as written.
fn decode(attribute: &str) -> String {
    attribute
        .replace("&#x2f;", "/")
        .replace("&#x2F;", "/")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// `link`, on the page at `address`.
fn target(address: &str, link: &str) -> Target {
    if link.is_empty() || link.starts_with('#') {
        return Target::Outside;
    }
    let app = Url::parse("http://app/").expect("a valid URL");
    let Some(url) = app.join(address).and_then(|page| page.join(link)).ok() else {
        return Target::Outside;
    };
    if url.origin() != app.origin() {
        return Target::Outside;
    }

    let path = url.path().to_string();
    if path.starts_with("/static/") {
        return Target::Static(path);
    }
    let mut kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, value)| {
            KEPT_PARAMETERS.contains(&key.as_ref()) && !(key == "page" && value == "1")
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    kept.sort();
    if kept.is_empty() {
        return Target::App(path);
    }
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(kept)
        .finish();
    Target::App(format!("{path}?{query}"))
}

fn path_of(address: &str) -> &str {
    address.split('?').next().unwrap_or(address)
}

/// A file name for an address: `/cards?page=2` → `cards-page-2`.
fn flat_name(address: &str) -> String {
    let mut name = String::new();
    for c in address.trim_start_matches('/').chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            name.push(c);
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    let name = name.trim_matches('-');
    if name.is_empty() {
        "home".to_string()
    } else {
        name.to_string()
    }
}

/// Empty `out` for a new export: a missing or empty folder, or an earlier
/// export (it has the [`MARKER`]); anything else is refused.
fn prepare(out: &Path) -> anyhow::Result<()> {
    if out.exists() {
        let empty = std::fs::read_dir(out)?.next().is_none();
        if !empty && !out.join(MARKER).exists() {
            anyhow::bail!(
                "{} is not an earlier export (no {MARKER} file): choose an empty folder",
                out.display()
            );
        }
        std::fs::remove_dir_all(out)?;
    }
    std::fs::create_dir_all(out)?;
    std::fs::write(
        out.join(MARKER),
        "Written by `backend export-public`: emptied at each export.\n",
    )?;
    Ok(())
}

fn folder_size(dir: &Path) -> std::io::Result<u64> {
    let mut size = 0;
    let mut folders = vec![PathBuf::from(dir)];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                folders.push(entry.path());
            } else {
                size += metadata.len();
            }
        }
    }
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_point_at_the_copys_files() {
        assert_eq!(
            target("/cards", &decode("?page=2&amp;view=table")),
            Target::App("/cards?page=2&view=table".into())
        );
        assert_eq!(
            target("/cards?page=3", "?page=1"),
            Target::App("/cards".into())
        );
        // filters are not in the copy
        assert_eq!(
            target("/cards", "/cards?rarity=Ultra+Rare&page=2"),
            Target::App("/cards?page=2".into())
        );
        assert_eq!(
            target("/cards/12", "/card-image/3"),
            Target::App("/card-image/3".into())
        );
        assert_eq!(
            target("/books/authors/2", "../series/4"),
            Target::App("/books/series/4".into())
        );
        assert_eq!(
            target("/cards", "/static/css/style.css"),
            Target::Static("/static/css/style.css".into())
        );
        for outside in ["https://cdn.jsdelivr.net/x.js", "#top", "", "mailto:a@b.c"] {
            assert_eq!(target("/cards", outside), Target::Outside, "{outside}");
        }

        assert_eq!(
            flat_name("/cards?page=2&view=table"),
            "cards-page-2-view-table"
        );
        assert_eq!(
            flat_name("/pokemon/card-image/5/fr"),
            "pokemon-card-image-5-fr"
        );
        assert_eq!(flat_name("/"), "home");
        assert_eq!(
            decode("&#x2f;cards&#x2f;12?a=1&amp;b=2"),
            "/cards/12?a=1&b=2"
        );

        let files = HashMap::from([
            ("/cards/12".to_string(), "cards-12.html".to_string()),
            (
                "/card-image/3".to_string(),
                "img/card-image-3.webp".to_string(),
            ),
        ]);
        let html = r#"<a href="&#x2f;cards&#x2f;12"><img src="/card-image/3"></a> <a href="/lots/1">lot</a> <link href="/static/css/a.css"> <a href="https://x.org/">x</a>"#;
        assert_eq!(
            rewrite(html, "/cards", &files),
            r##"<a href="cards-12.html"><img src="img/card-image-3.webp"></a> <a href="#">lot</a> <link href="static/css/a.css"> <a href="https://x.org/">x</a>"##
        );
    }
}
