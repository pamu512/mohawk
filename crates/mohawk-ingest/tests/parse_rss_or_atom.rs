use mohawk_ingest::parse_rss_or_atom;

const CISA_ADVISORIES_RSS: &str = include_str!("fixtures/cisa_advisories.rss.xml");
const SYNTHETIC_ATOM: &str = include_str!("fixtures/synthetic.atom.xml");

#[test]
fn cisa_fixture_is_namespaced_rss_with_escaped_html_descriptions() {
    assert!(
        CISA_ADVISORIES_RSS.contains(r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#),
        "fixture must keep the Dublin Core namespace from the public CISA feed"
    );
    assert!(
        CISA_ADVISORIES_RSS.contains(r#"version="2.0""#),
        "fixture must keep RSS 2.0"
    );
    assert!(
        CISA_ADVISORIES_RSS.contains("&lt;p&gt;"),
        "fixture descriptions must stay entity-escaped HTML, not a synthetic unescaped SAMPLE_RSS"
    );
    assert!(
        !CISA_ADVISORIES_RSS.contains("BIN Velocity Alert"),
        "do not relabel the in-tree SAMPLE_RSS as CISA"
    );
}

#[test]
fn parse_rss_or_atom_reads_trimmed_cisa_advisories_rss() {
    let items = parse_rss_or_atom(CISA_ADVISORIES_RSS).expect("trimmed CISA RSS");
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].title, "Hitachi Energy MicroSCADA Pro/X SYS600");
    assert_eq!(
        items[1].title,
        "Schneider Electric EcoStruxure Control Expert, EcoStruxure Process Expert, and Modicon M340, M580 and M580 Safety PLCs"
    );
    assert_eq!(
        items[2].title,
        "Schneider Electric PowerLogic PM55xx and PowerLogic PM8ECC"
    );
    assert!(
        items[0].body.contains("&lt;p&gt;"),
        "parser must return the escaped HTML description body, not a decoded rewrite"
    );
    assert!(items[0].body.contains("EXECUTIVE SUMMARY"));
    assert!(items[0].body.contains("MicroSCADA"));
}

#[test]
fn parse_rss_or_atom_reads_synthetic_atom_feed() {
    assert!(
        SYNTHETIC_ATOM.contains("<feed"),
        "Atom fixture must be a <feed>"
    );
    assert!(
        SYNTHETIC_ATOM.to_ascii_lowercase().contains("synthetic"),
        "Atom fixture must be clearly synthetic"
    );
    assert!(
        !SYNTHETIC_ATOM.to_ascii_lowercase().contains("<rss"),
        "Atom fixture must not be RSS"
    );

    let items = parse_rss_or_atom(SYNTHETIC_ATOM).expect("synthetic Atom");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Synthetic Atom Entry One");
    assert!(items[0].body.contains("parser coverage"));
    assert_eq!(items[1].title, "Synthetic Atom Entry Two");
    assert!(items[1].body.contains("Synthetic Atom HTML content."));
}
