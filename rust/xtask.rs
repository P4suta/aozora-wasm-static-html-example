use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use aozora_lab::doctor;
use aozora_lab::model::{EngineSelector, Scope, VerificationReport};
use aozora_lab::site::{self, BuildOptions};
use aozora_lab::verify::{self, VerifyOptions};
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Repository and release orchestration")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Debug, Subcommand)]
enum Task {
    Check,
    Ci,
    E2e,
    Format,
    FormatCheck,
    Lint,
    Test,
    Release(ReleaseArgs),
}

#[derive(Debug, Args)]
struct ReleaseArgs {
    #[command(subcommand)]
    command: ReleaseTask,
}

#[derive(Debug, Subcommand)]
enum ReleaseTask {
    Prepare(PrepareArgs),
    Verify(VerifyArgs),
    BootstrapDiagnostics(BootstrapDiagnosticsArgs),
    Build(BuildArgs),
    Visual(VisualArgs),
    AssertResults(AssertResultsArgs),
}

#[derive(Debug, Args)]
struct BootstrapDiagnosticsArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, value_enum, default_value = "wasm")]
    engine: EngineSelector,
    #[arg(long, default_value = "_release/corpus/manifest.json")]
    corpus: PathBuf,
    #[arg(long, default_value = "_release/candidates/artifacts.json")]
    artifacts: PathBuf,
    #[arg(long, default_value = "bootstrap/diagnostics-baseline.json")]
    out: PathBuf,
}

#[derive(Debug, Args)]
struct PrepareArgs {
    #[arg(long)]
    lab_commit: Option<String>,
    #[arg(long)]
    aozora_commit: Option<String>,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    corpus_archive: PathBuf,
    #[arg(long)]
    candidate_archive: PathBuf,
    #[arg(long)]
    baseline_archive: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct VerifyArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    platform: String,
    #[arg(long)]
    shard: String,
    #[arg(long, default_value = "_release/corpus/manifest.json")]
    corpus: PathBuf,
    #[arg(long, default_value = "_release/candidates/artifacts.json")]
    artifacts: PathBuf,
    #[arg(long, default_value = "_inputs/diagnostics/diagnostics-baseline.json")]
    diagnostics_baseline: PathBuf,
    #[arg(long, default_value = "reports")]
    reports: PathBuf,
}

#[derive(Debug, Args)]
struct BuildArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, default_value = "_release/corpus/manifest.json")]
    corpus: PathBuf,
    #[arg(long, default_value = "_release/candidates/artifacts.json")]
    artifacts: PathBuf,
    #[arg(long, default_value = "_inputs/diagnostics/diagnostics-baseline.json")]
    diagnostics_baseline: PathBuf,
    #[arg(long, default_value = "candidate-site")]
    out_dir: PathBuf,
}

#[derive(Debug, Args)]
struct VisualArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, default_value = "_release/baseline")]
    baseline: PathBuf,
    #[arg(long, default_value = "candidate-site")]
    candidate: PathBuf,
    #[arg(long, default_value = "visual-report")]
    out_dir: PathBuf,
    #[arg(long, default_value = "_inputs/approval")]
    approval_dir: PathBuf,
    #[arg(long)]
    browser: Option<String>,
    #[arg(long)]
    work: Option<String>,
}

#[derive(Debug, Args)]
struct AssertResultsArgs {
    #[arg(long)]
    verify: String,
    #[arg(long)]
    visual: String,
}

fn commit(value: &str) -> Result<()> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("lab_commit must be a lowercase 40-character commit SHA");
    }
    Ok(())
}

