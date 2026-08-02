use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::artifacts::EngineArtifact;
use crate::model::{Engine, EngineResult};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Request<'a> {
    protocol_version: u32,
    request_id: &'a str,
    operation: &'static str,
    source: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct Response {
    protocol_version: u32,
    request_id: String,
    ok: bool,
    result: Option<EngineResult>,
    error: Option<String>,
}

pub struct Worker {
    engine: Engine,
    child: Child,
    input: BufWriter<ChildStdin>,
    output: Receiver<Result<String, String>>,
    timeout: Duration,
    expected: Option<(String, u32)>,
    sequence: u64,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Worker")
            .field("engine", &self.engine)
            .field("timeout", &self.timeout)
            .field("sequence", &self.sequence)
            .finish_non_exhaustive()
    }
}

impl Worker {
    #[must_use]
    pub const fn engine_value(&self) -> Engine {
        self.engine
    }

    pub fn start(
        engine: Engine,
        command: &[String],
        root: &Path,
        artifact: Option<&EngineArtifact>,
    ) -> Result<Self> {
        let Some((program, arguments)) = command.split_first() else {
            bail!("{engine} worker command is empty");
        };
        let mut child = Command::new(program)
            .args(arguments)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("start {engine} worker: {program}"))?;
        let input = child.stdin.take().context("worker stdin was not piped")?;
        let output = child.stdout.take().context("worker stdout was not piped")?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let value = line.map_err(|error| error.to_string());
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            engine,
            child,
            input: BufWriter::new(input),
            output: receiver,
            timeout: Duration::from_millis(artifact.map_or(30_000, |value| value.timeout_ms)),
            expected: artifact.map(|value| {
                (
                    value.expected_version.clone(),
                    value.expected_schema_version,
                )
            }),
            sequence: 0,
        })
    }

    pub fn render(&mut self, source: &str) -> Result<EngineResult> {
        let request_id = format!("{}-{}", self.engine, self.sequence);
        self.sequence += 1;
        serde_json::to_writer(
            &mut self.input,
            &Request {
                protocol_version: 1,
                request_id: &request_id,
                operation: "render",
                source,
            },
        )
        .with_context(|| format!("encode {} request", self.engine))?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;
        let line = match self.output.recv_timeout(self.timeout) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => bail!("{} worker output failed: {error}", self.engine),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                bail!(
                    "{} worker timed out after {}ms",
                    self.engine,
                    self.timeout.as_millis()
                );
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let status = self.child.try_wait().ok().flatten();
                bail!(
                    "{} worker stopped before responding: {status:?}",
                    self.engine
                );
            }
        };
        let response: Response = serde_json::from_str(&line)
            .with_context(|| format!("{} worker emitted invalid JSONL", self.engine))?;
        if response.protocol_version != 1 || response.request_id != request_id {
            bail!(
                "{} worker returned a mismatched protocol/request id",
                self.engine
            );
        }
        let result = match (response.ok, response.result, response.error) {
            (true, Some(result), None) => result,
            (false, None, Some(error)) => {
                bail!("{} worker rejected {request_id}: {error}", self.engine)
            }
            _ => bail!(
                "{} worker returned an inconsistent response envelope",
                self.engine
            ),
        };
        result.validate()?;
        if let Some((version, schema)) = &self.expected {
            if result.version != *version {
                bail!(
                    "{} version mismatch: expected {version}, received {}",
                    self.engine,
                    result.version
                );
            }
            if result.schema_version != *schema {
                bail!(
                    "{} schema mismatch: expected {schema}, received {}",
                    self.engine,
                    result.schema_version
                );
            }
        }
        Ok(result)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use anyhow::{Result, bail};

    use super::Worker;
    use crate::artifacts::EngineArtifact;
    use crate::model::Engine;

    fn artifact(version: &str, timeout_ms: u64) -> EngineArtifact {
        EngineArtifact {
            command: Vec::new(),
            artifact_path: PathBuf::new(),
            sha256: "0".repeat(64),
            support_paths: BTreeMap::new(),
            expected_version: version.into(),
            expected_schema_version: 3,
            timeout_ms,
        }
    }

    #[test]
    fn rejects_non_json_worker_output() -> Result<()> {
        let command = vec!["sh".into(), "-c".into(), "read line; echo noise".into()];
        let mut worker = Worker::start(Engine::Cli, &command, Path::new("."), None)?;
        let Err(error) = worker.render("本文") else {
            bail!("invalid JSON must fail");
        };
        assert!(error.to_string().contains("invalid JSONL"));
        Ok(())
    }

    #[test]
    fn rejects_stopped_worker() -> Result<()> {
        let command = vec!["sh".into(), "-c".into(), "read line; exit 9".into()];
        let mut worker = Worker::start(Engine::Cli, &command, Path::new("."), None)?;
        let Err(error) = worker.render("本文") else {
            bail!("stopped worker must fail");
        };
        assert!(error.to_string().contains("stopped before responding"));
        Ok(())
    }

    #[test]
    fn rejects_timed_out_worker() -> Result<()> {
        let command = vec!["sh".into(), "-c".into(), "read line; exec sleep 5".into()];
        let metadata = artifact("0.5.0", 100);
        let mut worker = Worker::start(Engine::Cli, &command, Path::new("."), Some(&metadata))?;
        let Err(error) = worker.render("本文") else {
            bail!("timed out worker must fail");
        };
        assert!(error.to_string().contains("timed out after 100ms"));
        Ok(())
    }

    #[test]
    fn rejects_mixed_distribution_versions() -> Result<()> {
        let response = r#"{"protocolVersion":1,"requestId":"cli-0","ok":true,"result":{"version":"0.4.0","schemaVersion":3,"html":"<p>本文</p>","diagnostics":[],"gaiji":[],"nodes":[],"pairs":[],"containerPairs":[],"source":"本文"},"error":null}"#;
        let command = vec![
            "sh".into(),
            "-c".into(),
            format!("read line; echo '{response}'"),
        ];
        let metadata = artifact("0.5.0", 1_000);
        let mut worker = Worker::start(Engine::Cli, &command, Path::new("."), Some(&metadata))?;
        let Err(error) = worker.render("本文") else {
            bail!("mixed versions must fail");
        };
        assert!(
            error
                .to_string()
                .contains("version mismatch: expected 0.5.0, received 0.4.0")
        );
        Ok(())
    }
}
