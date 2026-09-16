//! Joins the Rancher, RKE2, and k3s scrapes: for each Rancher release, the
//! specific RKE2/k3s (and bundled Kubernetes) versions at the top and
//! bottom of its certified Kubernetes range.

use crate::k3s::K3sRelease;
use crate::rancher::RancherReleaseMatrix;
use crate::rke2::Rke2Release;
use serde::{Serialize, Serializer};
use tabled::Tabled;

/// One Rancher release joined with the RKE2 and k3s releases at the top
/// and bottom of its certified Kubernetes range.
#[derive(Debug, Clone, Tabled)]
pub struct RancherRke2Row {
    #[tabled(rename = "Rancher")]
    pub rancher_version: String,
    #[tabled(rename = "RKE2 Max")]
    pub rke2_max_version: String,
    #[tabled(rename = "RKE2 Min")]
    pub rke2_min_version: String,
    #[tabled(rename = "K3s Max")]
    pub k3s_max_version: String,
    #[tabled(rename = "K3s Min")]
    pub k3s_min_version: String,
    #[tabled(rename = "K8s Max")]
    pub kubernetes_max_version: String,
    #[tabled(rename = "K8s Min")]
    pub kubernetes_min_version: String,
}

/// Min/max pair nested under `rke2` and `k8s` in JSON output.
#[derive(Serialize)]
struct MinMax<'a> {
    max: &'a str,
    min: &'a str,
}

// Table rendering wants flat columns (via `Tabled`), but JSON output reads
// better nested under `rke2`/`k3s`/`k8s`; this manual impl gives each format
// its own shape from the same fields.
impl Serialize for RancherRke2Row {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Row<'a> {
            rancher_version: &'a str,
            rke2: MinMax<'a>,
            k3s: MinMax<'a>,
            k8s: MinMax<'a>,
        }

        Row {
            rancher_version: &self.rancher_version,
            rke2: MinMax {
                max: &self.rke2_max_version,
                min: &self.rke2_min_version,
            },
            k3s: MinMax {
                max: &self.k3s_max_version,
                min: &self.k3s_min_version,
            },
            k8s: MinMax {
                max: &self.kubernetes_max_version,
                min: &self.kubernetes_min_version,
            },
        }
        .serialize(serializer)
    }
}

/// Build one row per Rancher release, pairing it with the RKE2 and k3s
/// releases at the top and bottom of its certified Kubernetes range.
/// Releases where the range or a matching RKE2/k3s minor can't be resolved
/// are skipped (with a warning) rather than failing the whole run.
#[must_use]
pub fn build_rows(
    rancher_releases: &[RancherReleaseMatrix],
    rke2_releases: &[Rke2Release],
    k3s_releases: &[K3sRelease],
) -> Vec<RancherRke2Row> {
    rancher_releases
        .iter()
        .filter_map(|rancher| row_for_release(rancher, rke2_releases, k3s_releases))
        .collect()
}

fn row_for_release(
    rancher: &RancherReleaseMatrix,
    rke2_releases: &[Rke2Release],
    k3s_releases: &[K3sRelease],
) -> Option<RancherRke2Row> {
    let Some(range) = rancher.kubernetes_range() else {
        eprintln!(
            "no Kubernetes range found for Rancher {}; skipping in query output",
            rancher.version
        );
        return None;
    };
    let (Some(min), Some(max)) = (
        parse_minor(&range.lowest_version),
        parse_minor(&range.highest_version),
    ) else {
        eprintln!(
            "could not parse Kubernetes bounds '{} - {}' for Rancher {}; skipping in query output",
            range.lowest_version, range.highest_version, rancher.version
        );
        return None;
    };

    let Some(rke2_max) = find_release(rke2_releases, max) else {
        eprintln!(
            "no RKE2 release found for minor {}.{} (Rancher {} max); skipping in query output",
            max.0, max.1, rancher.version
        );
        return None;
    };
    let Some(rke2_min) = find_release(rke2_releases, min) else {
        eprintln!(
            "no RKE2 release found for minor {}.{} (Rancher {} min); skipping in query output",
            min.0, min.1, rancher.version
        );
        return None;
    };
    let Some(k3s_max) = find_release(k3s_releases, max) else {
        eprintln!(
            "no k3s release found for minor {}.{} (Rancher {} max); skipping in query output",
            max.0, max.1, rancher.version
        );
        return None;
    };
    let Some(k3s_min) = find_release(k3s_releases, min) else {
        eprintln!(
            "no k3s release found for minor {}.{} (Rancher {} min); skipping in query output",
            min.0, min.1, rancher.version
        );
        return None;
    };

    Some(RancherRke2Row {
        rancher_version: rancher.version.clone(),
        rke2_max_version: rke2_max.rke2_version.clone(),
        rke2_min_version: rke2_min.rke2_version.clone(),
        k3s_max_version: k3s_max.k3s_version.clone(),
        k3s_min_version: k3s_min.k3s_version.clone(),
        kubernetes_max_version: rke2_max.kubernetes_version.clone(),
        kubernetes_min_version: rke2_min.kubernetes_version.clone(),
    })
}

/// A release page pinned to one Kubernetes minor line, shared shape
/// between [`Rke2Release`] and [`K3sRelease`] so [`find_release`] works
/// for either without duplicating the lookup.
trait MinorRelease {
    fn minor(&self) -> &str;
}

