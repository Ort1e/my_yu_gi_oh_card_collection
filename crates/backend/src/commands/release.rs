//! `fetch-release --to DIR`: the NAS's update, its download part. Asks
//! GitHub for the release `nas-latest` (built by the CI from `main`), and
//! when its `VERSION` differs from the installed one (`./VERSION`, next to
//! the app), downloads `collection-nas.tar.gz` into `DIR`.
//! `deploy/nas/update.sh` then unpacks it, backs up and restarts.
//!
//! The repository is private: the release is read through GitHub's API
//! with a token that can only read it (`GITHUB_TOKEN`), from
//! `GITHUB_REPO` (`owner/name`).

use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

const TAG: &str = "nas-latest";
const ARCHIVE: &str = "collection-nas.tar.gz";

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

async fn download(http: &reqwest::Client, token: &str, asset: &Asset) -> anyhow::Result<Vec<u8>> {
    let bytes = http
        .get(&asset.url)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/octet-stream")
        .header(reqwest::header::USER_AGENT, "collection-nas")
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(bytes.to_vec())
}

pub(crate) async fn fetch(http: &reqwest::Client, to: &Path) -> anyhow::Result<Fetched> {
    let token = std::env::var("GITHUB_TOKEN").context("GITHUB_TOKEN is not set")?;
    let repo = std::env::var("GITHUB_REPO").context("GITHUB_REPO is not set (owner/name)")?;

    let release: Release = http
        .get(format!(
            "https://api.github.com/repos/{repo}/releases/tags/{TAG}"
        ))
        .bearer_auth(&token)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(reqwest::header::USER_AGENT, "collection-nas")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let latest = String::from_utf8(download(http, &token, asset(&release, "VERSION")?).await?)?
        .trim()
        .to_string();
    let installed = std::fs::read_to_string("VERSION").unwrap_or_default();
    if installed.trim() == latest {
        return Ok(Fetched::UpToDate(latest));
    }

    let archive = download(http, &token, asset(&release, ARCHIVE)?).await?;
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
