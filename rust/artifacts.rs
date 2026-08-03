use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::model::Engine;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct EngineArtifact {
    pub command: Vec<String>,
    pub artifact_path: PathBuf,
    pub sha256: String,
    #[serde(default)]
    pub support_paths: BTreeMap<PathBuf, String>,
    pub expected_version: String,
    pub expected_schema_version: u32,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

const fn default_timeout_ms() -> u64 {
    30_000
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct ArtifactFile {
    schema_version: u32,
    aozora_commit: String,
    engines: BTreeMap<Engine, EngineArtifact>,
}

#[derive(Clone, Debug)]
pub struct ArtifactManifest {
    pub path: PathBuf,
    pub sha256: String,
    pub aozora_commit: String,
    pub engines: BTreeMap<Engine, EngineArtifact>,
}

#[must_use]
pub fn sha256(bytes: impl AsRef<[u8]>) -> String {
    lowercase_hex(Sha256::digest(bytes.as_ref()))
}

#[must_use]
pub fn lowercase_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

pub fn verify_digest(bytes: &[u8], expected: &str, label: &str) -> Result<()> {
    validate_digest(expected, label)?;
    let received = sha256(bytes);
    if received != expected {
        bail!("{label} SHA-256 mismatch: expected {expected}, received {received}");
    }
    Ok(())
}

pub fn validate_digest(value: &str, label: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("{label} must be a lowercase SHA-256 digest");
    }
    Ok(())
}

pub fn load_manifest(root: &Path, option: Option<&Path>) -> Result<Option<ArtifactManifest>> {
    let path = option.map_or_else(|| root.join("lab/artifacts.json"), |value| root.join(value));
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && option.is_none() => {
            return Ok(None);
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let mut manifest: ArtifactFile =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    if manifest.schema_version != 1 {
        bail!("{} schemaVersion must be 1", path.display());
    }
    if manifest.aozora_commit.len() != 40
        || !manifest
            .aozora_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!(
            "{} aozoraCommit must be a lowercase 40-character SHA",
            path.display()
        );
    }
    let parent = path.parent().unwrap_or(root);
    for artifact in manifest.engines.values_mut() {
        if artifact.command.is_empty() || artifact.command.iter().any(String::is_empty) {
            bail!("{} contains an empty worker command", path.display());
        }
        if artifact.expected_version.is_empty() || artifact.expected_schema_version == 0 {
            bail!(
                "{} contains an invalid expected version/schema",
                path.display()
            );
        }
        if !(100..=300_000).contains(&artifact.timeout_ms) {
            bail!(
                "{} timeoutMs must be between 100 and 300000",
                path.display()
            );
        }
        validate_digest(&artifact.sha256, "artifact sha256")?;
        artifact.artifact_path = parent.join(&artifact.artifact_path);
        let mut support_paths = BTreeMap::new();
        for (support_path, digest) in &artifact.support_paths {
            validate_digest(digest, "support artifact sha256")?;
            support_paths.insert(parent.join(support_path), digest.clone());
        }
        artifact.support_paths = support_paths;
    }
    Ok(Some(ArtifactManifest {
        path,
        sha256: sha256(bytes),
        aozora_commit: manifest.aozora_commit,
        engines: manifest.engines,
    }))
}

pub fn verify_artifact(engine: Engine, artifact: &EngineArtifact) -> Result<()> {
    let bytes = fs::read(&artifact.artifact_path).with_context(|| {
        format!(
            "{engine} artifact unavailable: {}",
            artifact.artifact_path.display()
        )
    })?;
    verify_digest(
        &bytes,
        &artifact.sha256,
        &format!("{engine} artifact {}", artifact.artifact_path.display()),
    )?;
    for (path, expected) in &artifact.support_paths {
        let bytes = fs::read(path).with_context(|| {
            format!("{engine} support artifact unavailable: {}", path.display())
        })?;
        verify_digest(
            &bytes,
            expected,
            &format!("{engine} support artifact {}", path.display()),
        )?;
    }
    Ok(())
}

pub fn environment_command(engine: Engine) -> Result<Option<Vec<String>>> {
    let name = format!("AOZORA_LAB_{}_WORKER", engine.as_str().to_ascii_uppercase());
    let Ok(value) = env::var(&name) else {
        return Ok(None);
    };
    let command: Vec<String> = serde_json::from_str(&value)
        .with_context(|| format!("{name} must be a JSON command array"))?;
    if command.is_empty() || command.iter().any(String::is_empty) {
        bail!("{name} must contain at least one non-empty command component");
    }
    Ok(Some(command))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use anyhow::{Result, bail};
    use tempfile::tempdir;

    use super::{load_manifest, sha256, verify_artifact, verify_digest};
    use crate::model::Engine;

    #[test]
    fn digest_is_stable_and_mismatch_is_explicit() -> Result<()> {
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256("abc"), expected);
        assert_eq!(sha256("abc").len(), 64);
        let Err(error) = verify_digest(b"different", expected, "fixture") else {
            bail!("digest should differ");
        };
        let message = error.to_string();
        assert!(message.contains("expected"));
        assert!(message.contains("received"));
        Ok(())
    }

    #[test]
    fn support_artifact_tampering_fails_closed() -> Result<()> {
        let directory = tempdir()?;
        fs::write(directory.path().join("distribution"), b"distribution")?;
        fs::write(directory.path().join("adapter"), b"adapter")?;
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "aozoraCommit": "a".repeat(40),
            "engines": {
                "rust": {
                    "command": ["adapter"],
                    "artifactPath": "distribution",
                    "sha256": sha256("distribution"),
                    "supportPaths": { "adapter": sha256("adapter") },
                    "expectedVersion": "1.0.0",
                    "expectedSchemaVersion": 3
                }
            }
        });
        fs::write(
            directory.path().join("artifacts.json"),
            serde_json::to_vec(&manifest)?,
        )?;
        let loaded = load_manifest(directory.path(), Some(Path::new("artifacts.json")))?
            .ok_or_else(|| anyhow::anyhow!("manifest missing"))?;
        let artifact = loaded
            .engines
            .get(&Engine::Rust)
            .ok_or_else(|| anyhow::anyhow!("Rust artifact missing"))?;
        verify_artifact(Engine::Rust, artifact)?;
        fs::write(directory.path().join("adapter"), b"tampered")?;
        let Err(error) = verify_artifact(Engine::Rust, artifact) else {
            bail!("tampered support artifact must fail");
        };
        assert!(error.to_string().contains("SHA-256 mismatch"));
        Ok(())
    }
}