fn repository_commit(root: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .context("read checked-out lab commit")?;
    if !output.status.success() {
        bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .context("git commit was not UTF-8")
        .map(|value| value.trim().to_owned())
}

fn check_checkout(root: &Path, expected: &str) -> Result<()> {
    commit(expected)?;
    let actual = repository_commit(root)?;
    if actual != expected {
        bail!("lab checkout mismatch: expected {expected}, received {actual}");
    }
    Ok(())
}

fn unpack(root: &Path, archive: &Path, destination: &Path) -> Result<()> {
    let archive = root.join(archive);
    let destination = root.join(destination);
    if destination.exists() {
        bail!(
            "refusing to replace extraction destination {}",
            destination.display()
        );
    }
    fs::create_dir_all(&destination)
        .with_context(|| format!("create {}", destination.display()))?;
    let file = File::open(&archive).with_context(|| format!("open {}", archive.display()))?;
    tar::Archive::new(file)
        .unpack(&destination)
        .with_context(|| {
            format!(
                "extract {} into {}",
                archive.display(),
                destination.display()
            )
        })
}

fn require_file(root: &Path, path: &Path) -> Result<()> {
    let path = root.join(path);
    if !path.is_file() {
        bail!("required release input is missing: {}", path.display());
    }
    Ok(())
}

fn prepare(args: &PrepareArgs) -> Result<()> {
    let expected_commit = args
        .lab_commit
        .clone()
        .or_else(|| std::env::var("AOZORA_LAB_COMMIT").ok())
        .context("pass --lab-commit or AOZORA_LAB_COMMIT")?;
    check_checkout(&args.root, &expected_commit)?;
    unpack(
        &args.root,
        &args.corpus_archive,
        Path::new("_release/corpus"),
    )?;
    unpack(
        &args.root,
        &args.candidate_archive,
        Path::new("_release/candidates"),
    )?;
    require_file(&args.root, Path::new("_release/corpus/manifest.json"))?;
    require_file(&args.root, Path::new("_release/candidates/artifacts.json"))?;
    let expected_aozora = args
        .aozora_commit
        .clone()
        .or_else(|| std::env::var("AOZORA_COMMIT").ok());
    if let Some(expected) = &expected_aozora {
        commit(expected)?;
        let bytes = fs::read(args.root.join("_release/candidates/artifacts.json"))?;
        let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
        if manifest
            .get("aozoraCommit")
            .and_then(serde_json::Value::as_str)
            != Some(expected)
        {
            bail!("candidate bundle was built for a different aozora commit");
        }
    }
    if let Some(archive) = &args.baseline_archive {
        unpack(&args.root, archive, Path::new("_release/baseline"))?;
        require_file(&args.root, Path::new("_release/baseline/index.html"))?;
    }
    println!("release inputs extracted from pinned archives");
    Ok(())
}

fn write_report(path: &Path, report: &VerificationReport) -> Result<()> {
    let parent = path.parent().context("report path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create report directory {}", parent.display()))?;
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("write report {}", path.display()))
}

fn verify(args: &VerifyArgs) -> Result<()> {
    if args.platform.is_empty() || !args.platform.bytes().all(|byte| byte.is_ascii_lowercase()) {
        bail!("platform must contain lowercase ASCII letters");
    }
    let checks = doctor::run(&args.root, EngineSelector::All, Some(&args.artifacts))?;
    if checks.iter().any(|check| !check.ok) {
        for check in checks.iter().filter(|check| !check.ok) {
            eprintln!("{}: {}", check.engine, check.detail);
        }
        bail!("one or more release engines are unavailable");
    }
    let report = verify::run(
        &VerifyOptions {
            root: &args.root,
            engine: EngineSelector::All,
            scope: Scope::Full,
            shard: Some(&args.shard),
            corpus: Some(&args.corpus),
            artifacts: Some(&args.artifacts),
            diagnostics_baseline: Some(&args.diagnostics_baseline),
            require_rights_filtered: true,
        },
        |message| eprintln!("verify {message}"),
    )?;
    let shard = report
        .shard
        .split_once('/')
        .context("verified shard lacks i/n framing")?
        .0;
    let path = args
        .root
        .join(&args.reports)
        .join(format!("{}-{shard}.json", args.platform));
    write_report(&path, &report)?;
    println!(
        "release parity passed for {} engine(s), {} edition(s), shard {}",
        report.engines.len(),
        report.works.len(),
        report.shard
    );
    Ok(())
}

fn bootstrap_diagnostics(args: &BootstrapDiagnosticsArgs) -> Result<()> {
    verify::bootstrap_diagnostics(
        &VerifyOptions {
            root: &args.root,
            engine: args.engine,
            scope: Scope::Full,
            shard: None,
            corpus: Some(&args.corpus),
            artifacts: Some(&args.artifacts),
            diagnostics_baseline: None,
            require_rights_filtered: true,
        },
        &args.root.join(&args.out),
        |message| eprintln!("bootstrap diagnostics {message}"),
    )?;
    println!(
        "diagnostics bootstrap candidate written to {}",
        args.out.display()
    );
    Ok(())
}

fn build(args: &BuildArgs) -> Result<()> {
    let report = site::build(
        &BuildOptions {
            root: &args.root,
            out_dir: &args.out_dir,
            engine: EngineSelector::All,
            corpus: Some(&args.corpus),
            artifacts: Some(&args.artifacts),
            diagnostics_baseline: Some(&args.diagnostics_baseline),
            require_rights_filtered: true,
            size_limit: site::DEFAULT_SIZE_LIMIT,
        },
        |message| eprintln!("build {message}"),
    )?;
    println!(
        "release site built from {} engines and {} editions",
        report.engines.len(),
        report.works.len()
    );
    Ok(())
}

fn visual(args: &VisualArgs) -> Result<()> {
    let approval_dir = args.root.join(&args.approval_dir);
    let approval = approval_dir.join("approval.json");
    let mut command = Command::new("bun");
    command
        .current_dir(&args.root)
        .args(["run", "src/visual.ts", "--baseline"])
        .arg(&args.baseline)
        .arg("--candidate")
        .arg(&args.candidate)
        .arg("--out-dir")
        .arg(&args.out_dir);
    if approval_dir.exists() {
        if !approval.is_file() {
            bail!("visual approval artifact lacks {}", approval.display());
        }
        command.arg("--approval").arg(&approval);
    }
    if let Some(browser) = &args.browser {
        command.arg("--browser").arg(browser);
    }
    if let Some(work) = &args.work {
        command.arg("--work").arg(work);
    }
    let status = command.status().context("start visual verifier")?;
    if !status.success() {
        bail!("visual verifier failed with {status}");
    }
    Ok(())
}

