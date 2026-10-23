//! Page-number pagination for the list pages (10 items per page, as in Django).

use serde::Deserialize;

pub const PAGE_SIZE: i64 = 10;

#[derive(Debug, Deserialize)]
pub struct PageParam {
    pub page: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub number: i64,
    pub num_pages: i64,
    /// Items matching the filters.
    pub count: i64,
    /// Other query parameters (filters), already URL-encoded, to keep in the
    /// pagination links.
    pub querystring: String,
    /// Items per page.
    pub size: i64,
}

impl Page {
    /// Clamp the requested page into range (Django would 404 instead).
    pub fn new(requested: Option<i64>, count: i64, querystring: String) -> Page {
        Page::sized(requested, count, querystring, PAGE_SIZE)
    }

    /// `size` items per page (the picture grids show more).
    pub fn sized(requested: Option<i64>, count: i64, querystring: String, size: i64) -> Page {
        let num_pages = ((count + size - 1) / size).max(1);
        Page {
            number: requested.unwrap_or(1).clamp(1, num_pages),
            num_pages,
            count,
            querystring,
            size,
        }
    }

    pub fn offset(&self) -> i64 {
        (self.number - 1) * self.size
    }

    pub fn has_previous(&self) -> bool {
        self.number > 1
    }

    pub fn has_next(&self) -> bool {
        self.number < self.num_pages
    }

    pub fn first_href(&self) -> String {
        self.href(1)
    }

    pub fn prev_href(&self) -> String {
        self.href(self.number - 1)
    }

    pub fn next_href(&self) -> String {
        self.href(self.number + 1)
    }

    pub fn last_href(&self) -> String {
        self.href(self.num_pages)
    }

    /// Link to page `n`, keeping the filters.
    fn href(&self, n: i64) -> String {
        if self.querystring.is_empty() {
            format!("?page={n}")
        } else {
            format!("?page={n}&{}", self.querystring)
        }
    }
}

/// Encode `(key, value)` pairs, skipping empty values.
pub fn querystring<'a>(pairs: impl IntoIterator<Item = (&'a str, Option<&'a str>)>) -> String {
    let mut out = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            out.append_pair(k, v);
        }
    }
    out.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages() {
        let p = Page::new(Some(3), 25, String::new());
        assert_eq!((p.number, p.num_pages, p.offset()), (3, 3, 20));
        assert!(p.has_previous() && !p.has_next());
        assert_eq!(Page::new(Some(9), 0, String::new()).number, 1);
        assert_eq!(Page::new(None, 0, "a=b".into()).href(2), "?page=2&a=b");
    }

    #[test]
    fn querystring_skips_empty() {
        assert_eq!(
            querystring([
                ("name", Some("dark magician")),
                ("code", Some("")),
                ("x", None)
            ]),
            "name=dark+magician"
        );
    }
}
