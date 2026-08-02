use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::artifacts::{
    ArtifactManifest, environment_command, load_manifest, sha256, verify_artifact, verify_digest,
};
use crate::corpus;
use crate::model::{
    Diagnostic, Edition, Engine, EngineResult, EngineSelector, Scope, VerificationReport,
    VerifiedWork,
};
use crate::worker::Worker;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsBaseline {
    schema_version: u32,
    corpus_manifest_sha256: String,
    entries: BTreeMap<String, Vec<Diagnostic>>,
}

pub struct VerifyOptions<'a> {
    pub root: &'a Path,
    pub engine: EngineSelector,
    pub scope: Scope,
    pub shard: Option<&'a str>,
    pub corpus: Option<&'a Path>,
    pub artifacts: Option<&'a Path>,
    pub diagnostics_baseline: Option<&'a Path>,
    pub require_rights_filtered: bool,
}

fn split_body(source: &str) -> Result<String> {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    let separators: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            (line.len() >= 20 && line.bytes().all(|byte| byte == b'-')).then_some(index)
        })
        .collect();
    let bibliography = lines
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, line)| line.starts_with("底本：").then_some(index))
        .context("missing terminal bibliography beginning with 底本：")?;
    let body_start = match separators.as_slice() {
        [] => {
            let header_start = lines
                .iter()
                .position(|line| !line.trim().is_empty())
                .context("source has no title header")?;
            let header_end = lines[header_start..bibliography]
                .iter()
                .position(|line| line.trim().is_empty())
                .map(|offset| header_start + offset)
                .context("legacy source has no blank line after its title header")?;
            lines[header_end..bibliography]
                .iter()
                .position(|line| !line.trim().is_empty())
                .map(|offset| header_end + offset)
                .context("legacy source has no body after its title header")?
        }
        [_] => bail!("source has an unmatched legend separator"),
        [_, second, ..] => second + 1,
    };
    if body_start >= bibliography {
        bail!("source body starts after its terminal bibliography");
    }
    let mut body = &lines[body_start..bibliography];
    while body.first().is_some_and(|line| line.trim().is_empty()) {
        body = &body[1..];
    }
    while body.last().is_some_and(|line| line.trim().is_empty()) {
        body = &body[..body.len() - 1];
    }
    if body.is_empty() {
        bail!("empty source body");
    }
    Ok(body.join("\n"))
}

fn source_body(edition: &Edition) -> Result<String> {
    let bytes = fs::read(&edition.source_path)
        .with_context(|| format!("read {} source", edition.edition_id))?;
    verify_digest(
        &bytes,
        &edition.source_sha256,
        &format!("{} source", edition.edition_id),
    )?;
    let source = std::str::from_utf8(&bytes)
        .with_context(|| format!("{} source is not valid UTF-8", edition.edition_id))?;
    if source.contains('\u{fffd}') {
        bail!(
            "{} source contains a Unicode replacement character",
            edition.edition_id
        );
    }
    split_body(source)
}

fn features(source: &str) -> BTreeSet<&'static str> {
    let mut result = BTreeSet::from(["plain"]);
    if source.contains('《') {
        result.insert("ruby");
    }
    if source.contains("※［＃") {
        result.insert("gaiji");
    }
    if source.contains("［＃") {
        result.insert("annotation");
    }
    if source.contains("ここから") || source.contains("ここで") {
        result.insert("container");
    }
    if ["傍点", "太字", "字下げ", "字上げ", "見出し"]
        .iter()
        .any(|needle| source.contains(needle))
    {
        result.insert("styled-range");
    }
    result
}

