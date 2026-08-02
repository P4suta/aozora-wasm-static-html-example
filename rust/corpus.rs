use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use url::Url;

use crate::artifacts::{sha256, validate_digest};
use crate::model::{Contributor, Edition, LoadedCorpus, RightsEvidence};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct LegacyMetadata {
    url: String,
    snapshot_date: String,
    retrieved_date: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyContributor {
    role: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct LegacyWork {
    id: String,
    title: String,
    reading: String,
    copyright: String,
    contributors: Vec<LegacyContributor>,
    card_url: String,
    archive_url: String,
    archive_filename: String,
    archive_sha256: String,
    source_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct LegacyManifest {
    metadata_source: LegacyMetadata,
    works: Vec<LegacyWork>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct CorpusMetadata {
    repository: String,
    commit: String,
    reference_date: String,
    cutoff_year: u16,
    metadata_url: String,
    metadata_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RightsContributor {
    role: String,
    name: String,
    copyright: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstPublication {
    raw: String,
    years: Vec<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct CorpusEntry {
    edition_id: String,
    work_id: String,
    title: String,
    reading: String,
    copyright: String,
    contributors: Vec<RightsContributor>,
    first_publication: FirstPublication,
    card_url: String,
    archive_url: String,
    archive_filename: String,
    utf8_filename: String,
    upstream_commit: String,
    csv_sha256: String,
    archive_sha256: String,
    utf8_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct RightsManifest {
    schema_version: u32,
    corpus: CorpusMetadata,
    entries: Vec<CorpusEntry>,
}

fn text(value: &str, label: &str) -> Result<()> {
    if value.is_empty() || value.trim() != value {
        bail!("{label} must be non-empty without surrounding whitespace");
    }
    Ok(())
}

fn digits(value: &str, count: usize, label: &str) -> Result<()> {
    if value.len() != count || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("{label} must contain exactly {count} ASCII digits");
    }
    Ok(())
}

fn official_url(value: &str, label: &str) -> Result<()> {
    let url = Url::parse(value).with_context(|| format!("{label} must be a URL"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("www.aozora.gr.jp")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        bail!("{label} must be an official Aozora HTTPS URL");
    }
    Ok(())
}

fn archive_url(value: &str) -> Result<()> {
    official_url(value, "archiveUrl")?;
    if !Url::parse(value)?.path().ends_with(".zip") {
        bail!("archiveUrl must identify an official ZIP");
    }
    Ok(())
}

fn iso_date(value: &str, label: &str) -> Result<u16> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        bail!("{label} must be an ISO date");
    }
    let year: u16 = value[..4].parse()?;
    let month: u8 = value[5..7].parse()?;
    let day: u8 = value[8..].parse()?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0 || day == 0 || day > days {
        bail!("{label} is not a real calendar date");
    }
    Ok(year)
}

fn filename(value: &str, label: &str) -> Result<()> {
    let path = Path::new(value);
    if value.is_empty()
        || path.extension().and_then(|part| part.to_str()) != Some("txt")
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        bail!("{label} must be a safe .txt filename");
    }
    Ok(())
}

fn commit(value: &str, label: &str) -> Result<()> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("{label} must be a lowercase 40-character SHA");
    }
    Ok(())
}

#[must_use]
pub fn make_edition_id(work_id: &str, archive_url: &str) -> String {
    format!(
        "{work_id}-{}",
        &sha256(format!("{work_id}\n{archive_url}"))[..16]
    )
}

fn parse_rights(bytes: &[u8], manifest_dir: &Path) -> Result<LoadedCorpus> {
    let manifest: RightsManifest =
        serde_json::from_slice(bytes).context("parse rights corpus manifest")?;
    if manifest.schema_version != 1 {
        bail!("rights corpus schemaVersion must be 1");
    }
    if manifest.corpus.repository != "https://github.com/P4suta/aozora-rights-filtered-corpus" {
        bail!("rights corpus repository is not canonical");
    }
    commit(&manifest.corpus.commit, "corpus commit")?;
    let reference_year = iso_date(&manifest.corpus.reference_date, "referenceDate")?;
    if reference_year.checked_sub(96) != Some(manifest.corpus.cutoff_year) {
        bail!("cutoffYear does not match the annual referenceDate policy");
    }
    official_url(&manifest.corpus.metadata_url, "metadataUrl")?;
    validate_digest(&manifest.corpus.metadata_sha256, "metadataSha256")?;
    let mut ids = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut editions = Vec::with_capacity(manifest.entries.len());
    for entry in manifest.entries {
        digits(&entry.work_id, 6, "workId")?;
        text(&entry.title, "title")?;
        text(&entry.reading, "reading")?;
        if entry.copyright != "なし" {
            bail!("{} work copyright is not なし", entry.work_id);
        }
        if entry.contributors.is_empty() {
            bail!("{} has no contributors", entry.work_id);
        }
        for contributor in &entry.contributors {
            text(&contributor.role, "contributor role")?;
            text(&contributor.name, "contributor name")?;
            if contributor.copyright != "なし" {
                bail!("{} contributor copyright is not なし", entry.work_id);
            }
        }
        text(&entry.first_publication.raw, "first publication")?;
        if entry.first_publication.years.is_empty() {
            bail!("{} has no parsed publication years", entry.work_id);
        }
        if entry
            .first_publication
            .years
            .iter()
            .any(|year| *year < 1000)
            || entry
                .first_publication
                .years
                .iter()
                .any(|year| *year > manifest.corpus.cutoff_year)
        {
            bail!(
                "{} publication year exceeds cutoff {}",
                entry.work_id,
                manifest.corpus.cutoff_year
            );
        }
        official_url(&entry.card_url, "cardUrl")?;
        archive_url(&entry.archive_url)?;
        filename(&entry.archive_filename, "archiveFilename")?;
        filename(&entry.utf8_filename, "utf8Filename")?;
        commit(&entry.upstream_commit, "upstreamCommit")?;
        if entry.upstream_commit != manifest.corpus.commit {
            bail!(
                "{} upstreamCommit differs from the corpus commit",
                entry.work_id
            );
        }
        validate_digest(&entry.csv_sha256, "csvSha256")?;
        validate_digest(&entry.archive_sha256, "archiveSha256")?;
        validate_digest(&entry.utf8_sha256, "utf8Sha256")?;
        let expected_id = make_edition_id(&entry.work_id, &entry.archive_url);
        if entry.edition_id != expected_id {
            bail!("{} editionId must be {expected_id}", entry.work_id);
        }
        if !ids.insert(entry.edition_id.clone()) {
            bail!("duplicate editionId {}", entry.edition_id);
        }
        if !identities.insert((entry.work_id.clone(), entry.archive_url.clone())) {
            bail!("duplicate workId + archiveUrl edition identity");
        }
        editions.push(Edition {
            edition_id: entry.edition_id,
            work_id: entry.work_id,
            title: entry.title,
            reading: entry.reading,
            contributors: entry
                .contributors
                .into_iter()
                .map(|value| Contributor {
                    role: value.role,
                    name: value.name,
                    copyright: value.copyright,
                })
                .collect(),
            card_url: entry.card_url,
            archive_url: entry.archive_url,
            source_path: manifest_dir
                .join("sources")
                .join(entry.utf8_filename)
                .to_string_lossy()
                .into_owned(),
            source_sha256: entry.utf8_sha256,
            rights: RightsEvidence {
                mode: "rights-filtered".into(),
                first_publication_raw: Some(entry.first_publication.raw),
                publication_years: Some(entry.first_publication.years),
                reference_date: Some(manifest.corpus.reference_date.clone()),
                cutoff_year: Some(manifest.corpus.cutoff_year),
                upstream_commit: Some(entry.upstream_commit),
                csv_sha256: Some(entry.csv_sha256),
                archive_sha256: entry.archive_sha256,
            },
        });
    }
    Ok(LoadedCorpus {
        manifest_sha256: sha256(bytes),
        rights_filtered: true,
        editions,
    })
}

fn parse_legacy(bytes: &[u8], root: &Path) -> Result<LoadedCorpus> {
    let manifest: LegacyManifest =
        serde_json::from_slice(bytes).context("parse development works manifest")?;
    official_url(&manifest.metadata_source.url, "metadata source URL")?;
    validate_digest(&manifest.metadata_source.sha256, "metadata source SHA-256")?;
    iso_date(&manifest.metadata_source.snapshot_date, "snapshotDate")?;
    iso_date(&manifest.metadata_source.retrieved_date, "retrievedDate")?;
    if manifest.metadata_source.retrieved_date < manifest.metadata_source.snapshot_date {
        bail!("metadata retrievedDate precedes snapshotDate");
    }
    let mut ids = BTreeSet::new();
    let mut editions = Vec::with_capacity(manifest.works.len());
    for work in manifest.works {
        digits(&work.id, 6, "work id")?;
        if !ids.insert(work.id.clone()) {
            bail!("duplicate work id {}", work.id);
        }
        text(&work.title, "title")?;
        text(&work.reading, "reading")?;
        if work.copyright != "なし" || work.contributors.is_empty() {
            bail!("{} is not a copyright-none work with contributors", work.id);
        }
        official_url(&work.card_url, "cardUrl")?;
        archive_url(&work.archive_url)?;
        filename(&work.archive_filename, "archiveFilename")?;
        validate_digest(&work.archive_sha256, "archiveSha256")?;
        validate_digest(&work.source_sha256, "sourceSha256")?;
        editions.push(Edition {
            edition_id: make_edition_id(&work.id, &work.archive_url),
            work_id: work.id.clone(),
            title: work.title,
            reading: work.reading,
            contributors: work
                .contributors
                .into_iter()
                .map(|value| Contributor {
                    role: value.role,
                    name: value.name,
                    copyright: "なし".into(),
                })
                .collect(),
            card_url: work.card_url,
            archive_url: work.archive_url,
            source_path: root
                .join("sources")
                .join(format!("{}.txt", work.id))
                .to_string_lossy()
                .into_owned(),
            source_sha256: work.source_sha256,
            rights: RightsEvidence {
                mode: "development-legacy".into(),
                first_publication_raw: None,
                publication_years: None,
                reference_date: None,
                cutoff_year: None,
                upstream_commit: None,
                csv_sha256: None,
                archive_sha256: work.archive_sha256,
            },
        });
    }
    Ok(LoadedCorpus {
        manifest_sha256: sha256(bytes),
        rights_filtered: false,
        editions,
    })
}

pub fn load(root: &Path, option: Option<&Path>) -> Result<LoadedCorpus> {
    let path: PathBuf = option.map_or_else(|| root.join("works.json"), |value| root.join(value));
    let bytes =
        fs::read(&path).with_context(|| format!("read corpus manifest {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse corpus manifest {}", path.display()))?;
    if value.get("corpus").is_some() && value.get("entries").is_some() {
        parse_rights(&bytes, path.parent().unwrap_or(root))
    } else {
        parse_legacy(&bytes, root)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail};
    use serde_json::json;

    use super::{make_edition_id, parse_rights};

    fn manifest(years: &[u16], copyright: &str) -> Vec<u8> {
        let work_id = "000001";
        let archive_url = "https://www.aozora.gr.jp/cards/000001/files/1.zip";
        let commit = "a".repeat(40);
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "corpus": {
                "repository": "https://github.com/P4suta/aozora-rights-filtered-corpus",
                "commit": commit,
                "referenceDate": "2026-08-02",
                "cutoffYear": 1930,
                "metadataUrl": "https://www.aozora.gr.jp/index_pages/list_person_all_extended_utf8.zip",
                "metadataSha256": "b".repeat(64)
            },
            "entries": [{
                "editionId": make_edition_id(work_id, archive_url),
                "workId": work_id,
                "title": "作品",
                "reading": "さくひん",
                "copyright": copyright,
                "contributors": [{"role": "著者", "name": "著者", "copyright": "なし"}],
                "firstPublication": {"raw": "1929年、1930年", "years": years},
                "cardUrl": "https://www.aozora.gr.jp/cards/000001/card1.html",
                "archiveUrl": archive_url,
                "archiveFilename": "source.txt",
                "utf8Filename": "edition.txt",
                "upstreamCommit": "a".repeat(40),
                "csvSha256": "c".repeat(64),
                "archiveSha256": "d".repeat(64),
                "utf8Sha256": "e".repeat(64)
            }]
        }))
        .unwrap_or_default()
    }

    #[test]
    fn accepts_the_cutoff_boundary_and_rejects_the_next_year() -> Result<()> {
        assert!(parse_rights(&manifest(&[1929, 1930], "なし"), std::path::Path::new(".")).is_ok());
        let Err(error) = parse_rights(&manifest(&[1931], "なし"), std::path::Path::new("."))
        else {
            bail!("1931 must exceed cutoff");
        };
        assert!(error.to_string().contains("exceeds cutoff"));
        Ok(())
    }

    #[test]
    fn rejects_non_free_work_metadata() -> Result<()> {
        let Err(error) = parse_rights(&manifest(&[1930], "あり"), std::path::Path::new("."))
        else {
            bail!("copyright flag must be none");
        };
        assert!(error.to_string().contains("copyright"));
        Ok(())
    }
}
