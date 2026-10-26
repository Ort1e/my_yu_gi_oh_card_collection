//! `fetch-release --to DIR`: the NAS's update, its download part. Asks
//! GitHub for the release `nas-latest` (built by the CI from `main`), and
//! when its `VERSION` differs from the installed one (`./VERSION`, next to
//! the app), downloads `collection-nas.tar.gz` into `DIR`.
//! `deploy/nas/update.sh` then unpacks it, backs up and restarts.
//!
//! The repository (`GITHUB_REPO`, `owner/name`, default this one) is
//! public: the files are downloaded from the release's public address. With
//! a `GITHUB_TOKEN` (were the repository private one day), through GitHub's
//! API instead.

use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

const TAG: &str = "nas-latest";
const ARCHIVE: &str = "collection-nas.tar.gz";
const REPO: &str = "Ort1e/my_yu_gi_oh_card_collection";
const AGENT: &str = "collection-nas";

#[derive(Debug, Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    /// The API address of the file (`…/releases/assets/<id>`).
    url: String,
}

/// What `fetch-release` found.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Fetched {
    UpToDate(String),
    /// The new version, its archive written in the folder.
    New(String),
}

/// The asset named `name` of the release.
fn asset<'a>(release: &'a Release, name: &str) -> anyhow::Result<&'a Asset> {
    release
        .assets
        .iter()
        .find(|a| a.name == name)
        .with_context(|| format!("the release {TAG} has no {name}"))
}

/// Where the release's files come from.
enum Source {
    /// The public address: `https://github.com/<repo>/releases/download/…`.
    Public(String),
    /// GitHub's API, with a token (a private repository).
    Api { token: String, release: Release },
}

impl Source {
    async fn new(http: &reqwest::Client) -> anyhow::Result<Source> {
        let repo = std::env::var("GITHUB_REPO")
            .ok()
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| REPO.to_string());
        let Some(token) = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()) else {
            return Ok(Source::Public(format!(
                "https://github.com/{repo}/releases/download/{TAG}"
            )));
        };

        let release = http
            .get(format!(
                "https://api.github.com/repos/{repo}/releases/tags/{TAG}"
            ))
            .bearer_auth(&token)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header(reqwest::header::USER_AGENT, AGENT)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(Source::Api { token, release })
    }

    /// The release's file `name`.
    async fn download(&self, http: &reqwest::Client, name: &str) -> anyhow::Result<Vec<u8>> {
        let request = match self {
            Source::Public(base) => http.get(format!("{base}/{name}")),
            Source::Api { token, release } => http
                .get(&asset(release, name)?.url)
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/octet-stream"),
        };
        let bytes = request
            .header(reqwest::header::USER_AGENT, AGENT)
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("downloading {name} of the release {TAG}"))?
            .bytes()
            .await?;
        Ok(bytes.to_vec())
    }
}

pub(crate) async fn fetch(http: &reqwest::Client, to: &Path) -> anyhow::Result<Fetched> {
    let source = Source::new(http).await?;

    let latest = String::from_utf8(source.download(http, "VERSION").await?)?
        .trim()
        .to_string();
    let installed = std::fs::read_to_string("VERSION").unwrap_or_default();
    if installed.trim() == latest {
        return Ok(Fetched::UpToDate(latest));
    }

    let archive = source.download(http, ARCHIVE).await?;
    std::fs::create_dir_all(to)?;
    let partial = to.join(format!("{ARCHIVE}.partial"));
    std::fs::write(&partial, archive)?;
    std::fs::rename(&partial, to.join(ARCHIVE))?;
    Ok(Fetched::New(latest))
}

pub(crate) async fn run(http: &reqwest::Client, to: &Path) -> anyhow::Result<()> {
    match fetch(http, to).await? {
        Fetched::UpToDate(version) => println!("up to date: {version}"),
        Fetched::New(version) => println!("new: {version} in {}", to.join(ARCHIVE).display()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_are_found_by_name() {
        let release: Release = serde_json::from_str(
            r#"{"name": "NAS build", "assets": [
                {"name": "VERSION", "url": "https://api.github.com/repos/o/r/releases/assets/1"},
                {"name": "collection-nas.tar.gz", "url": "https://api.github.com/repos/o/r/releases/assets/2"}
            ]}"#,
        )
        .unwrap();
        assert!(asset(&release, ARCHIVE).unwrap().url.ends_with("/2"));
        assert!(asset(&release, "other").is_err());
    }
}