fn quick(editions: &[Edition]) -> Result<Vec<Edition>> {
    let mut candidates: Vec<(Edition, BTreeSet<&'static str>)> = editions
        .iter()
        .map(|edition| {
            let source = source_body(edition)
                .with_context(|| format!("prepare {} source body", edition.edition_id))?;
            Ok((edition.clone(), features(&source)))
        })
        .collect::<Result<_>>()?;
    candidates.sort_by(|left, right| left.0.edition_id.cmp(&right.0.edition_id));
    let mut uncovered: BTreeSet<&str> = candidates
        .iter()
        .flat_map(|(_, values)| values.iter().copied())
        .collect();
    let mut selected = Vec::new();
    while !uncovered.is_empty() && !candidates.is_empty() {
        candidates.sort_by(|left, right| {
            let left_score = left.1.intersection(&uncovered).count();
            let right_score = right.1.intersection(&uncovered).count();
            right_score
                .cmp(&left_score)
                .then_with(|| left.0.edition_id.cmp(&right.0.edition_id))
        });
        let (edition, covered) = candidates.remove(0);
        uncovered.retain(|feature| !covered.contains(feature));
        selected.push(edition);
    }
    Ok(selected)
}

fn shard(value: Option<&str>) -> Result<(usize, usize, String)> {
    let Some(value) = value else {
        return Ok((0, 1, "0/1".into()));
    };
    let Some((index, count)) = value.split_once('/') else {
        bail!("invalid shard {value}; expected i/n");
    };
    let index: usize = index
        .parse()
        .with_context(|| format!("invalid shard {value}"))?;
    let count: usize = count
        .parse()
        .with_context(|| format!("invalid shard {value}"))?;
    if count == 0 || index >= count {
        bail!("invalid shard {value}; require 0 <= i < n");
    }
    Ok((index, count, format!("{index}/{count}")))
}

fn baseline(root: &Path, option: Option<&Path>) -> Result<DiagnosticsBaseline> {
    let path = option.map_or_else(
        || root.join("lab/diagnostics-baseline.json"),
        |value| root.join(value),
    );
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let result: DiagnosticsBaseline =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    if result.schema_version != 1 {
        bail!("{} schemaVersion must be 1", path.display());
    }
    Ok(result)
}

fn commands(
    root: &Path,
    selector: EngineSelector,
    manifest: Option<&ArtifactManifest>,
    require_pinned: bool,
) -> Result<Vec<Worker>> {
    let mut workers = Vec::new();
    for engine in selector.engines() {
        let artifact = manifest.and_then(|value| value.engines.get(&engine));
        if let Some(value) = artifact {
            verify_artifact(engine, value)?;
        }
        let command = match artifact {
            Some(value) => value.command.clone(),
            None if require_pinned => {
                bail!("{engine} is missing from the pinned artifact manifest")
            }
            None => environment_command(engine)?.with_context(|| {
                format!(
                    "{engine} worker is not pinned; add it to lab/artifacts.json or set AOZORA_LAB_{}_WORKER",
                    engine.as_str().to_ascii_uppercase()
                )
            })?,
        };
        workers.push(Worker::start(engine, &command, root, artifact)?);
    }
    Ok(workers)
}

fn bytes(value: &impl serde::Serialize) -> Result<Vec<u8>> {
    serde_json::to_vec(value).context("serialize comparison projection")
}

fn mismatch(
    edition: &str,
    candidate: Engine,
    field: &str,
    expected: &[u8],
    received: &[u8],
) -> anyhow::Error {
    anyhow::anyhow!(
        "{edition} {candidate}.{field} differs from canonical (expected sha256={}, received sha256={})",
        sha256(expected),
        sha256(received)
    )
}

pub fn assert_parity(
    edition: &str,
    candidate_engine: Engine,
    expected: &EngineResult,
    received: &EngineResult,
) -> Result<()> {
    for (field, expected, received) in [
        (
            "version",
            expected.version.as_bytes(),
            received.version.as_bytes(),
        ),
        ("html", expected.html.as_bytes(), received.html.as_bytes()),
        (
            "source",
            expected.source.as_bytes(),
            received.source.as_bytes(),
        ),
    ] {
        if expected != received {
            return Err(mismatch(
                edition,
                candidate_engine,
                field,
                expected,
                received,
            ));
        }
    }
    if expected.schema_version != received.schema_version {
        return Err(mismatch(
            edition,
            candidate_engine,
            "schemaVersion",
            &expected.schema_version.to_le_bytes(),
            &received.schema_version.to_le_bytes(),
        ));
    }
    for (field, expected, received) in [
        (
            "diagnostics",
            bytes(&expected.diagnostics)?,
            bytes(&received.diagnostics)?,
        ),
        ("gaiji", bytes(&expected.gaiji)?, bytes(&received.gaiji)?),
        ("nodes", bytes(&expected.nodes)?, bytes(&received.nodes)?),
        ("pairs", bytes(&expected.pairs)?, bytes(&received.pairs)?),
        (
            "containerPairs",
            bytes(&expected.container_pairs)?,
            bytes(&received.container_pairs)?,
        ),
    ] {
        if expected != received {
            return Err(mismatch(
                edition,
                candidate_engine,
                field,
                &expected,
                &received,
            ));
        }
    }
    Ok(())
}

fn diagnostics(
    edition_id: &str,
    actual: &[Diagnostic],
    baseline: &DiagnosticsBaseline,
) -> Result<Vec<String>> {
    let expected = baseline
        .entries
        .get(edition_id)
        .map_or(&[][..], Vec::as_slice);
    let expected_set: BTreeSet<Vec<u8>> = expected.iter().map(bytes).collect::<Result<_>>()?;
    let actual_set: BTreeSet<Vec<u8>> = actual.iter().map(bytes).collect::<Result<_>>()?;
    let additions: Vec<&Vec<u8>> = actual_set.difference(&expected_set).collect();
    if !additions.is_empty() {
        bail!(
            "{edition_id} produced new diagnostics: {}",
            String::from_utf8_lossy(additions[0])
        );
    }
    Ok(expected_set
        .difference(&actual_set)
        .map(|value| {
            format!(
                "{edition_id} resolved diagnostic {}",
                String::from_utf8_lossy(value)
            )
        })
        .collect())
}

pub fn run(
    options: &VerifyOptions<'_>,
    mut progress: impl FnMut(&str),
) -> Result<VerificationReport> {
    let loaded = corpus::load(options.root, options.corpus)?;
    if options.require_rights_filtered && !loaded.rights_filtered {
        bail!("release verification requires a rights-filtered corpus manifest");
    }
    let scoped = match options.scope {
        Scope::Quick => quick(&loaded.editions)?,
        Scope::Full => loaded.editions,
    };
    let (shard_index, shard_count, shard_text) = shard(options.shard)?;
    let editions: Vec<Edition> = scoped
        .into_iter()
        .enumerate()
        .filter_map(|(index, edition)| (index % shard_count == shard_index).then_some(edition))
        .collect();
    if editions.is_empty() {
        bail!("shard {shard_text} selected no editions");
    }
    let baseline = baseline(options.root, options.diagnostics_baseline)?;
    if baseline.corpus_manifest_sha256 != loaded.manifest_sha256 {
        bail!("diagnostics baseline was generated for a different corpus manifest");
    }
    let manifest = load_manifest(options.root, options.artifacts)?;
    if options.require_rights_filtered && manifest.is_none() {
        bail!("release verification requires a pinned artifact manifest");
    }
    let mut workers = commands(
        options.root,
        options.engine,
        manifest.as_ref(),
        options.require_rights_filtered,
    )?;
    let engines: Vec<Engine> = workers.iter().map(|worker| worker.engine()).collect();
    let canonical_engine = if options.engine == EngineSelector::All {
        Engine::Wasm
    } else {
        *engines.first().context("no engines selected")?
    };
    let mut works = Vec::with_capacity(editions.len());
    for edition in editions {
        progress(&format!(
            "{} ({})",
            edition.edition_id,
            engines
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
        let source = source_body(&edition)?;
        let mut outputs = BTreeMap::new();
        for worker in &mut workers {
            let engine = worker.engine();
            let output = worker.render(&source)?;
            outputs.insert(engine, output);
        }
        let canonical = outputs
            .get(&canonical_engine)
            .cloned()
            .with_context(|| format!("{} canonical output is missing", edition.edition_id))?;
        for (engine, output) in &outputs {
            if *engine != canonical_engine {
                assert_parity(&edition.edition_id, *engine, &canonical, output)?;
            }
        }
        let improvements = diagnostics(&edition.edition_id, &canonical.diagnostics, &baseline)?;
        works.push(VerifiedWork {
            edition,
            canonical_engine,
            canonical,
            engines: outputs,
            improvements,
        });
    }
    Ok(VerificationReport {
        schema_version: 1,
        scope: options.scope,
        shard: shard_text,
        engines,
        corpus_manifest_sha256: loaded.manifest_sha256,
        artifacts_manifest_sha256: manifest.as_ref().map(|value| value.sha256.clone()),
        aozora_commit: manifest.map(|value| value.aozora_commit),
        works,
    })
}

impl Worker {
    #[must_use]
    pub const fn engine(&self) -> Engine {
        self.engine_value()
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail};

    use super::{assert_parity, shard, split_body};
    use crate::model::{Engine, EngineResult};

    fn result() -> EngineResult {
        EngineResult {
            version: "1.0.0".into(),
            schema_version: 3,
            html: "<p>本文</p>".into(),
            diagnostics: Vec::new(),
            gaiji: Vec::new(),
            nodes: Vec::new(),
            pairs: Vec::new(),
            container_pairs: Vec::new(),
            source: "本文".into(),
        }
    }

    #[test]
    fn parity_is_field_exact() -> Result<()> {
        let expected = result();
        let mut candidate = result();
        assert!(assert_parity("edition", Engine::Go, &expected, &candidate).is_ok());
        candidate.html.push('\n');
        let Err(error) = assert_parity("edition", Engine::Go, &expected, &candidate) else {
            bail!("transport must not trim output");
        };
        let message = error.to_string();
        assert!(message.contains("Go.html") || message.contains("go.html"));
        Ok(())
    }

    #[test]
    fn validates_shards_and_source_envelopes() {
        assert_eq!(shard(Some("2/3")).ok(), Some((2, 3, "2/3".into())));
        assert!(shard(Some("3/3")).is_err());
        let source = "題\n--------------------\n凡例\n--------------------\n\n本文\n\n底本：本\n";
        assert_eq!(split_body(source).ok().as_deref(), Some("本文"));
        let horizontal_rule = "題\n--------------------\n凡例\n--------------------\n本文\n--------------------\n備考\n底本：本\n";
        assert_eq!(
            split_body(horizontal_rule).ok().as_deref(),
            Some("本文\n--------------------\n備考")
        );
        let legacy = "題\n著者\n\n本文\n\n底本：本\n";
        assert_eq!(split_body(legacy).ok().as_deref(), Some("本文"));
        assert!(split_body("題\n--------------------\n本文\n底本：本\n").is_err());
    }
}
