//! Shared scraping helpers for the "K8s Components" table that appears on
//! both RKE2 and k3s support-matrix pages: same table shape, just a
//! different label for each distro's own version field.

use color_eyre::eyre::{Result, eyre};
use scraper::{Html, Selector};

/// Column header text identifying the Kubernetes/component-versions table.
const HEADER_PREFIX: &str = "K8s Components";

/// Find the components table (identified by its own header text, since
/// there's no distinct heading directly above it) and return the text of
/// whichever row actually carries the `Kubernetes:` field: multi-arch pages
/// list one row per architecture, and only the first repeats it.
///
/// # Errors
/// Returns an error if the table or a `Kubernetes:`-bearing row can't be
/// found, meaning the page layout has changed upstream.
pub fn kubernetes_row_text(doc: &Html, url: &str) -> Result<String> {
    let table_sel = Selector::parse("table").expect("static selector is valid CSS");
    let th_sel = Selector::parse("thead th").expect("static selector is valid CSS");
    let row_sel = Selector::parse("tbody > tr").expect("static selector is valid CSS");
    let cell_sel = Selector::parse("td").expect("static selector is valid CSS");

    let table = doc
        .select(&table_sel)
        .find(|table| {
            table
                .select(&th_sel)
                .next()
                .is_some_and(|th| th.text().collect::<String>().starts_with(HEADER_PREFIX))
        })
        .ok_or_else(|| eyre!("could not find '{HEADER_PREFIX}' table on {url}"))?;

    table
        .select(&row_sel)
        .find_map(|row| {
            let text = row.select(&cell_sel).next()?.text().collect::<String>();
            text.contains("Kubernetes:").then_some(text)
        })
        .ok_or_else(|| eyre!("no row with a 'Kubernetes:' field in the components table on {url}"))
}

/// Pull the single whitespace-delimited value following a `"Label:"` marker
/// out of the components table's free-text cell (there's no structured
/// per-field markup to select against; it's one paragraph of
/// space-separated `Label: value` pairs).
pub fn extract_field(text: &str, label: &str) -> Option<String> {
    let after = text.split_once(label)?.1.trim_start();
    after.split_whitespace().next().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_field_stops_at_next_whitespace() {
        let text = "Kubernetes: v1.36.4 RKE2 Version: v1.36.4+rke2r1";
        assert_eq!(
            extract_field(text, "Kubernetes:").as_deref(),
            Some("v1.36.4")
        );
        assert_eq!(
            extract_field(text, "RKE2 Version:").as_deref(),
            Some("v1.36.4+rke2r1")
        );
        assert_eq!(extract_field(text, "Missing:"), None);
    }
}
