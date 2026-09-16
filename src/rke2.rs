//! Scraper for SUSE's RKE2 "all supported versions" support-matrix pages.
//!
//! Unlike Rancher (where a minor line spans many patch releases with a
//! range of certified Kubernetes versions), each RKE2 minor line's sidebar
//! entry already points at one page, and that page pins the line to one
//! exact Kubernetes build rather than a range.

use crate::component_table;
use crate::sidebar;
use color_eyre::eyre::{Result, eyre};
use scraper::{Html, Selector};
use serde::Serialize;

/// A known-good support-matrix page. Its sidebar enumerates every RKE2
/// minor line, so it also serves as the bootstrap index.
pub const ENTRY_POINT: &str =
    "https://www.suse.com/suse-rke2/support-matrix/all-supported-versions/rke2-v1-36";

/// Sidebar accordion heading listing every RKE2 minor line.
const SIDEBAR_HEADING: &str = "RKE2 1.X";

/// One `RKE2 1.X` sidebar entry: a single RKE2 minor line's own page.
#[derive(Debug, Clone, Serialize)]
pub struct Rke2MinorLine {
    pub minor: String,
    pub url: String,
}

/// Component versions scraped from one RKE2 minor line's own page.
#[derive(Debug, Clone, Serialize)]
pub struct Rke2Release {
    pub minor: String,
    pub url: String,
    pub rke2_version: String,
    pub kubernetes_version: String,
}

/// Parse the sidebar `RKE2 1.X` section into one entry per minor line.
///
/// # Errors
/// Returns an error if no `RKE2 1.X` section is found, which means the
/// sidebar markup has changed upstream.
pub fn parse_minor_lines(html: &str) -> Result<Vec<Rke2MinorLine>> {
    let section = sidebar::parse_sections(html)
        .into_iter()
        .find(|section| section.heading == SIDEBAR_HEADING)
        .ok_or_else(|| eyre!("no '{SIDEBAR_HEADING}' section found in sidebar"))?;

    Ok(section
        .links
        .into_iter()
        .map(|link| Rke2MinorLine {
            minor: link.text.trim_start_matches('v').to_string(),
            url: link.url,
        })
        .collect())
}

/// Parse a single RKE2 minor line's support-matrix page.
///
/// # Errors
/// Returns an error if the release heading (`<h1>`), the components table,
/// or the `Kubernetes:`/`RKE2 Version:` fields within it can't be located,
/// meaning the page layout has changed upstream.
pub fn parse_release(html: &str, url: &str) -> Result<Rke2Release> {
    let doc = Html::parse_document(html);

    let h1_sel = Selector::parse("h1").expect("static selector is valid CSS");
    let minor = doc
        .select(&h1_sel)
        .next()
        .map(|h1| h1.text().collect::<String>().trim().to_string())
        .and_then(|t| t.strip_prefix("RKE2 v").map(str::to_string))
        .ok_or_else(|| eyre!("could not find release heading (h1) on {url}"))?;

    let row_text = component_table::kubernetes_row_text(&doc, url)?;
    let kubernetes_version = component_table::extract_field(&row_text, "Kubernetes:")
        .ok_or_else(|| eyre!("missing 'Kubernetes:' field in components table on {url}"))?;
    let rke2_version = component_table::extract_field(&row_text, "RKE2 Version:")
        .ok_or_else(|| eyre!("missing 'RKE2 Version:' field in components table on {url}"))?;

    Ok(Rke2Release {
        minor,
        url: url.to_string(),
        rke2_version,
        kubernetes_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDEBAR_FIXTURE: &str = r#"
        <div id="accordion">
            <h3 class="sidebar-menu-title t1-paragraph">RANCHER 2.15.X</h3>
            <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                <li class="sidebar-menu-item"><a href="/rancher/v2-15-1">v2.15.1</a></li>
            </ul></div>
            <h3 class="sidebar-menu-title t1-paragraph">RKE2 1.X</h3>
            <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                <li class="sidebar-menu-item">
                    <a href="/suse-rke2/support-matrix/all-supported-versions/rke2-v1-36" class="active">v1.36</a>
                </li>
                <li class="sidebar-menu-item">
                    <a href="/suse-rke2/support-matrix/all-supported-versions/rke2-v1-35">v1.35</a>
                </li>
            </ul></div>
        </div>
    "#;

    #[test]
    fn parses_only_the_rke2_section() {
        let lines = parse_minor_lines(SIDEBAR_FIXTURE).expect("fixture should parse");

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].minor, "1.36");
        assert_eq!(
            lines[0].url,
            "https://www.suse.com/suse-rke2/support-matrix/all-supported-versions/rke2-v1-36"
        );
        assert_eq!(lines[1].minor, "1.35");
    }

    const RELEASE_FIXTURE: &str = r#"
        <h1 class="t1-heading-2">RKE2 v1.36</h1>
        <h2>This matrix is revised as of v1.36.3+rke2r1</h2>
        <table class="table no-background">
            <thead><tr><th>OS</th><th>OS Version</th></tr></thead>
            <tbody><tr><td><p>SLES</p></td><td><p>16.0</p></td></tr></tbody>
        </table>
        <table class="table no-background">
            <thead><tr><th>K8s Components [2,3,4,5]</th><th>Architecture</th></tr></thead>
            <tbody>
                <tr>
                    <td><p>Kubernetes: v1.36.4 RKE2 Version: v1.36.4+rke2r1 Etcd: v3.6.14-k3s1 CNI: Canal (Flannel: v0.28.9, Calico: v3.32.1)</p></td>
                    <td><p>x86_64 arm64</p></td>
                </tr>
                <tr>
                    <td><p>RKE2 Version: v1.36.4+rke2r1 Etcd: v3.6.14-k3s1</p></td>
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
        assert_eq!(release.rke2_version, "v1.36.4+rke2r1");
    }
}