fn assert_results(args: &AssertResultsArgs) -> Result<()> {
    if args.verify != "success" || args.visual != "success" {
        bail!(
            "real-work release gate failed: verify={} visual={}",
            args.verify,
            args.visual
        );
    }
    println!("real-work release gate passed");
    Ok(())
}

fn run(root: &Path, label: &str, program: &str, arguments: &[&str]) -> Result<()> {
    eprintln!("xtask {label}");
    let status = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .status()
        .with_context(|| format!("start {label}: {program}"))?;
    if !status.success() {
        bail!("{label} failed with {status}");
    }
    Ok(())
}

fn format(root: &Path) -> Result<()> {
    run(
        root,
        "biome format",
        "bunx",
        &["biome", "format", "--write", "."],
    )?;
    run(root, "cargo fmt", "cargo", &["fmt"])
}

fn format_check(root: &Path) -> Result<()> {
    run(
        root,
        "biome format check",
        "bunx",
        &["biome", "format", "."],
    )?;
    run(root, "cargo fmt check", "cargo", &["fmt", "--", "--check"])
}

fn lint(root: &Path) -> Result<()> {
    run(
        root,
        "biome lint",
        "bunx",
        &["biome", "lint", "--error-on-warnings", "."],
    )?;
    run(
        root,
        "clippy",
        "cargo",
        &[
            "clippy",
            "--locked",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    )
}

fn test(root: &Path) -> Result<()> {
    run(root, "Bun tests", "bun", &["test"])?;
    run(root, "Rust tests", "cargo", &["test", "--locked"])
}

fn build_development_site(root: &Path) -> Result<()> {
    run(
        root,
        "development site build",
        "cargo",
        &[
            "run", "--quiet", "--locked", "--bin", "lab", "--", "build", "--engine", "wasm",
        ],
    )
}

fn check(root: &Path) -> Result<()> {
    format_check(root)?;
    lint(root)?;
    run(root, "TypeScript", "bunx", &["tsc", "--noEmit"])?;
    run(root, "Bun coverage", "bun", &["test", "--coverage"])?;
    run(root, "Rust tests", "cargo", &["test", "--locked"])?;
    run(root, "dead code", "bunx", &["knip"])?;
    build_development_site(root)
}

fn playwright(root: &Path) -> Result<()> {
    run(root, "Playwright", "bunx", &["playwright", "test"])
}

fn e2e(root: &Path) -> Result<()> {
    build_development_site(root)?;
    playwright(root)
}

fn ci(root: &Path) -> Result<()> {
    check(root)?;
    playwright(root)
}

fn execute(task: Task) -> Result<()> {
    match task {
        Task::Check => check(Path::new(".")),
        Task::Ci => ci(Path::new(".")),
        Task::E2e => e2e(Path::new(".")),
        Task::Format => format(Path::new(".")),
        Task::FormatCheck => format_check(Path::new(".")),
        Task::Lint => lint(Path::new(".")),
        Task::Test => test(Path::new(".")),
        Task::Release(args) => match args.command {
            ReleaseTask::Prepare(args) => prepare(&args),
            ReleaseTask::Verify(args) => verify(&args),
            ReleaseTask::BootstrapDiagnostics(args) => bootstrap_diagnostics(&args),
            ReleaseTask::Build(args) => build(&args),
            ReleaseTask::Visual(args) => visual(&args),
            ReleaseTask::AssertResults(args) => assert_results(&args),
        },
    }
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

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::path::Path;

    use anyhow::Result;
    use tar::{Builder, Header};

    use super::{commit, unpack};

    #[test]
    fn accepts_only_pinned_commits() {
        assert!(commit("0123456789abcdef0123456789abcdef01234567").is_ok());
        assert!(commit("main").is_err());
        assert!(commit("0123456789ABCDEF0123456789ABCDEF01234567").is_err());
    }

    #[test]
    fn extracts_tar_once_without_a_shell() -> Result<()> {
        let root = tempfile::tempdir()?;
        let file = File::create(root.path().join("input.tar"))?;
        let mut builder = Builder::new(file);
        let bytes = b"{}\n";
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, "manifest.json", &bytes[..])?;
        builder.finish()?;

        unpack(
            root.path(),
            Path::new("input.tar"),
            Path::new("nested/output"),
        )?;
        assert_eq!(
            fs::read_to_string(root.path().join("nested/output/manifest.json"))?,
            "{}\n"
        );
        assert!(
            unpack(
                root.path(),
                Path::new("input.tar"),
                Path::new("nested/output")
            )
            .is_err()
        );
        Ok(())
    }
}
