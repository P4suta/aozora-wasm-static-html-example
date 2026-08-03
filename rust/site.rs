use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::json;
use tempfile::Builder;

use crate::artifacts::sha256;
use crate::model::{Edition, Engine, EngineSelector, VerificationReport, VerifiedWork};
use crate::verify::{self, VerifyOptions};

pub const DEFAULT_SIZE_LIMIT: u64 = 900 * 1024 * 1024;
const PAGE_SIZE: usize = 100;

pub struct BuildOptions<'a> {
    pub root: &'a Path,
    pub out_dir: &'a Path,
    pub engine: EngineSelector,
    pub corpus: Option<&'a Path>,
    pub artifacts: Option<&'a Path>,
    pub diagnostics_baseline: Option<&'a Path>,
    pub require_rights_filtered: bool,
    pub size_limit: u64,
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn shell(
    title: &str,
    title_language: Option<&str>,
    description: &str,
    prefix: &str,
    body: &str,
) -> String {
    let title_language = title_language
        .map(|language| format!(" lang=\"{}\"", escape(language)))
        .unwrap_or_default();
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <meta name=\"description\" content=\"{}\">\n  <title{title_language}>{}</title>\n  <link rel=\"stylesheet\" href=\"{prefix}styles/aozora-notation.css\">\n  <link rel=\"stylesheet\" href=\"{prefix}styles/site.css\">\n</head>\n<body>\n{body}\n</body>\n</html>\n",
        escape(description),
        escape(title)
    )
}

fn navigation(prefix: &str) -> String {
    format!(
        "  <header class=\"site-header\"><a href=\"{prefix}index.html\">aozora distribution verification lab</a><nav aria-label=\"Indexes\"><a href=\"{prefix}indexes/authors/index.html\">Authors</a> · <a href=\"{prefix}indexes/gojuon/index.html\">Kana index</a></nav><span>Unofficial</span></header>"
    )
}

