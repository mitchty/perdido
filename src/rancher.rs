//! Scraper for SUSE's Rancher "all supported versions" support-matrix pages.
//!
//! Every page under `/suse-rancher/support-matrix/all-supported-versions/`
//! ships the same sidebar accordion listing every Rancher release, so any
//! single page doubles as an index for the whole matrix. Each page's own
//! body additionally carries that release's Kubernetes support table.

use crate::sidebar;
use color_eyre::eyre::{Result, eyre};
use scraper::{ElementRef, Html, Selector};
use serde::Serialize;

/// A known-good support-matrix page. Its sidebar enumerates every other
/// Rancher release, so it also serves as the bootstrap index.
///
/// No trailing slash, matching the form of the sidebar's own `<a href>`
/// values, so the entry point and its own sidebar entry share one cache key.
pub const ENTRY_POINT: &str =
    "https://www.suse.com/suse-rancher/support-matrix/all-supported-versions/rancher-v2-15-1";

/// One row of the "Supported Kubernetes Platforms for Rancher Manager" table.
#[derive(Debug, Clone, Serialize)]
pub struct KubernetesPlatformSupport {
    pub distro: String,
    pub lowest_version: String,
    pub highest_version: String,
    pub notes: Option<String>,
}

/// A single published Rancher patch release, as listed in the sidebar.
#[derive(Debug, Clone, Serialize)]
pub struct RancherPatchRelease {
    pub version: String,
    pub url: String,
}

/// All patch releases published under one `major.minor` Rancher line.
#[derive(Debug, Clone, Serialize)]
pub struct RancherMinorLine {
    pub minor: String,
    pub patches: Vec<RancherPatchRelease>,
}

impl RancherMinorLine {
    /// Sidebar order is newest-first, so the latest patch is always first.
    #[must_use]
    pub fn latest(&self) -> Option<&RancherPatchRelease> {
        self.patches.first()
    }
}

/// Kubernetes support matrix scraped from one Rancher version's own page.
#[derive(Debug, Clone, Serialize)]
pub struct RancherReleaseMatrix {
    pub version: String,
    pub url: String,
    pub kubernetes_support: Vec<KubernetesPlatformSupport>,
}

impl RancherReleaseMatrix {
    /// The Kubernetes version range this release itself certifies.
    ///
    /// Prefers the RKE2 row: it's Rancher's own downstream distro and the
    /// row every other page has, whereas hosted-provider rows (AKS/EKS/GKE)
    /// sometimes trail it by a build metadata suffix (e.g. `v1.24.12-gke.1000`).
    #[must_use]
    pub fn kubernetes_range(&self) -> Option<&KubernetesPlatformSupport> {
        self.kubernetes_support
            .iter()
            .find(|platform| platform.distro.eq_ignore_ascii_case("RKE2"))
            .or_else(|| self.kubernetes_support.first())
    }
}

/// Top-level payload this crate is building toward exposing as a JSON "API".
#[derive(Debug, Clone, Serialize)]
pub struct SupportMatrixSnapshot {
    pub minor_lines: Vec<RancherMinorLine>,
    pub latest_releases: Vec<RancherReleaseMatrix>,
}

/// Oldest Rancher minor line worth fetching a release page for. Below this,
/// pages either link out to a PDF instead of an HTML table (pre-2.6) or use
/// a different layout entirely (pre-2.5); either way there's no Kubernetes
/// range to scrape, so these lines are dropped before any page fetch is
/// even attempted rather than surfacing a per-page parse warning.
const MIN_SUPPORTED_MINOR: (u32, u32) = (2, 7);

/// Parse the sidebar `RANCHER x.y.X` accordion into minor-line -> patch lists.
///
/// # Errors
/// Returns an error if no `RANCHER x.y.X` sections are found, which means
/// the sidebar markup has changed upstream.
pub fn parse_minor_lines(html: &str) -> Result<Vec<RancherMinorLine>> {
    let lines: Vec<RancherMinorLine> = sidebar::parse_sections(html)
        .into_iter()
        .filter_map(|section| {
            let minor = section
                .heading
                .strip_prefix("RANCHER ")
                .and_then(|rest| rest.strip_suffix(".X"))?
                .to_string();
            let patches = section
                .links
                .into_iter()
                .map(|link| RancherPatchRelease {
                    version: link.text,
                    url: link.url,
                })
                .collect();
            Some(RancherMinorLine { minor, patches })
        })
        .filter(|line| parse_minor(&line.minor).is_some_and(|m| m >= MIN_SUPPORTED_MINOR))
        .collect();

    if lines.is_empty() {
        return Err(eyre!("no RANCHER x.y.X sections found in sidebar"));
    }
    Ok(lines)
}