impl MinorRelease for Rke2Release {
    fn minor(&self) -> &str {
        &self.minor
    }
}

impl MinorRelease for K3sRelease {
    fn minor(&self) -> &str {
        &self.minor
    }
}

fn find_release<T: MinorRelease>(releases: &[T], minor: (u32, u32)) -> Option<&T> {
    releases
        .iter()
        .find(|release| parse_minor(release.minor()) == Some(minor))
}

/// Parse a `major.minor` version prefix, tolerating an optional leading
/// `v` and any trailing patch/build suffix (e.g. `v1.34`, `1.34.11`).
fn parse_minor(text: &str) -> Option<(u32, u32)> {
    let text = text.strip_prefix('v').unwrap_or(text);
    let mut parts = text.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rancher::KubernetesPlatformSupport;

    fn rancher(version: &str, lowest: &str, highest: &str) -> RancherReleaseMatrix {
        RancherReleaseMatrix {
            version: version.to_string(),
            url: "https://example.invalid/".to_string(),
            kubernetes_support: vec![KubernetesPlatformSupport {
                distro: "RKE2".to_string(),
                lowest_version: lowest.to_string(),
                highest_version: highest.to_string(),
                notes: None,
            }],
        }
    }

    fn rke2(minor: &str, rke2_version: &str, kubernetes_version: &str) -> Rke2Release {
        Rke2Release {
            minor: minor.to_string(),
            url: "https://example.invalid/".to_string(),
            rke2_version: rke2_version.to_string(),
            kubernetes_version: kubernetes_version.to_string(),
        }
    }

    fn k3s(minor: &str, k3s_version: &str, kubernetes_version: &str) -> K3sRelease {
        K3sRelease {
            minor: minor.to_string(),
            url: "https://example.invalid/".to_string(),
            k3s_version: k3s_version.to_string(),
            kubernetes_version: kubernetes_version.to_string(),
        }
    }

    #[test]
    fn pairs_max_and_min_rke2_and_k3s_releases_for_the_rancher_range() {
        let rancher_releases = vec![rancher("v2.15.1", "v1.34", "v1.36")];
        let rke2_releases = vec![
            rke2("1.36", "v1.36.4+rke2r1", "v1.36.4"),
            rke2("1.35", "v1.35.8+rke2r1", "v1.35.8"),
            rke2("1.34", "v1.34.11+rke2r1", "v1.34.11"),
            rke2("1.33", "v1.33.13+rke2r2", "v1.33.13"),
        ];
        let k3s_releases = vec![
            k3s("1.36", "v1.36.4+k3s1", "v1.36.4"),
            k3s("1.35", "v1.35.8+k3s1", "v1.35.8"),
            k3s("1.34", "v1.34.11+k3s1", "v1.34.11"),
            k3s("1.33", "v1.33.13+k3s2", "v1.33.13"),
        ];

        let rows = build_rows(&rancher_releases, &rke2_releases, &k3s_releases);

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.rancher_version, "v2.15.1");
        assert_eq!(row.rke2_max_version, "v1.36.4+rke2r1");
        assert_eq!(row.rke2_min_version, "v1.34.11+rke2r1");
        assert_eq!(row.k3s_max_version, "v1.36.4+k3s1");
        assert_eq!(row.k3s_min_version, "v1.34.11+k3s1");
        assert_eq!(row.kubernetes_max_version, "v1.36.4");
        assert_eq!(row.kubernetes_min_version, "v1.34.11");
    }

    #[test]
    fn skips_release_with_no_kubernetes_range() {
        let rancher_releases = vec![RancherReleaseMatrix {
            version: "v2.5.16".to_string(),
            url: "https://example.invalid/".to_string(),
            kubernetes_support: Vec::new(),
        }];
        let rke2_releases = vec![rke2("1.36", "v1.36.4+rke2r1", "v1.36.4")];
        let k3s_releases = vec![k3s("1.36", "v1.36.4+k3s1", "v1.36.4")];

        assert!(build_rows(&rancher_releases, &rke2_releases, &k3s_releases).is_empty());
    }

    #[test]
    fn skips_release_when_a_bound_has_no_matching_rke2_release() {
        let rancher_releases = vec![rancher("v2.15.1", "v1.20", "v1.36")];
        let rke2_releases = vec![rke2("1.36", "v1.36.4+rke2r1", "v1.36.4")];
        let k3s_releases = vec![k3s("1.36", "v1.36.4+k3s1", "v1.36.4")];

        assert!(build_rows(&rancher_releases, &rke2_releases, &k3s_releases).is_empty());
    }

    #[test]
    fn skips_release_when_a_bound_has_no_matching_k3s_release() {
        let rancher_releases = vec![rancher("v2.15.1", "v1.34", "v1.36")];
        let rke2_releases = vec![
            rke2("1.36", "v1.36.4+rke2r1", "v1.36.4"),
            rke2("1.34", "v1.34.11+rke2r1", "v1.34.11"),
        ];
        let k3s_releases = vec![k3s("1.36", "v1.36.4+k3s1", "v1.36.4")];

        assert!(build_rows(&rancher_releases, &rke2_releases, &k3s_releases).is_empty());
    }
}
