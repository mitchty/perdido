//! Parsing for the support-matrix sidebar accordion shared by every product
//! family's pages (Rancher, RKE2, K3s, Harvester, ...): it's the same
//! CMS-rendered block regardless of which page you land on, so any one page
//! doubles as an index for every other product's releases too.

use scraper::{Html, Selector};

/// Root of every support-matrix URL; sidebar `<a href>` values are relative to this.
pub const BASE_URL: &str = "https://www.suse.com";

/// One `<a href>` entry from a sidebar accordion section.
#[derive(Debug, Clone)]
pub struct SidebarLink {
    pub text: String,
    pub url: String,
}

/// One accordion section: its heading (e.g. `"RANCHER 2.15.X"`, `"RKE2 1.X"`)
/// and the links listed under it, in document (newest-first) order.
#[derive(Debug, Clone)]
pub struct SidebarSection {
    pub heading: String,
    pub links: Vec<SidebarLink>,
}

/// Parse every `#accordion` heading/list pair, in document order.
pub fn parse_sections(html: &str) -> Vec<SidebarSection> {
    let doc = Html::parse_document(html);
    // scraper has no sibling combinator support, so headings and their
    // version lists are matched together, in document order, and paired up
    // by hand below instead.
    let section_sel = Selector::parse(
        "#accordion > h3.sidebar-menu-title, #accordion > div.sidebar-menu-list-wrap",
    )
    .expect("static selector is valid CSS");
    let link_sel = Selector::parse("a[href]").expect("static selector is valid CSS");

    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;

    for el in doc.select(&section_sel) {
        match el.value().name() {
            "h3" => current_heading = Some(el.text().collect::<String>().trim().to_string()),
            "div" => {
                let Some(heading) = current_heading.take() else {
                    continue;
                };
                let links = el
                    .select(&link_sel)
                    .map(|a| SidebarLink {
                        text: a.text().collect::<String>().trim().to_string(),
                        url: format!("{BASE_URL}{}", a.value().attr("href").unwrap_or_default()),
                    })
                    .collect();
                sections.push(SidebarSection { heading, links });
            }
            _ => {}
        }
    }

    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
        <div id="accordion">
            <h3 class="sidebar-menu-title t1-paragraph">RANCHER 2.15.X</h3>
            <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                <li class="sidebar-menu-item">
                    <a href="/suse-rancher/support-matrix/all-supported-versions/rancher-v2-15-1" class="active">v2.15.1</a>
                </li>
            </ul></div>
            <h3 class="sidebar-menu-title t1-paragraph">RKE2 1.X</h3>
            <div class="sidebar-menu-list-wrap"><ul class="sidebar-menu-list">
                <li class="sidebar-menu-item">
                    <a href="/suse-rke2/support-matrix/all-supported-versions/rke2-v1-36">v1.36</a>
                </li>
                <li class="sidebar-menu-item">
                    <a href="/suse-rke2/support-matrix/all-supported-versions/rke2-v1-35">v1.35</a>
                </li>
            </ul></div>
        </div>
    "#;

    #[test]
    fn pairs_each_heading_with_its_following_list() {
        let sections = parse_sections(FIXTURE);

        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].heading, "RANCHER 2.15.X");
        assert_eq!(sections[0].links[0].text, "v2.15.1");
        assert_eq!(
            sections[0].links[0].url,
            "https://www.suse.com/suse-rancher/support-matrix/all-supported-versions/rancher-v2-15-1"
        );
        assert_eq!(sections[1].heading, "RKE2 1.X");
        assert_eq!(sections[1].links.len(), 2);
        assert_eq!(sections[1].links[1].text, "v1.35");
    }
}
