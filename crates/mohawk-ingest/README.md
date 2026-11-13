# mohawk-ingest

Pure feed parsing, HTML stripping, prose chunking, HTTPS allowlist
validation, and sync-schedule math used by Mohawk's Threat Intel Desk.

Network fetch, persistence, and LLM synthesis stay in the app
(`src-tauri/src/engine/scraper.rs`). This crate has no HTTP client.
`cargo test --offline -p mohawk-ingest` is offline on purpose.

## Parser fixtures

- `tests/fixtures/cisa_advisories.rss.xml` — trimmed public CISA advisories RSS
  (`https://www.cisa.gov/cybersecurity-advisories/all.xml`): RSS 2.0, `xmlns:dc`,
  escaped-HTML descriptions. Tests drive this file through `parse_rss_or_atom`.
- `tests/fixtures/synthetic.atom.xml` — small synthetic Atom feed so
  `parse_rss_or_atom` / `parse_atom_entries` have coverage. None of the
  allowlisted live feeds were Atom on 2026-11-13 probes; the working ones were
  RSS 2.0.

The in-tree `SAMPLE_RSS` unit fixture is still synthetic and is not a CISA stand-in.

## Allowlist: FTC Consumer Safety Briefings is not a live feed

The allowlisted FTC URL
`https://www.ftc.gov/feeds/press-release-consumer-safety/rss.xml`
(desk label **Consumer Safety Briefings**) is not serving RSS. Independent
probes on 2026-11-13 (Asia/Hong_Kong) observed:

- HTTP 404 HTML ("Page Not Found")
- HTTP 403 Akamai WAF HTML ("request resembles an abusive automated request")

Neither status is treated as the only observed failure. The allowlist entry is
unchanged; this crate does not guess a replacement URL. Several historical
`/feeds/` URLs and the old hub path also 404'd; no substitute is recorded here.

Green ingest tests prove the offline parser and checked-in fixtures. They do
not prove desk sources (including FORCE SYNC) are healthy.