fn work_cards(editions: &[Edition], prefix: &str) -> String {
    editions
        .iter()
        .map(|edition| {
            format!(
                "<li class=\"work-card\"><a href=\"{prefix}works/{}.html\"><span class=\"work-number\">{}</span><strong lang=\"ja\">{}</strong><span lang=\"ja\">{}</span></a></li>",
                escape(&edition.edition_id),
                escape(&edition.work_id),
                escape(&edition.title),
                escape(&edition.contributors.iter().map(|value| value.name.as_str()).collect::<Vec<_>>().join("／"))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn index_page(
    editions: &[Edition],
    total: usize,
    page: usize,
    pages: usize,
    engines: &[Engine],
    rights_filtered: bool,
    prefix: &str,
) -> String {
    let corpus = if rights_filtered {
        "rights-filtered corpus"
    } else {
        "development legacy manifest"
    };
    let page_links = (1..=pages)
        .map(|value| {
            if value == page {
                format!("<strong aria-current=\"page\">{value}</strong>")
            } else {
                let href = if value == 1 {
                    format!("{prefix}index.html")
                } else {
                    format!("{prefix}indexes/pages/{value}.html")
                };
                format!("<a href=\"{href}\">{value}</a>")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let body = format!(
        "{}\n  <main class=\"index-main\"><section class=\"summary\"><h1>Editions</h1><p>This site reports {total} editions from the {} that passed verification with <code>{}</code>.</p></section><section aria-labelledby=\"editions-heading\"><div class=\"section-heading\"><h2 id=\"editions-heading\">All editions</h2><span>{total} editions · page {page}/{pages}</span></div><ol class=\"work-grid\">{}</ol><nav class=\"pagination\" aria-label=\"Edition pages\">{page_links}</nav></section><nav class=\"resources\" aria-label=\"Resources\"><a href=\"{prefix}build-report.json\">Build report</a> · <a href=\"https://www.aozora.gr.jp/guide/kijyunn.html\">File handling policy</a></nav></main><footer class=\"site-footer\"><p>Texts and bibliographic data: <a href=\"https://www.aozora.gr.jp/\">Aozora Bunko</a></p></footer>",
        navigation(prefix),
        escape(corpus),
        escape(
            &engines
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" / ")
        ),
        work_cards(editions, prefix)
    );
    shell(
        "aozora distribution verification lab",
        None,
        "Static verification results for aozora distributions and Aozora Bunko editions.",
        prefix,
        &body,
    )
}

fn rights(edition: &Edition) -> String {
    if edition.rights.mode == "rights-filtered" {
        format!(
            "<dl><dt>First publication</dt><dd lang=\"ja\">{}</dd><dt>Parsed years</dt><dd>{}</dd><dt>Reference date / cutoff</dt><dd>{} / {}</dd><dt>Upstream commit</dt><dd><code>{}</code></dd></dl>",
            escape(
                edition
                    .rights
                    .first_publication_raw
                    .as_deref()
                    .unwrap_or_default()
            ),
            edition
                .rights
                .publication_years
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            escape(edition.rights.reference_date.as_deref().unwrap_or_default()),
            edition
                .rights
                .cutoff_year
                .map_or_else(String::new, |value| value.to_string()),
            escape(
                edition
                    .rights
                    .upstream_commit
                    .as_deref()
                    .unwrap_or_default()
            )
        )
    } else {
        "<p>This development legacy manifest is not accepted by the release gate.</p>".into()
    }
}

fn adjacent(edition: Option<&Edition>, rel: &str, marker: &str) -> String {
    edition.map_or_else(
        || "<span aria-hidden=\"true\"></span>".into(),
        |value| {
            format!(
                "<a rel=\"{rel}\" href=\"./{}.html\">{} <span lang=\"ja\">{}</span></a>",
                escape(&value.edition_id),
                escape(marker),
                escape(&value.title)
            )
        },
    )
}

fn work_page(work: &VerifiedWork, previous: Option<&Edition>, next: Option<&Edition>) -> String {
    let edition = &work.edition;
    let diagnostics = if work.canonical.diagnostics.is_empty() {
        "<p>diagnostics: 0</p>".into()
    } else {
        format!(
            "<ol>{}</ol>",
            work.canonical
                .diagnostics
                .iter()
                .map(|value| format!(
                    "<li><code>{}</code></li>",
                    escape(&serde_json::to_string(value).unwrap_or_default())
                ))
                .collect::<Vec<_>>()
                .join("")
        )
    };
    let contributors = edition
        .contributors
        .iter()
        .map(|value| {
            format!(
                "<p class=\"contributor\" lang=\"ja\"><span>{}</span>{}</p>",
                escape(&value.role),
                escape(&value.name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!(
        "{navigation}\n<main><article class=\"book\"><header class=\"book-header\"><p class=\"eyebrow\">Canonical output · {engine}</p><h1 lang=\"ja\">{title}</h1><p class=\"reading\" lang=\"ja\">{reading}</p><div class=\"contributors\">{contributors}</div></header><section class=\"text-section\" aria-labelledby=\"text-heading\"><h2 id=\"text-heading\">Text</h2><div class=\"reader aozora-notation\" lang=\"ja\">{text}</div></section><section class=\"source-information\"><h2>Diagnostics</h2>{diagnostics}</section><section class=\"source-information\"><h2>Rights and sources</h2>{rights}<ul class=\"source-links\"><li><a href=\"{card_url}\">Aozora Bunko work card</a></li><li><a href=\"{archive_url}\">Official text ZIP</a></li><li><a href=\"../sources/{edition_id}.txt\" download>Verified UTF-8 source</a></li><li><a href=\"../reports/{edition_id}.json\">Distribution comparison report</a></li><li><a href=\"https://www.aozora.gr.jp/guide/kijyunn.html\">File handling policy</a></li></ul></section></article><nav class=\"work-navigation\" aria-label=\"Edition navigation\">{previous}<a href=\"../index.html\">All editions</a>{next}</nav></main><footer class=\"site-footer\"><p>aozora {version} · schema {schema}</p><p>Texts and bibliographic data: <a href=\"https://www.aozora.gr.jp/\">Aozora Bunko</a></p></footer>",
        navigation = navigation("../"),
        engine = work.canonical_engine,
        title = escape(&edition.title),
        reading = escape(&edition.reading),
        text = work.canonical.html,
        rights = rights(edition),
        card_url = escape(&edition.card_url),
        archive_url = escape(&edition.archive_url),
        edition_id = escape(&edition.edition_id),
        previous = adjacent(previous, "prev", "←"),
        next = adjacent(next, "next", "→"),
        version = escape(&work.canonical.version),
        schema = work.canonical.schema_version,
    );
    shell(
        &edition.title,
        Some("ja"),
        "Static verification result for an Aozora Bunko edition.",
        "../",
        &body,
    )
}

fn grouped_page(
    title: &str,
    groups: &BTreeMap<String, Vec<Edition>>,
    japanese_group_names: bool,
) -> String {
    let sections = groups
        .iter()
        .map(|(name, editions)| {
            let language = if japanese_group_names {
                " lang=\"ja\""
            } else {
                ""
            };
            format!(
                "<section><div class=\"section-heading\"><h2{language}>{}</h2><span>{} editions</span></div><ol class=\"work-grid\">{}</ol></section>",
                escape(name),
                editions.len(),
                work_cards(editions, "../../")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!(
        "{}<main class=\"index-main\"><h1 class=\"index-title\">{}</h1>{sections}</main><footer class=\"site-footer\"><p>Texts and bibliographic data: <a href=\"https://www.aozora.gr.jp/\">Aozora Bunko</a></p></footer>",
        navigation("../../"),
        escape(title)
    );
    shell(
        &format!("{title} — aozora verification lab"),
        None,
        title,
        "../../",
        &body,
    )
}

fn gojuon(reading: &str) -> &'static str {
    let first = reading.chars().next().unwrap_or('他');
    for (characters, row) in [
        ("あいうえおぁぃぅぇぉ", "A row"),
        ("かきくけこがぎぐげご", "Ka row"),
        ("さしすせそざじずぜぞ", "Sa row"),
        ("たちつてとだぢづでどっ", "Ta row"),
        ("なにぬねの", "Na row"),
        ("はひふへほばびぶべぼぱぴぷぺぽ", "Ha row"),
        ("まみむめも", "Ma row"),
        ("やゆよゃゅょ", "Ya row"),
        ("らりるれろ", "Ra row"),
        ("わをん", "Wa row"),
    ] {
        if characters.contains(first) {
            return row;
        }
    }
    "Other"
}

fn copy_static(root: &Path, staging: &Path) -> Result<()> {
    fs::create_dir_all(staging.join("styles"))?;
    for (source, target) in [
        (root.join("src/site.css"), staging.join("styles/site.css")),
        (
            root.join("vendor/aozora-notation.css"),
            staging.join("styles/aozora-notation.css"),
        ),
        (root.join("LICENSE-APACHE"), staging.join("LICENSE-APACHE")),
        (root.join("LICENSE-MIT"), staging.join("LICENSE-MIT")),
    ] {
        fs::copy(&source, &target)
            .with_context(|| format!("copy {} to {}", source.display(), target.display()))?;
    }
    fs::write(staging.join(".nojekyll"), b"")?;
    Ok(())
}

fn directory_size(path: &Path) -> Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        total += if entry.file_type()?.is_dir() {
            directory_size(&entry.path())?
        } else {
            entry.metadata()?.len()
        };
    }
    Ok(total)
}

fn write_site(
    root: &Path,
    staging: &Path,
    report: &VerificationReport,
    rights_filtered: bool,
) -> Result<()> {
    for directory in [
        "works",
        "sources",
        "reports",
        "indexes/pages",
        "indexes/authors",
        "indexes/gojuon",
    ] {
        fs::create_dir_all(staging.join(directory))?;
    }
    copy_static(root, staging)?;
    let editions: Vec<Edition> = report
        .works
        .iter()
        .map(|work| work.edition.clone())
        .collect();
    for (index, work) in report.works.iter().enumerate() {
        fs::write(
            staging
                .join("works")
                .join(format!("{}.html", work.edition.edition_id)),
            work_page(
                work,
                index.checked_sub(1).and_then(|value| editions.get(value)),
                editions.get(index + 1),
            ),
        )?;
        fs::copy(
            &work.edition.source_path,
            staging
                .join("sources")
                .join(format!("{}.txt", work.edition.edition_id)),
        )?;
        let engine_digests: BTreeMap<Engine, String> = work
            .engines
            .iter()
            .map(|(engine, result)| Ok((*engine, sha256(serde_json::to_vec(result)?))))
            .collect::<Result<_>>()?;
        let value = json!({
            "schemaVersion": 1,
            "edition": work.edition,
            "canonicalEngine": work.canonical_engine,
            "canonical": work.canonical,
            "engineResultSha256": engine_digests,
            "improvements": work.improvements,
        });
        let mut output = serde_json::to_vec_pretty(&value)?;
        output.push(b'\n');
        fs::write(
            staging
                .join("reports")
                .join(format!("{}.json", work.edition.edition_id)),
            output,
        )?;
    }
    let diagnostics = report
        .works
        .iter()
        .map(|work| {
            (
                work.edition.edition_id.clone(),
                work.canonical.diagnostics.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics_output = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 1,
        "corpusManifestSha256": report.corpus_manifest_sha256,
        "entries": diagnostics,
    }))?;
    diagnostics_output.push(b'\n');
    fs::write(
        staging.join("diagnostics-baseline.json"),
        diagnostics_output,
    )?;
    let pages = editions.len().div_ceil(PAGE_SIZE).max(1);
    for page in 1..=pages {
        let start = (page - 1) * PAGE_SIZE;
        let end = (start + PAGE_SIZE).min(editions.len());
        let prefix = if page == 1 { "./" } else { "../../" };
        let path = if page == 1 {
            staging.join("index.html")
        } else {
            staging.join("indexes/pages").join(format!("{page}.html"))
        };
        fs::write(
            path,
            index_page(
                &editions[start..end],
                editions.len(),
                page,
                pages,
                &report.engines,
                rights_filtered,
                prefix,
            ),
        )?;
    }
    let mut authors: BTreeMap<String, Vec<Edition>> = BTreeMap::new();
    let mut readings: BTreeMap<String, Vec<Edition>> = BTreeMap::new();
    for edition in editions {
        authors
            .entry(
                edition
                    .contributors
                    .iter()
                    .map(|value| value.name.as_str())
                    .collect::<Vec<_>>()
                    .join("／"),
            )
            .or_default()
            .push(edition.clone());
        readings
            .entry(gojuon(&edition.reading).into())
            .or_default()
            .push(edition);
    }
    fs::write(
        staging.join("indexes/authors/index.html"),
        grouped_page("Authors", &authors, true),
    )?;
    fs::write(
        staging.join("indexes/gojuon/index.html"),
        grouped_page("Kana index", &readings, false),
    )?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildReport<'a> {
    schema_version: u32,
    corpus_manifest_sha256: &'a str,
    artifacts_manifest_sha256: Option<&'a str>,
    aozora_commit: Option<&'a str>,
    corpus_mode: &'static str,
    engines: &'a [Engine],
    editions: usize,
    size_bytes: u64,
    size_limit_bytes: u64,
    works: Vec<BuildWork<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildWork<'a> {
    edition_id: &'a str,
    source_sha256: &'a str,
    html_sha256: String,
    report: String,
}

fn write_build_report(
    staging: &Path,
    verification: &VerificationReport,
    rights_filtered: bool,
    limit: u64,
) -> Result<u64> {
    let base = directory_size(staging)?;
    let mut total = base;
    loop {
        let report = BuildReport {
            schema_version: 1,
            corpus_manifest_sha256: &verification.corpus_manifest_sha256,
            artifacts_manifest_sha256: verification.artifacts_manifest_sha256.as_deref(),
            aozora_commit: verification.aozora_commit.as_deref(),
            corpus_mode: if rights_filtered {
                "rights-filtered"
            } else {
                "development-legacy"
            },
            engines: &verification.engines,
            editions: verification.works.len(),
            size_bytes: total,
            size_limit_bytes: limit,
            works: verification
                .works
                .iter()
                .map(|work| BuildWork {
                    edition_id: &work.edition.edition_id,
                    source_sha256: &work.edition.source_sha256,
                    html_sha256: sha256(&work.canonical.html),
                    report: format!("./reports/{}.json", work.edition.edition_id),
                })
                .collect(),
        };
        let mut bytes = serde_json::to_vec_pretty(&report)?;
        bytes.push(b'\n');
        let next = base + u64::try_from(bytes.len()).context("build report size overflow")?;
        fs::write(staging.join("build-report.json"), bytes)?;
        if next == total {
            return Ok(next);
        }
        total = next;
    }
}

fn replace(staging: PathBuf, target: &Path) -> Result<()> {
    let parent = target.parent().context("output directory has no parent")?;
    let backup = Builder::new().prefix(".lab-backup-").tempdir_in(parent)?;
    let previous = backup.path().join("previous");
    let moved = match fs::rename(target, &previous) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error).context("move previous output"),
    };
    if let Err(error) = fs::rename(&staging, target) {
        if moved {
            fs::rename(&previous, target).context("restore previous output")?;
        }
        return Err(error).context("publish staged site");
    }
    Ok(())
}

pub fn build(
    options: &BuildOptions<'_>,
    mut progress: impl FnMut(&str),
) -> Result<VerificationReport> {
    let output = options.root.join(options.out_dir);
    if output == options.root {
        bail!("output directory must not be the project root");
    }
    let loaded = crate::corpus::load(options.root, options.corpus)?;
    let report = verify::run(
        &VerifyOptions {
            root: options.root,
            engine: options.engine,
            scope: crate::model::Scope::Full,
            shard: None,
            corpus: options.corpus,
            artifacts: options.artifacts,
            diagnostics_baseline: options.diagnostics_baseline,
            require_rights_filtered: options.require_rights_filtered,
        },
        &mut progress,
    )?;
    let parent = output.parent().context("output directory has no parent")?;
    fs::create_dir_all(parent)?;
    let staging_dir = Builder::new().prefix(".lab-stage-").tempdir_in(parent)?;
    write_site(
        options.root,
        staging_dir.path(),
        &report,
        loaded.rights_filtered,
    )?;
    let size = write_build_report(
        staging_dir.path(),
        &report,
        loaded.rights_filtered,
        options.size_limit,
    )?;
    if size > options.size_limit {
        bail!(
            "site size {size} bytes exceeds the {} byte release gate",
            options.size_limit
        );
    }
    let staging = staging_dir.keep();
    replace(staging, &output)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;

    use anyhow::Result;
    use tempfile::tempdir;

    use super::{BuildOptions, DEFAULT_SIZE_LIMIT, build, gojuon};
    use crate::artifacts::sha256;
    use crate::model::EngineSelector;

    fn hashes(path: &Path, prefix: &Path, result: &mut BTreeMap<String, String>) -> Result<()> {
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let child = entry.path();
            if entry.file_type()?.is_dir() {
                hashes(&child, prefix, result)?;
            } else {
                result.insert(
                    child.strip_prefix(prefix)?.to_string_lossy().into_owned(),
                    sha256(fs::read(child)?),
                );
            }
        }
        Ok(())
    }

    #[test]
    fn full_site_is_reproducible_and_failures_preserve_output() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let temporary = tempdir()?;
        let first = temporary.path().join("first");
        let second = temporary.path().join("second");
        for output in [&first, &second] {
            build(
                &BuildOptions {
                    root,
                    out_dir: output,
                    engine: EngineSelector::Wasm,
                    corpus: None,
                    artifacts: None,
                    diagnostics_baseline: None,
                    require_rights_filtered: false,
                    size_limit: DEFAULT_SIZE_LIMIT,
                },
                |_| {},
            )?;
        }
        let mut first_hashes = BTreeMap::new();
        let mut second_hashes = BTreeMap::new();
        hashes(&first, &first, &mut first_hashes)?;
        hashes(&second, &second, &mut second_hashes)?;
        assert_eq!(first_hashes, second_hashes);
        assert_eq!(
            first_hashes
                .keys()
                .filter(|name| name.starts_with("works/"))
                .count(),
            10
        );
        let index = fs::read_to_string(first.join("index.html"))?;
        assert!(!index.contains("<script"));
        assert!(index.contains("<html lang=\"en\">"));
        assert!(index.contains(">Authors</a>"));
        assert!(index.contains(">Kana index</a>"));
        assert!(index.contains(">Editions</h1>"));
        assert!(index.contains(">All editions</h2>"));
        assert!(index.contains(">Build report</a>"));
        assert!(index.contains("<strong lang=\"ja\">"));
        assert!(!index.contains("class=\"hero\""));
        assert!(!index.contains("class=\"about\""));
        assert!(!index.contains("収録版"));

        let Some(work_name) = first_hashes
            .keys()
            .find(|name| name.starts_with("works/") && name.ends_with(".html"))
        else {
            anyhow::bail!("generated site lacks a work page");
        };
        let work = fs::read_to_string(first.join(work_name))?;
        assert!(work.contains("<html lang=\"en\">"));
        assert!(work.contains("<title lang=\"ja\">"));
        assert!(work.contains("<h1 lang=\"ja\">"));
        assert!(work.contains("<div class=\"reader aozora-notation\" lang=\"ja\">"));
        assert!(work.contains(">Text</h2>"));
        assert!(work.contains(">Diagnostics</h2>"));
        assert!(work.contains(">Rights and sources</h2>"));
        assert!(work.contains(">All editions</a>"));
        assert!(!work.contains("class=\"ornament\""));
        assert!(!work.contains("aria-label=\"本文\""));

        assert_eq!(gojuon("あいびき"), "A row");
        assert_eq!(gojuon("くものいと"), "Ka row");
        assert_eq!(gojuon(""), "Other");

        let sentinel = temporary.path().join("preserved");
        fs::create_dir(&sentinel)?;
        fs::write(sentinel.join("sentinel"), "keep")?;
        let failed = build(
            &BuildOptions {
                root,
                out_dir: &sentinel,
                engine: EngineSelector::Wasm,
                corpus: None,
                artifacts: None,
                diagnostics_baseline: None,
                require_rights_filtered: false,
                size_limit: 1,
            },
            |_| {},
        );
        assert!(failed.is_err());
        assert_eq!(fs::read_to_string(sentinel.join("sentinel"))?, "keep");
        Ok(())
    }
}
