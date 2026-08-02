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

fn shell(title: &str, description: &str, prefix: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"ja\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <meta name=\"description\" content=\"{}\">\n  <title>{}</title>\n  <link rel=\"stylesheet\" href=\"{prefix}styles/aozora-notation.css\">\n  <link rel=\"stylesheet\" href=\"{prefix}styles/site.css\">\n</head>\n<body>\n{body}\n</body>\n</html>\n",
        escape(description),
        escape(title)
    )
}

fn navigation(prefix: &str) -> String {
    format!(
        "  <header class=\"site-header\"><a href=\"{prefix}index.html\">aozora distribution verification lab</a><nav aria-label=\"索引\"><a href=\"{prefix}indexes/authors/index.html\">著者</a> · <a href=\"{prefix}indexes/gojuon/index.html\">五十音</a></nav><span>非公式</span></header>"
    )
}

fn work_cards(editions: &[Edition], prefix: &str) -> String {
    editions
        .iter()
        .map(|edition| {
            format!(
                "<li class=\"work-card\"><a href=\"{prefix}works/{}.html\"><span class=\"work-number\">{}</span><strong>{}</strong><span>{}</span></a></li>",
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
    let mode = if rights_filtered {
        "固定した権利判定済みコーパス"
    } else {
        "開発用legacy manifest（正式公開ゲートでは使用不可）"
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
        "{}\n  <main class=\"index-main\"><section class=\"hero\"><p class=\"eyebrow\">aozora · real-work parity gate</p><h1>実作品で、<br>七つの配布面を測る。</h1><p>{}の本文を用い、全projectionの一致を検証した静的サイトです。</p><p>検証面: <code>{}</code>。クライアントJavaScriptは配信しません。</p></section><section aria-labelledby=\"works-heading\"><div class=\"section-heading\"><h2 id=\"works-heading\">収録版</h2><span>{total} editions · page {page}/{pages}</span></div><ol class=\"work-grid\">{}</ol><nav class=\"pagination\" aria-label=\"ページ分割された作品索引\">{page_links}</nav></section><section class=\"about\"><h2>検証資料</h2><p><a href=\"{prefix}build-report.json\">ビルドレポート</a> · <a href=\"https://www.aozora.gr.jp/guide/kijyunn.html\">青空文庫収録ファイルの取り扱い規準</a></p></section></main><footer class=\"site-footer\"><p>青空文庫および各関係者による公式サービスではありません。</p></footer>",
        navigation(prefix),
        escape(mode),
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
        "aozoraの全配布面を権利確認済み実作品で比較する非公式の静的検証サイトです。",
        prefix,
        &body,
    )
}

fn rights(edition: &Edition) -> String {
    if edition.rights.mode == "rights-filtered" {
        format!(
            "<dl><dt>初出</dt><dd>{}</dd><dt>解析年</dt><dd>{}</dd><dt>基準日 / cutoff</dt><dd>{} / {}</dd><dt>上流commit</dt><dd><code>{}</code></dd></dl>",
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
        "<p>開発用legacy manifestです。正式公開には権利判定済みコーパスmanifestが必要です。</p>"
            .into()
    }
}

fn adjacent(edition: Option<&Edition>, rel: &str, marker: &str) -> String {
    edition.map_or_else(
        || "<span aria-hidden=\"true\"></span>".into(),
        |value| {
            format!(
                "<a rel=\"{rel}\" href=\"./{}.html\">{} {}</a>",
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
                "<p class=\"contributor\"><span>{}</span>{}</p>",
                escape(&value.role),
                escape(&value.name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!(
        "{}\n<main><article class=\"book\"><header class=\"book-header\"><p class=\"eyebrow\">byte-identical canonical output · {}</p><h1>{}</h1><p class=\"reading\">{}</p><div class=\"contributors\">{contributors}</div></header><div class=\"ornament\" aria-hidden=\"true\">＊　＊　＊</div><section class=\"reader aozora-notation\" aria-label=\"本文\">{}</section><section class=\"source-information\"><h2>Diagnostics</h2>{diagnostics}</section><section class=\"source-information\"><h2>採用根拠と出典</h2>{}<ul class=\"source-links\"><li><a href=\"{}\">青空文庫の作品カード</a></li><li><a href=\"{}\">公式テキストZIP</a></li><li><a href=\"../sources/{}.txt\" download>検証したUTF-8原文</a></li><li><a href=\"../reports/{}.json\">配布面比較report</a></li><li><a href=\"https://www.aozora.gr.jp/guide/kijyunn.html\">収録ファイルの取り扱い規準</a></li></ul></section></article><nav class=\"work-navigation\" aria-label=\"作品間の移動\">{}<a href=\"../index.html\">作品一覧</a>{}</nav></main><footer class=\"site-footer\"><p>aozora {} · schema {}</p><p>非公式の検証サイトです。</p></footer>",
        navigation("../"),
        work.canonical_engine,
        escape(&edition.title),
        escape(&edition.reading),
        work.canonical.html,
        rights(edition),
        escape(&edition.card_url),
        escape(&edition.archive_url),
        escape(&edition.edition_id),
        escape(&edition.edition_id),
        adjacent(previous, "prev", "←"),
        adjacent(next, "next", "→"),
        escape(&work.canonical.version),
        work.canonical.schema_version
    );
    shell(
        &format!("{} — aozora verification lab", edition.title),
        &format!(
            "{}をaozoraで変換し配布面を比較した非公式の検証結果です。",
            edition.title
        ),
        "../",
        &body,
    )
}

fn grouped_page(title: &str, groups: &BTreeMap<String, Vec<Edition>>) -> String {
    let sections = groups
        .iter()
        .map(|(name, editions)| {
            format!(
                "<section><div class=\"section-heading\"><h2>{}</h2><span>{}</span></div><ol class=\"work-grid\">{}</ol></section>",
                escape(name),
                editions.len(),
                work_cards(editions, "../../")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!(
        "{}<main class=\"index-main\"><section class=\"hero\"><p class=\"eyebrow\">static index</p><h1>{}</h1></section>{sections}</main><footer class=\"site-footer\"><p>非公式の検証サイトです。</p></footer>",
        navigation("../../"),
        escape(title)
    );
    shell(
        &format!("{title} — aozora verification lab"),
        title,
        "../../",
        &body,
    )
}

fn gojuon(reading: &str) -> &'static str {
    let first = reading.chars().next().unwrap_or('他');
    for (characters, row) in [
        ("あいうえおぁぃぅぇぉ", "あ行"),
        ("かきくけこがぎぐげご", "か行"),
        ("さしすせそざじずぜぞ", "さ行"),
        ("たちつてとだぢづでどっ", "た行"),
        ("なにぬねの", "な行"),
        ("はひふへほばびぶべぼぱぴぷぺぽ", "は行"),
        ("まみむめも", "ま行"),
        ("やゆよゃゅょ", "や行"),
        ("らりるれろ", "ら行"),
        ("わをん", "わ行"),
    ] {
        if characters.contains(first) {
            return row;
        }
    }
    "その他"
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
        grouped_page("著者索引", &authors),
    )?;
    fs::write(
        staging.join("indexes/gojuon/index.html"),
        grouped_page("五十音索引", &readings),
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

    use super::{BuildOptions, DEFAULT_SIZE_LIMIT, build};
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
        assert!(
            fs::read_to_string(first.join("index.html"))?
                .find("<script")
                .is_none()
        );

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
