use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitCode};

use anyhow::{Context, Result, bail};
use aozora_lab::doctor;
use aozora_lab::model::{EngineSelector, Scope, VerificationReport};
use aozora_lab::site::{self, BuildOptions};
use aozora_lab::verify::{self, VerifyOptions};
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "lab",
    version,
    about = "aozora real-work distribution verification lab"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<LabCommand>,
}

#[derive(Debug, Subcommand)]
enum LabCommand {
    Doctor(DoctorArgs),
    Verify(VerifyArgs),
    Build(BuildArgs),
    Visual(VisualArgs),
    Full(FullArgs),
}

#[derive(Debug, Args)]
struct InputArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    corpus: Option<PathBuf>,
    #[arg(long)]
    artifacts: Option<PathBuf>,
    #[arg(long)]
    diagnostics_baseline: Option<PathBuf>,
    #[arg(long)]
    require_rights_filtered: bool,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    #[arg(long, value_enum, default_value = "all")]
    engine: EngineSelector,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    artifacts: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct VerifyArgs {
    #[arg(long, value_enum, default_value = "wasm")]
    engine: EngineSelector,
    #[arg(long, value_enum, default_value = "quick")]
    scope: Scope,
    #[arg(long)]
    shard: Option<String>,
    #[arg(long)]
    report: Option<PathBuf>,
    #[command(flatten)]
    input: InputArgs,
}

#[derive(Debug, Args)]
struct BuildArgs {
    #[arg(long, value_enum, default_value = "wasm")]
    engine: EngineSelector,
    #[arg(long, default_value = "dist")]
    out_dir: PathBuf,
    #[arg(long, default_value_t = site::DEFAULT_SIZE_LIMIT)]
    size_limit_bytes: u64,
    #[command(flatten)]
    input: InputArgs,
}

#[derive(Debug, Args)]
struct VisualArgs {
    #[arg(long)]
    baseline: PathBuf,
    #[arg(long)]
    candidate: PathBuf,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, default_value = "visual-report")]
    out_dir: PathBuf,
    #[arg(long)]
    approval: Option<PathBuf>,
    #[arg(long)]
    browser: Option<String>,
}

#[derive(Debug, Args)]
struct FullArgs {
    #[arg(long)]
    baseline: PathBuf,
    #[arg(long, default_value = "dist")]
    out_dir: PathBuf,
    #[arg(long, default_value = "visual-report")]
    visual_out_dir: PathBuf,
    #[arg(long)]
    approval: Option<PathBuf>,
    #[arg(long, default_value_t = site::DEFAULT_SIZE_LIMIT)]
    size_limit_bytes: u64,
    #[command(flatten)]
    input: InputArgs,
}

fn write_report(path: &Path, report: &VerificationReport) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create report directory {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}

fn verify(args: &VerifyArgs) -> Result<VerificationReport> {
    let report = verify::run(
        &VerifyOptions {
            root: &args.input.root,
            engine: args.engine,
            scope: args.scope,
            shard: args.shard.as_deref(),
            corpus: args.input.corpus.as_deref(),
            artifacts: args.input.artifacts.as_deref(),
            diagnostics_baseline: args.input.diagnostics_baseline.as_deref(),
            require_rights_filtered: args.input.require_rights_filtered,
        },
        |message| eprintln!("verify {message}"),
    )?;
    if let Some(path) = &args.report {
        write_report(path, &report)?;
    }
    println!(
        "PASS: {} engine(s), {} edition(s), scope={:?}, shard={}",
        report.engines.len(),
        report.works.len(),
        report.scope,
        report.shard
    );
    Ok(report)
}

fn doctor(args: &DoctorArgs) -> Result<()> {
    let checks = doctor::run(&args.root, args.engine, args.artifacts.as_deref())?;
    for check in &checks {
        println!(
            "{} {:<7} {}",
            if check.ok { "ok " } else { "ERR" },
            check.engine,
            check.detail
        );
    }
    if checks.iter().any(|check| !check.ok) {
        bail!("one or more requested engines are unavailable");
    }
    Ok(())
}

fn build(args: &BuildArgs) -> Result<VerificationReport> {
    let report = site::build(
        &BuildOptions {
            root: &args.input.root,
            out_dir: &args.out_dir,
            engine: args.engine,
            corpus: args.input.corpus.as_deref(),
            artifacts: args.input.artifacts.as_deref(),
            diagnostics_baseline: args.input.diagnostics_baseline.as_deref(),
            require_rights_filtered: args.input.require_rights_filtered,
            size_limit: args.size_limit_bytes,
        },
        |message| eprintln!("build {message}"),
    )?;
    println!(
        "PASS: built {} edition(s) with {} engine(s) into {}",
        report.works.len(),
        report.engines.len(),
        args.input.root.join(&args.out_dir).display()
    );
    Ok(report)
}

fn visual(args: &VisualArgs) -> Result<()> {
    let mut command = ProcessCommand::new("bun");
    command
        .current_dir(&args.root)
        .args(["run", "src/visual.ts", "--baseline"])
        .arg(&args.baseline)
        .arg("--candidate")
        .arg(&args.candidate)
        .arg("--out-dir")
        .arg(&args.out_dir);
    if let Some(approval) = &args.approval {
        command.arg("--approval").arg(approval);
    }
    if let Some(browser) = &args.browser {
        command.arg("--browser").arg(browser);
    }
    let status = command.status().context("start browser visual verifier")?;
    if !status.success() {
        bail!("visual verification failed with {status}");
    }
    Ok(())
}

fn execute(command: Option<LabCommand>) -> Result<()> {
    match command {
        None => {
            verify(&VerifyArgs {
                engine: EngineSelector::Wasm,
                scope: Scope::Quick,
                shard: None,
                report: None,
                input: InputArgs {
                    root: PathBuf::from("."),
                    corpus: None,
                    artifacts: None,
                    diagnostics_baseline: None,
                    require_rights_filtered: false,
                },
            })?;
        }
        Some(LabCommand::Doctor(args)) => doctor(&args)?,
        Some(LabCommand::Verify(args)) => {
            verify(&args)?;
        }
        Some(LabCommand::Build(args)) => {
            build(&args)?;
        }
        Some(LabCommand::Visual(args)) => visual(&args)?,
        Some(LabCommand::Full(args)) => {
            let root = args.input.root.clone();
            doctor(&DoctorArgs {
                engine: EngineSelector::All,
                root: root.clone(),
                artifacts: args.input.artifacts.clone(),
            })?;
            build(&BuildArgs {
                engine: EngineSelector::All,
                out_dir: args.out_dir.clone(),
                size_limit_bytes: args.size_limit_bytes,
                input: InputArgs {
                    require_rights_filtered: true,
                    ..args.input
                },
            })?;
            visual(&VisualArgs {
                baseline: args.baseline,
                candidate: args.out_dir,
                root,
                out_dir: args.visual_out_dir,
                approval: args.approval,
                browser: None,
            })?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match execute(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
