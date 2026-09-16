//! Scraper for SUSE's k3s "all supported versions" support-matrix pages.
//!
//! Same page shape as RKE2 (see [`crate::rke2`]): each minor line's sidebar
//! entry points at one page pinning that line to one exact Kubernetes build.

use crate::component_table;
use crate::sidebar;
use color_eyre::eyre::{Result, eyre};
use scraper::{Html, Selector};
use serde::Serialize;

/// A known-good support-matrix page. Its sidebar enumerates every k3s
/// minor line, so it also serves as the bootstrap index.
pub const ENTRY_POINT: &str =
    "https://www.suse.com/suse-k3s/support-matrix/all-supported-versions/k3s-v1-36";

/// Sidebar accordion heading listing every k3s minor line.
const SIDEBAR_HEADING: &str = "K3S 1.X";

/// One `K3S 1.X` sidebar entry: a single k3s minor line's own page.
#[derive(Debug, Clone, Serialize)]
pub struct K3sMinorLine {
    pub minor: String,
    pub url: String,
}

/// Component versions scraped from one k3s minor line's own page.
#[derive(Debug, Clone, Serialize)]
pub struct K3sRelease {
    pub minor: String,
    pub url: String,
    pub k3s_version: String,
    pub kubernetes_version: String,
}

/// Parse the sidebar `K3S 1.X` section into one entry per minor line.
///
/// # Errors
/// Returns an error if no `K3S 1.X` section is found, which means the
/// sidebar markup has changed upstream.
pub fn parse_minor_lines(html: &str) -> Result<Vec<K3sMinorLine>> {
    let section = sidebar::parse_sections(html)
        .into_iter()
        .find(|section| section.heading == SIDEBAR_HEADING)
        .ok_or_else(|| eyre!("no '{SIDEBAR_HEADING}' section found in sidebar"))?;

    Ok(section
        .links
        .into_iter()
        .map(|link| K3sMinorLine {
            minor: link.text.trim_start_matches('v').to_string(),
            url: link.url,
        })
        .collect())
}

/// Length of the `"k3s v"` heading prefix, matched case-insensitively since
/// pages are inconsistent about capitalizing "k3s".
const HEADING_PREFIX_LEN: usize = "k3s v".len();

/// Parse a single k3s minor line's support-matrix page.
///
/// # Errors
/// Returns an error if the release heading (`<h1>`), the components table,
/// or the `Kubernetes:`/`K3s Version:` fields within it can't be located,
/// meaning the page layout has changed upstream.
pub fn parse_release(html: &str, url: &str) -> Result<K3sRelease> {
    let doc = Html::parse_document(html);

    let h1_sel = Selector::parse("h1").expect("static selector is valid CSS");
    let minor = doc
        .select(&h1_sel)
        .next()
        .map(|h1| h1.text().collect::<String>().trim().to_string())
        .and_then(|t| {
            let prefix_matches = t
                .get(..HEADING_PREFIX_LEN)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("k3s v"));
            prefix_matches.then(|| t[HEADING_PREFIX_LEN..].to_string())
        })
        .ok_or_else(|| eyre!("could not find release heading (h1) on {url}"))?;

    let row_text = component_table::kubernetes_row_text(&doc, url)?;
    let kubernetes_version = component_table::extract_field(&row_text, "Kubernetes:")
        .ok_or_else(|| eyre!("missing 'Kubernetes:' field in components table on {url}"))?;
    let k3s_version = component_table::extract_field(&row_text, "K3s Version:")
        .ok_or_else(|| eyre!("missing 'K3s Version:' field in components table on {url}"))?;

    Ok(K3sRelease {
        minor,
        url: url.to_string(),
        k3s_version,
        kubernetes_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDEBAR_FIXTURE: &str = r#"
        <div id="accordion">
            <h3 class="sidebar-menu-title t1-paragraph">RKE2 1.X</h3>
            <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                <li class="sidebar-menu-item"><a href="/suse-rke2/support-matrix/all-supported-versions/rke2-v1-36">v1.36</a></li>
            </ul></div>
            <h3 class="sidebar-menu-title t1-paragraph">K3S 1.X</h3>
            <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                <li class="sidebar-menu-item">
                    <a href="/suse-k3s/support-matrix/all-supported-versions/k3s-v1-36" class="active">v1.36</a>
                </li>
                <li class="sidebar-menu-item">
                    <a href="/suse-k3s/support-matrix/all-supported-versions/k3s-v1-35">v1.35</a>
                </li>
            </ul></div>
        </div>
    "#;

    #[test]
    fn parses_only_the_k3s_section() {
        let lines = parse_minor_lines(SIDEBAR_FIXTURE).expect("fixture should parse");

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].minor, "1.36");
        assert_eq!(
            lines[0].url,
            "https://www.suse.com/suse-k3s/support-matrix/all-supported-versions/k3s-v1-36"
        );
        assert_eq!(lines[1].minor, "1.35");
    }

    const RELEASE_FIXTURE: &str = r#"
        <h1 class="t1-heading-2">k3s v1.36</h1>
        <h2>This matrix is revised as of v1.36.3+k3s1</h2>
        <table class="table no-background">
            <thead><tr><th>OS</th><th>OS Version</th></tr></thead>
            <tbody><tr><td><p>SLES</p></td><td><p>16.0</p></td></tr></tbody>
        </table>
        <table class="table no-background">
            <thead><tr><th>K8s Components [2,3,4,5]</th><th>Architecture</th></tr></thead>
            <tbody>
                <tr>
                    <td><p>Kubernetes: v1.36.4 K3s Version: v1.36.4+k3s1 Kine: v0.14.11, SQLite: 3.51.1</p></td>
                    <td><p>x86_64 arm64</p></td>
                </tr>
                <tr>
                    <td><p>K3s Version: v1.36.4+k3s1 Kine: v0.14.11</p></td>
                    <td><p>s390x</p></td>
                </tr>
            </tbody>
        </table>
    "#;

    #[test]
    fn parses_release_from_components_table() {
        let release =
            parse_release(RELEASE_FIXTURE, "https://example.invalid/").expect("should parse");

        assert_eq!(release.minor, "1.36");
        assert_eq!(release.kubernetes_version, "v1.36.4");
        assert_eq!(release.k3s_version, "v1.36.4+k3s1");
    }

    const UPPERCASE_HEADING_FIXTURE: &str = r#"
        <h1 class="t1-heading-2">K3s v1.23</h1>
        <table class="table no-background">
            <thead><tr><th>K8s Components [2,3,4,5]</th><th>Architecture</th></tr></thead>
            <tbody>
                <tr>
                    <td><p>Kubernetes: v1.23.17 K3s Version: v1.23.17+k3s1</p></td>
                    <td><p>x86_64</p></td>
                </tr>
            </tbody>
        </table>
    "#;

    #[test]
    fn parses_release_heading_regardless_of_k3s_casing() {
        let release = parse_release(UPPERCASE_HEADING_FIXTURE, "https://example.invalid/")
            .expect("should parse");

        assert_eq!(release.minor, "1.23");
    }
}