/// Parse a `major.minor` sidebar heading (e.g. `"2.15"`) into a comparable pair.
fn parse_minor(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.splitn(2, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Visible text of the heading above the Kubernetes support table. Matched
/// by text rather than `id` since generated slugs are more likely to drift.
const K8S_HEADING_TEXT: &str = "Supported Kubernetes Platforms for Rancher Manager";

/// Parse a single Rancher version's support-matrix page.
///
/// # Errors
/// Returns an error if the release heading (`<h1>`) or the Kubernetes
/// support table can't be located, meaning the page layout has changed
/// upstream.
pub fn parse_release_matrix(html: &str, url: &str) -> Result<RancherReleaseMatrix> {
    let doc = Html::parse_document(html);

    // Pre-2.6 pages head with plain "Rancher vX.Y.Z" rather than "Rancher
    // Manager vX.Y.Z"; accept both so version parsing succeeds even on
    // legacy pages, letting the real reason those get skipped (no inline
    // Kubernetes table below) surface instead of a misleading "no h1" error.
    let h1_sel = Selector::parse("h1").expect("static selector is valid CSS");
    let version = doc
        .select(&h1_sel)
        .next()
        .map(|h1| h1.text().collect::<String>().trim().to_string())
        .and_then(|t| {
            t.strip_prefix("Rancher Manager ")
                .or_else(|| t.strip_prefix("Rancher "))
                .map(str::to_string)
        })
        .ok_or_else(|| eyre!("could not find release heading (h1) on {url}"))?;

    // Older pages heading this section with h2 rather than h3; the visible
    // text stays put even though the CMS authors were inconsistent about level.
    let heading_sel = Selector::parse("h2, h3").expect("static selector is valid CSS");
    let table = doc
        .select(&heading_sel)
        .find(|h| h.text().collect::<String>().trim() == K8S_HEADING_TEXT)
        .and_then(next_table)
        .ok_or_else(|| eyre!("could not find '{K8S_HEADING_TEXT}' table on {url}"))?;

    Ok(RancherReleaseMatrix {
        version,
        url: url.to_string(),
        kubernetes_support: parse_platform_table(table),
    })
}

/// Walk forward from a heading to the next `<table>` sibling, giving up if
/// the next section heading (`h1`/`h2`/`h3`) is reached first.
fn next_table(heading: ElementRef<'_>) -> Option<ElementRef<'_>> {
    heading
        .next_siblings()
        .filter_map(ElementRef::wrap)
        .take_while(|el| !matches!(el.value().name(), "h1" | "h2" | "h3"))
        .find(|el| el.value().name() == "table")
}

/// Extract `Distro | Lowest | Highest | Notes` rows from the platform table.
fn parse_platform_table(table: ElementRef<'_>) -> Vec<KubernetesPlatformSupport> {
    let row_sel = Selector::parse("tbody > tr").expect("static selector is valid CSS");
    let cell_sel = Selector::parse("td").expect("static selector is valid CSS");

    table
        .select(&row_sel)
        .filter_map(|row| {
            let cells: Vec<String> = row
                .select(&cell_sel)
                .map(|td| td.text().collect::<String>().trim().to_string())
                .collect();
            let [distro, lowest_version, highest_version, notes] = cells.try_into().ok()?;
            Some(KubernetesPlatformSupport {
                distro,
                lowest_version,
                highest_version,
                notes: (!notes.is_empty()).then_some(notes),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDEBAR_FIXTURE: &str = r#"
        <div id="accordion">
            <h3 class="sidebar-menu-title t1-paragraph">RANCHER 2.15.X</h3>
            <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                <li class="sidebar-menu-item">
                    <a href="/suse-rancher/support-matrix/all-supported-versions/rancher-v2-15-1" class="active">v2.15.1</a>
                </li>
            </ul></div>
            <h3 class="sidebar-menu-title t1-paragraph">RANCHER 2.14.X</h3>
            <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                <li class="sidebar-menu-item">
                    <a href="/suse-rancher/support-matrix/all-supported-versions/rancher-v2-14-5">v2.14.5</a>
                </li>
                <li class="sidebar-menu-item">
                    <a href="/suse-rancher/support-matrix/all-supported-versions/rancher-v2-14-4">v2.14.4</a>
                </li>
            </ul></div>
            <h3 class="sidebar-menu-title t1-paragraph">RKE1 1.X</h3>
            <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                <li class="sidebar-menu-item"><a href="/rke1/v1-32">v1.32</a></li>
            </ul></div>
        </div>
    "#;

    #[test]
    fn parses_minor_lines_and_skips_non_rancher_sections() {
        let lines = parse_minor_lines(SIDEBAR_FIXTURE).expect("fixture should parse");

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].minor, "2.15");
        assert_eq!(lines[0].latest().unwrap().version, "v2.15.1");
        assert_eq!(lines[1].minor, "2.14");
        assert_eq!(lines[1].latest().unwrap().version, "v2.14.5");
    }

    #[test]
    fn parse_minor_lines_drops_pre_2_7_sections() {
        let fixture = r#"
            <div id="accordion">
                <h3 class="sidebar-menu-title t1-paragraph">RANCHER 2.7.X</h3>
                <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                    <li class="sidebar-menu-item">
                        <a href="/suse-rancher/support-matrix/all-supported-versions/rancher-v2-7-18">v2.7.18</a>
                    </li>
                </ul></div>
                <h3 class="sidebar-menu-title t1-paragraph">RANCHER 2.6.X</h3>
                <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                    <li class="sidebar-menu-item">
                        <a href="/suse-rancher/support-matrix/all-supported-versions/rancher-v2-6-14">v2.6.14</a>
                    </li>
                </ul></div>
                <h3 class="sidebar-menu-title t1-paragraph">RANCHER 2.5.X</h3>
                <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                    <li class="sidebar-menu-item">
                        <a href="/suse-rancher/support-matrix/all-supported-versions/rancher-v2-5-16">v2.5.16</a>
                    </li>
                </ul></div>
            </div>
        "#;

        let lines = parse_minor_lines(fixture).expect("fixture should parse");

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].minor, "2.7");
    }

    const RELEASE_FIXTURE: &str = r#"
        <h1 class="t1-heading-2">Rancher Manager v2.15.1</h1>
        <h3 id="supported-kubernetes-platforms-for-rancher-manager">Supported Kubernetes Platforms for Rancher Manager</h3>
        <p>Important: some caveat text.</p>
        <table class="table no-background">
            <thead><tr><th>Distro</th><th>Lowest Version certified</th><th>Highest Version certified on</th><th>Notes</th></tr></thead>
            <tbody>
                <tr><td><p>RKE2</p></td><td><p>v1.34</p></td><td><p>v1.36</p></td><td><p>See RKE2 docs</p></td></tr>
                <tr><td><p>k3s</p></td><td><p>v1.34</p></td><td><p>v1.36</p></td><td><p></p></td></tr>
            </tbody>
        </table>
        <h3 id="next-section">Rancher Manager Supported Architectures</h3>
        <table><tbody><tr><td><p>unrelated</p></td></tr></tbody></table>
    "#;

    #[test]
    fn parses_release_matrix_and_stops_at_next_heading() {
        let matrix = parse_release_matrix(RELEASE_FIXTURE, "https://example.invalid/")
            .expect("fixture should parse");

        assert_eq!(matrix.version, "v2.15.1");
        assert_eq!(matrix.kubernetes_support.len(), 2);
        assert_eq!(matrix.kubernetes_support[0].distro, "RKE2");
        assert_eq!(matrix.kubernetes_support[0].lowest_version, "v1.34");
        assert_eq!(matrix.kubernetes_support[0].highest_version, "v1.36");
        assert_eq!(
            matrix.kubernetes_support[0].notes.as_deref(),
            Some("See RKE2 docs")
        );
        assert_eq!(matrix.kubernetes_support[1].notes, None);
    }

    const LEGACY_PDF_ONLY_FIXTURE: &str = r#"
        <h1 class="t1-heading-2">Rancher v2.5.16</h1>
        <p>Support matrix for this supported product version.</p>
        <p><a href="/matrix.pdf">View Matrix (pdf) &gt;</a></p>
    "#;

    #[test]
    fn legacy_heading_parses_version_but_reports_missing_table() {
        let err = parse_release_matrix(LEGACY_PDF_ONLY_FIXTURE, "https://example.invalid/")
            .expect_err("pre-2.6 pages have no inline table");

        // The version itself should be recognized (proving the "Rancher "
        // heading form, without "Manager", was matched) even though the
        // page is ultimately rejected for lacking the Kubernetes table.
        assert!(err.to_string().contains(K8S_HEADING_TEXT));
    }

    #[test]
    fn kubernetes_range_prefers_rke2_row() {
        let matrix = parse_release_matrix(RELEASE_FIXTURE, "https://example.invalid/")
            .expect("fixture should parse");

        let range = matrix.kubernetes_range().expect("RKE2 row present");
        assert_eq!(range.distro, "RKE2");
        assert_eq!(range.lowest_version, "v1.34");
        assert_eq!(range.highest_version, "v1.36");
    }
}
