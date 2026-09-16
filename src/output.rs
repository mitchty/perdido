//! Table/JSON rendering for the scraped support matrices.

use crate::k3s::K3sRelease;
use crate::matrix::RancherRke2Row;
use crate::rancher::RancherReleaseMatrix;
use crate::rke2::Rke2Release;
use color_eyre::eyre::Result;
use serde::Serialize;
use tabled::settings::Style;
use tabled::{Table, Tabled};

/// One row of the `rancher` command's summary output.
#[derive(Debug, Clone, Serialize, Tabled)]
pub struct RancherVersionRow {
    #[tabled(rename = "Rancher Version")]
    pub rancher_version: String,
    #[tabled(rename = "Min K8s")]
    pub min_k8s: String,
    #[tabled(rename = "Max K8s")]
    pub max_k8s: String,
}

impl From<&RancherReleaseMatrix> for RancherVersionRow {
    fn from(matrix: &RancherReleaseMatrix) -> Self {
        let range = matrix.kubernetes_range();
        Self {
            rancher_version: matrix.version.clone(),
            min_k8s: range.map_or_else(String::new, |k| k.lowest_version.clone()),
            max_k8s: range.map_or_else(String::new, |k| k.highest_version.clone()),
        }
    }
}

/// Print the latest-per-minor-line Rancher summary as either a table or JSON.
///
/// # Errors
/// Returns an error if JSON serialization fails.
pub fn print_rancher_summary(releases: &[RancherReleaseMatrix], as_json: bool) -> Result<()> {
    let rows: Vec<RancherVersionRow> = releases.iter().map(RancherVersionRow::from).collect();
    print_rows(rows, as_json)
}

/// One row of the `rke2` command's summary output.
#[derive(Debug, Clone, Serialize, Tabled)]
pub struct Rke2VersionRow {
    #[tabled(rename = "RKE2 Version")]
    pub rke2_version: String,
    #[tabled(rename = "Kubernetes Version")]
    pub kubernetes_version: String,
}

impl From<&Rke2Release> for Rke2VersionRow {
    fn from(release: &Rke2Release) -> Self {
        Self {
            rke2_version: release.rke2_version.clone(),
            kubernetes_version: release.kubernetes_version.clone(),
        }
    }
}

/// Print the per-minor-line RKE2 summary as either a table or JSON.
///
/// # Errors
/// Returns an error if JSON serialization fails.
pub fn print_rke2_summary(releases: &[Rke2Release], as_json: bool) -> Result<()> {
    let rows: Vec<Rke2VersionRow> = releases.iter().map(Rke2VersionRow::from).collect();
    print_rows(rows, as_json)
}

/// One row of the `k3s` command's summary output.
#[derive(Debug, Clone, Serialize, Tabled)]
pub struct K3sVersionRow {
    #[tabled(rename = "K3s Version")]
    pub k3s_version: String,
    #[tabled(rename = "Kubernetes Version")]
    pub kubernetes_version: String,
}

impl From<&K3sRelease> for K3sVersionRow {
    fn from(release: &K3sRelease) -> Self {
        Self {
            k3s_version: release.k3s_version.clone(),
            kubernetes_version: release.kubernetes_version.clone(),
        }
    }
}

/// Print the per-minor-line k3s summary as either a table or JSON.
///
/// # Errors
/// Returns an error if JSON serialization fails.
pub fn print_k3s_summary(releases: &[K3sRelease], as_json: bool) -> Result<()> {
    let rows: Vec<K3sVersionRow> = releases.iter().map(K3sVersionRow::from).collect();
    print_rows(rows, as_json)
}

/// Print the joined Rancher/RKE2 rows (see [`crate::matrix`]) as either a
/// table or JSON.
///
/// # Errors
/// Returns an error if JSON serialization fails.
pub fn print_query_summary(rows: &[RancherRke2Row], as_json: bool) -> Result<()> {
    print_rows(rows.to_vec(), as_json)
}

fn print_rows<T>(rows: Vec<T>, as_json: bool) -> Result<()>
where
    T: Serialize + Tabled,
{
    if as_json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        let mut table = Table::new(rows);
        table.with(Style::empty());
        println!("{table}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rancher::KubernetesPlatformSupport;

    #[test]
    fn row_falls_back_to_first_distro_when_rke2_absent() {
        let matrix = RancherReleaseMatrix {
            version: "v2.15.1".to_string(),
            url: "https://example.invalid/".to_string(),
            kubernetes_support: vec![KubernetesPlatformSupport {
                distro: "AKS".to_string(),
                lowest_version: "v1.34".to_string(),
                highest_version: "v1.36".to_string(),
                notes: None,
            }],
        };

        let row = RancherVersionRow::from(&matrix);
        assert_eq!(row.rancher_version, "v2.15.1");
        assert_eq!(row.min_k8s, "v1.34");
        assert_eq!(row.max_k8s, "v1.36");
    }
}
