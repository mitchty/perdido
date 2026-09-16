mod component_table;
mod k3s;
mod matrix;
mod output;
mod rancher;
mod rke2;
mod sidebar;

use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use futures::FutureExt;
use futures::future::join_all;
use k3s::{K3sMinorLine, K3sRelease};
use rancher::{RancherMinorLine, RancherReleaseMatrix, SupportMatrixSnapshot};
use rke2::{Rke2MinorLine, Rke2Release};

#[derive(Parser)]
#[command(name = "perdido", about = "SUSE Rancher support-matrix scraper")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Dump the full scraped Rancher snapshot with Debug formatting.
    Debug,
    /// Print the latest Rancher release per minor line and its Kubernetes bounds.
    Rancher {
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Print each RKE2 minor line and the Kubernetes version it's built on.
    Rke2 {
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Print each k3s minor line and the Kubernetes version it's built on.
    K3s {
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Print, for each Rancher release, every RKE2/k3s minor line within
    /// its certified Kubernetes range (latest first).
    Query {
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    let client = reqwest::Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()?;

    match cli.command {
        Command::Debug => {
            let (minor_lines, latest_releases) = rancher_snapshot(&client).await?;
            let snapshot = SupportMatrixSnapshot {
                minor_lines,
                latest_releases,
            };
            println!("{snapshot:#?}");
        }
        Command::Rancher { json } => {
            let (_, latest_releases) = rancher_snapshot(&client).await?;
            output::print_rancher_summary(&latest_releases, json)?;
        }
        Command::Rke2 { json } => {
            let releases = rke2_releases(&client).await?;
            output::print_rke2_summary(&releases, json)?;
        }
        Command::K3s { json } => {
            let releases = k3s_releases(&client).await?;
            output::print_k3s_summary(&releases, json)?;
        }
        Command::Query { json } => {
            // Rancher, RKE2, and k3s live under separate sidebar sections
            // but share nothing else fetch-wise, so pull all three product
            // lines at once.
            let (rancher_result, rke2_result, k3s_result) = tokio::join!(
                rancher_snapshot(&client),
                rke2_releases(&client),
                k3s_releases(&client)
            );
            let (_, rancher_releases) = rancher_result?;
            let rke2_release_list = rke2_result?;
            let k3s_release_list = k3s_result?;
            let rows = matrix::build_rows(&rancher_releases, &rke2_release_list, &k3s_release_list);
            output::print_query_summary(&rows, json)?;
        }
    }

    Ok(())
}

/// Fetch the Rancher sidebar index plus every minor line's latest release.
async fn rancher_snapshot(
    client: &reqwest::Client,
) -> Result<(Vec<RancherMinorLine>, Vec<RancherReleaseMatrix>)> {
    let entry_html = fetch(client, rancher::ENTRY_POINT).await?;
    let minor_lines = rancher::parse_minor_lines(&entry_html)?;
    let latest_releases =
        fetch_rancher_releases(client, rancher::ENTRY_POINT, &entry_html, &minor_lines).await;
    Ok((minor_lines, latest_releases))
}

/// Fetch the RKE2 sidebar index plus every minor line's own page.
async fn rke2_releases(client: &reqwest::Client) -> Result<Vec<Rke2Release>> {
    let entry_html = fetch(client, rke2::ENTRY_POINT).await?;
    let minor_lines = rke2::parse_minor_lines(&entry_html)?;
    Ok(fetch_rke2_releases(client, rke2::ENTRY_POINT, &entry_html, &minor_lines).await)
}

/// Fetch the k3s sidebar index plus every minor line's own page.
async fn k3s_releases(client: &reqwest::Client) -> Result<Vec<K3sRelease>> {
    let entry_html = fetch(client, k3s::ENTRY_POINT).await?;
    let minor_lines = k3s::parse_minor_lines(&entry_html)?;
    Ok(fetch_k3s_releases(client, k3s::ENTRY_POINT, &entry_html, &minor_lines).await)
}

/// Fetch and parse every minor line's latest release page concurrently.
///
/// `minor_lines` already excludes pre-2.7 releases (see
/// [`rancher::parse_minor_lines`]), so any parse failure here is an
/// unexpected upstream layout change rather than a known legacy gap; it's
/// still just skipped (with a warning) rather than failing the whole run.
async fn fetch_rancher_releases(
    client: &reqwest::Client,
    entry_point: &str,
    entry_html: &str,
    minor_lines: &[RancherMinorLine],
) -> Vec<RancherReleaseMatrix> {
    let requests = minor_lines
        .iter()
        .filter_map(RancherMinorLine::latest)
        .map(|patch| {
            let label = patch.version.clone();
            let url = patch.url.clone();
            let already_fetched = (url == entry_point).then(|| entry_html.to_string());
            async move {
                let html = match already_fetched {
                    Some(html) => html,
                    None => fetch(client, &url).await?,
                };
                rancher::parse_release_matrix(&html, &url)
            }
            .map(|result| (label, result))
        });

    collect_ok(join_all(requests).await)
}

/// Fetch and parse every RKE2 minor line's page concurrently.
async fn fetch_rke2_releases(
    client: &reqwest::Client,
    entry_point: &str,
    entry_html: &str,
    minor_lines: &[Rke2MinorLine],
) -> Vec<Rke2Release> {
    let requests = minor_lines.iter().map(|line| {
        let label = line.minor.clone();
        let url = line.url.clone();
        let already_fetched = (url == entry_point).then(|| entry_html.to_string());
        async move {
            let html = match already_fetched {
                Some(html) => html,
                None => fetch(client, &url).await?,
            };
            rke2::parse_release(&html, &url)
        }
        .map(|result| (label, result))
    });

    collect_ok(join_all(requests).await)
}

/// Fetch and parse every k3s minor line's page concurrently.
async fn fetch_k3s_releases(
    client: &reqwest::Client,
    entry_point: &str,
    entry_html: &str,
    minor_lines: &[K3sMinorLine],
) -> Vec<K3sRelease> {
    let requests = minor_lines.iter().map(|line| {
        let label = line.minor.clone();
        let url = line.url.clone();
        let already_fetched = (url == entry_point).then(|| entry_html.to_string());
        async move {
            let html = match already_fetched {
                Some(html) => html,
                None => fetch(client, &url).await?,
            };
            k3s::parse_release(&html, &url)
        }
        .map(|result| (label, result))
    });

    collect_ok(join_all(requests).await)
}

/// Keep only the successful results, logging (and dropping) any failures
/// rather than failing the whole run over one unparseable page. `label`
/// identifies the release (e.g. `"v2.5.16"`, `"1.36"`) independently of
/// whatever the page itself could be parsed to say, so the message stays
/// useful even when parsing failed before a version could be recovered.
fn collect_ok<T>(results: Vec<(String, Result<T>)>) -> Vec<T> {
    results
        .into_iter()
        .filter_map(|(label, result)| match result {
            Ok(value) => Some(value),
            Err(err) => {
                eprintln!("skipping {label}: {err}");
                None
            }
        })
        .collect()
}

async fn fetch(client: &reqwest::Client, url: &str) -> Result<String> {
    //    eprintln!("fetching {url}");
    let text = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(text)
}
