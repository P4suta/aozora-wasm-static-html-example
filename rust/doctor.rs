use std::env;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::artifacts::{environment_command, load_manifest, verify_artifact};
use crate::model::{Engine, EngineSelector};

#[derive(Debug)]
pub struct Check {
    pub engine: Engine,
    pub ok: bool,
    pub detail: String,
}

fn executable(command: &str, root: &Path) -> bool {
    let path = Path::new(command);
    if path.is_absolute() {
        return path.is_file();
    }
    if command.contains('/') || command.contains('\\') {
        return root.join(path).is_file();
    }
    env::var_os("PATH").is_some_and(|value| {
        env::split_paths(&value).any(|directory| {
            let candidate: PathBuf = directory.join(command);
            candidate.is_file()
        })
    })
}

pub fn run(root: &Path, selector: EngineSelector, artifacts: Option<&Path>) -> Result<Vec<Check>> {
    let manifest = load_manifest(root, artifacts)?;
    let mut checks = Vec::new();
    for engine in selector.engines() {
        let result = (|| -> Result<String> {
            let artifact = manifest
                .as_ref()
                .and_then(|value| value.engines.get(&engine));
            if let Some(value) = artifact {
                verify_artifact(engine, value)?;
            }
            let command = match artifact {
                Some(value) => value.command.clone(),
                None => environment_command(engine)?
                    .ok_or_else(|| anyhow::anyhow!("no pinned JSONL worker command"))?,
            };
            let program = command
                .first()
                .ok_or_else(|| anyhow::anyhow!("empty worker command"))?;
            if !executable(program, root) {
                anyhow::bail!("worker executable is unavailable: {program}");
            }
            Ok(format!(
                "{}: {}",
                if artifact.is_some() {
                    "pinned artifact"
                } else {
                    "local override"
                },
                command.join(" ")
            ))
        })();
        match result {
            Ok(detail) => checks.push(Check {
                engine,
                ok: true,
                detail,
            }),
            Err(error) => checks.push(Check {
                engine,
                ok: false,
                detail: error.to_string(),
            }),
        }
    }
    Ok(checks)
}
