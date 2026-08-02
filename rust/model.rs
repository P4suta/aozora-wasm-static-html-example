use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

pub const ENGINES: [Engine; 7] = [
    Engine::Wasm,
    Engine::Rust,
    Engine::Cli,
    Engine::Ffi,
    Engine::Extism,
    Engine::Python,
    Engine::Go,
];

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Wasm,
    Rust,
    Cli,
    Ffi,
    Extism,
    Python,
    Go,
}

impl Engine {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::Rust => "rust",
            Self::Cli => "cli",
            Self::Ffi => "ffi",
            Self::Extism => "extism",
            Self::Python => "python",
            Self::Go => "go",
        }
    }
}

impl Display for Engine {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Engine {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        ENGINES
            .into_iter()
            .find(|engine| engine.as_str() == value)
            .ok_or_else(|| anyhow::anyhow!("unknown engine {value}"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum EngineSelector {
    Wasm,
    Rust,
    Cli,
    Ffi,
    Extism,
    Python,
    Go,
    All,
}

impl EngineSelector {
    #[must_use]
    pub fn engines(self) -> Vec<Engine> {
        match self {
            Self::All => ENGINES.to_vec(),
            Self::Wasm => vec![Engine::Wasm],
            Self::Rust => vec![Engine::Rust],
            Self::Cli => vec![Engine::Cli],
            Self::Ffi => vec![Engine::Ffi],
            Self::Extism => vec![Engine::Extism],
            Self::Python => vec![Engine::Python],
            Self::Go => vec![Engine::Go],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Quick,
    Full,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub kind: String,
    pub severity: String,
    pub source: String,
    pub span: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codepoint: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Gaiji {
    pub span: Span,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mencode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codepoint: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub kind: String,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Pair {
    pub kind: String,
    pub open: Span,
    pub close: Span,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct EngineResult {
    pub version: String,
    pub schema_version: u32,
    pub html: String,
    pub diagnostics: Vec<Diagnostic>,
    pub gaiji: Vec<Gaiji>,
    pub nodes: Vec<Node>,
    pub pairs: Vec<Pair>,
    pub container_pairs: Vec<Pair>,
    pub source: String,
}

impl EngineResult {
    pub fn validate(&self) -> Result<()> {
        if self.version.is_empty() {
            bail!("engine result version is empty");
        }
        if self.schema_version == 0 {
            bail!("engine result schemaVersion must be positive");
        }
        for span in self
            .diagnostics
            .iter()
            .map(|entry| &entry.span)
            .chain(self.gaiji.iter().map(|entry| &entry.span))
            .chain(self.nodes.iter().map(|entry| &entry.span))
        {
            if span.end < span.start {
                bail!("engine result contains a reversed span");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contributor {
    pub role: String,
    pub name: String,
    pub copyright: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RightsEvidence {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_publication_raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication_years: Option<Vec<u16>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cutoff_year: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub csv_sha256: Option<String>,
    pub archive_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edition {
    pub edition_id: String,
    pub work_id: String,
    pub title: String,
    pub reading: String,
    pub contributors: Vec<Contributor>,
    pub card_url: String,
    pub archive_url: String,
    #[serde(skip)]
    pub source_path: String,
    pub source_sha256: String,
    pub rights: RightsEvidence,
}

#[derive(Clone, Debug)]
pub struct LoadedCorpus {
    pub manifest_sha256: String,
    pub rights_filtered: bool,
    pub editions: Vec<Edition>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedWork {
    pub edition: Edition,
    pub canonical_engine: Engine,
    pub canonical: EngineResult,
    pub engines: BTreeMap<Engine, EngineResult>,
    pub improvements: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReport {
    pub schema_version: u32,
    pub scope: Scope,
    pub shard: String,
    pub engines: Vec<Engine>,
    pub corpus_manifest_sha256: String,
    pub artifacts_manifest_sha256: Option<String>,
    pub aozora_commit: Option<String>,
    pub works: Vec<VerifiedWork>,
}
