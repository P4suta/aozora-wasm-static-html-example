use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use aozora_lab::artifacts::lowercase_hex;
use aozora_lab::doctor;
use aozora_lab::model::{EngineSelector, Scope, VerificationReport};
use aozora_lab::site::{self, BuildOptions};
use aozora_lab::verify::{self, VerifyOptions};
use clap::{Args, Parser, Subcommand, ValueEnum};
use sha2::{Digest, Sha256};

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
    Spellcheck,
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
    StableResolve(StableResolveArgs),
    StableFetch(StableFetchArgs),
}

#[derive(Debug, Args)]
struct StableResolveArgs {
    #[arg(long, default_value = "P4suta/aozora")]
    repository: String,
    #[arg(long, default_value = "aozora-wasm")]
    npm_package: String,
    #[arg(long, default_value = "aozora")]
    crate_name: String,
    #[arg(long, default_value = "aozora")]
    python_package: String,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum StablePlatform {
    Linux,
    Macos,
    Windows,
}

impl StablePlatform {
    fn native_archive(self, version: &str) -> String {
        match self {
            Self::Linux => format!("aozora-v{version}-x86_64-unknown-linux-gnu.tar.gz"),
            Self::Macos => format!("aozora-v{version}-aarch64-apple-darwin.tar.gz"),
            Self::Windows => format!("aozora-v{version}-x86_64-pc-windows-msvc.zip"),
        }
    }
}

#[derive(Debug, Args)]
struct StableFetchArgs {
    #[arg(long, default_value = "P4suta/aozora")]
    repository: String,
    #[arg(long)]
    tag: String,
    #[arg(long)]
    version: String,
    #[arg(long, value_enum)]
    platform: StablePlatform,
    #[arg(long)]
    out_dir: PathBuf,
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

fn stable_version(value: &str) -> Result<()> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        bail!("stable version must have numeric X.Y.Z form");
    }
    Ok(())
}

fn repository(value: &str) -> Result<()> {
    let Some((owner, name)) = value.split_once('/') else {
        bail!("repository must have owner/name form");
    };
    if owner.is_empty()
        || name.is_empty()
        || !owner
            .bytes()
            .chain(name.bytes())
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("repository contains unsupported characters");
    }
    Ok(())
}

fn output(command: &mut Command, label: &str) -> Result<String> {
    let output = command.output().with_context(|| format!("start {label}"))?;
    if !output.status.success() {
        bail!(
            "{label} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .with_context(|| format!("{label} emitted non-UTF-8 output"))
        .map(|value| value.trim().to_owned())
}

fn run_command(command: &mut Command, label: &str) -> Result<()> {
    let status = command.status().with_context(|| format!("start {label}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!("{label} failed with {status}")
    }
}

fn curl_json(url: &str, label: &str) -> Result<serde_json::Value> {
    let bytes = output(
        Command::new("curl").args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--header",
            "User-Agent: aozora-real-work-lab/0.1",
            url,
        ]),
        label,
    )?;
    serde_json::from_str(&bytes).with_context(|| format!("decode {label}"))
}

fn append_output(name: &str, value: &str) -> Result<()> {
    if value.contains('\n') || value.contains('\r') {
        bail!("workflow output {name} contains a line break");
    }
    let Ok(path) = std::env::var("GITHUB_OUTPUT") else {
        return Ok(());
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open GITHUB_OUTPUT {path}"))?;
    writeln!(file, "{name}={value}").with_context(|| format!("write workflow output {name}"))
}

fn stable_resolve(args: &StableResolveArgs) -> Result<()> {
    repository(&args.repository)?;
    let release: serde_json::Value = serde_json::from_str(&output(
        Command::new("gh").args(["api", &format!("repos/{}/releases/latest", args.repository)]),
        "resolve latest GitHub release",
    )?)
    .context("decode latest GitHub release")?;
    if release.get("draft").and_then(serde_json::Value::as_bool) != Some(false)
        || release
            .get("prerelease")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
    {
        bail!("latest GitHub release is not a published stable release");
    }
    let tag = release
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .context("latest GitHub release lacks tag_name")?;
    let version = tag
        .strip_prefix('v')
        .context("stable GitHub release tag must start with v")?;
    stable_version(version)?;
    let commit_sha = output(
        Command::new("gh").args([
            "api",
            &format!("repos/{}/commits/{tag}", args.repository),
            "--jq",
            ".sha",
        ]),
        "resolve stable release commit",
    )?;
    commit(&commit_sha)?;

    let npm: String = serde_json::from_str(&output(
        Command::new("npm").args([
            "view",
            &format!("{}@{version}", args.npm_package),
            "version",
            "--json",
        ]),
        "resolve npm release",
    )?)
    .context("decode npm version")?;
    if npm != version {
        bail!("npm version {npm} does not match GitHub release {version}");
    }

    let crate_metadata = curl_json(
        &format!(
            "https://crates.io/api/v1/crates/{}/{version}",
            args.crate_name
        ),
        "resolve crates.io release",
    )?;
    if crate_metadata
        .pointer("/version/num")
        .and_then(serde_json::Value::as_str)
        != Some(version)
    {
        bail!("crates.io does not expose the GitHub release version");
    }
    let python_metadata = curl_json(
        &format!(
            "https://pypi.org/pypi/{}/{version}/json",
            args.python_package
        ),
        "resolve PyPI release",
    )?;
    if python_metadata
        .pointer("/info/version")
        .and_then(serde_json::Value::as_str)
        != Some(version)
    {
        bail!("PyPI does not expose the GitHub release version");
    }

    append_output("tag", tag)?;
    append_output("version", version)?;
    append_output("commit", &commit_sha)?;
    println!("stable release resolved: {tag} at {commit_sha}");
    Ok(())
}

fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(lowercase_hex(hasher.finalize()))
}

fn verify_checksum(directory: &Path, filename: &str) -> Result<()> {
    let checksum_path = directory.join(format!("{filename}.sha256"));
    let contents = fs::read_to_string(&checksum_path)
        .with_context(|| format!("read {}", checksum_path.display()))?;
    let mut fields = contents.split_whitespace();
    let expected = fields.next().context("checksum lacks digest")?;
    let recorded = fields
        .next()
        .context("checksum lacks artifact filename")?
        .trim_start_matches('*');
    if fields.next().is_some() || recorded != filename {
        bail!(
            "{} does not name exactly {filename}",
            checksum_path.display()
        );
    }
    let actual = digest(&directory.join(filename))?;
    if actual != expected {
        bail!("stable artifact checksum mismatch for {filename}");
    }
    Ok(())
}

fn one_file(root: &Path, extension: &str, label: &str) -> Result<PathBuf> {
    let mut matches = fs::read_dir(root)
        .with_context(|| format!("read {}", root.display()))?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.to_string_lossy().ends_with(extension))
        .collect::<Vec<_>>();
    matches.sort();
    match matches.as_slice() {
        [path] => Ok(path.clone()),
        _ => bail!(
            "{label}: expected one {extension} file under {}, found {}",
            root.display(),
            matches.len()
        ),
    }
}

fn python_command() -> Result<&'static str> {
    ["python3", "python"]
        .into_iter()
        .find(|candidate| {
            Command::new(candidate)
                .args(["-m", "pip", "--version"])
                .output()
                .is_ok_and(|output| output.status.success())
        })
        .context("Python 3 with pip is required to fetch the stable wheel")
}

fn stable_fetch(args: &StableFetchArgs) -> Result<()> {
    repository(&args.repository)?;
    stable_version(&args.version)?;
    if args.tag != format!("v{}", args.version) {
        bail!("stable tag and version do not match");
    }
    if args.out_dir.exists() {
        bail!(
            "refusing to replace stable fetch directory {}",
            args.out_dir.display()
        );
    }
    let release = args.out_dir.join("release");
    let native = args.out_dir.join("native");
    let python = args.out_dir.join("python");
    for directory in [&release, &native, &python] {
        fs::create_dir_all(directory).with_context(|| format!("create {}", directory.display()))?;
    }

    let native_archive = args.platform.native_archive(&args.version);
    let mut github = Command::new("gh");
    github.args([
        "release",
        "download",
        &args.tag,
        "--repo",
        &args.repository,
        "--dir",
    ]);
    github.arg(&release);
    for pattern in [
        "aozora.wasm",
        "aozora.wasm.sha256",
        "aozora-go.tar.gz",
        "aozora-go.tar.gz.sha256",
    ] {
        github.args(["--pattern", pattern]);
    }
    run_command(&mut github, "download stable portable release assets")?;

    let mut native_download = Command::new("gh");
    native_download.args([
        "release",
        "download",
        &args.tag,
        "--repo",
        &args.repository,
        "--dir",
    ]);
    native_download.arg(&native);
    for pattern in [&native_archive, &format!("{native_archive}.sha256")] {
        native_download.args(["--pattern", pattern]);
    }
    run_command(&mut native_download, "download stable native release asset")?;

    run_command(
        Command::new("npm")
            .args([
                "pack",
                &format!("aozora-wasm@{}", args.version),
                "--pack-destination",
            ])
            .arg(&release),
        "download stable npm package",
    )?;
    let crate_path = release.join(format!("aozora-{}.crate", args.version));
    run_command(
        Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--header",
                "User-Agent: aozora-real-work-lab/0.1",
                "--output",
            ])
            .arg(&crate_path)
            .arg(format!(
                "https://crates.io/api/v1/crates/aozora/{}/download",
                args.version
            )),
        "download stable Rust crate",
    )?;
    run_command(
        Command::new(python_command()?)
            .args([
                "-m",
                "pip",
                "download",
                "--disable-pip-version-check",
                "--no-cache-dir",
                "--only-binary=:all:",
                "--no-deps",
                "--dest",
            ])
            .arg(&python)
            .arg(format!("aozora=={}", args.version)),
        "download stable Python wheel",
    )?;

    verify_checksum(&release, "aozora.wasm")?;
    verify_checksum(&release, "aozora-go.tar.gz")?;
    verify_checksum(&native, &native_archive)?;
    let crate_metadata = curl_json(
        &format!("https://crates.io/api/v1/crates/aozora/{}", args.version),
        "read stable crate checksum",
    )?;
    let expected = crate_metadata
        .pointer("/version/checksum")
        .and_then(serde_json::Value::as_str)
        .context("crates.io metadata lacks checksum")?;
    if digest(&crate_path)? != expected {
        bail!("stable crate checksum does not match crates.io metadata");
    }
    let npm = one_file(&release, ".tgz", "stable npm package")?;
    let wheel = one_file(&python, ".whl", "stable Python wheel")?;
    if !npm.to_string_lossy().contains(&args.version)
        || !wheel.to_string_lossy().contains(&args.version)
    {
        bail!("a stable registry returned a different package version");
    }
    println!(
        "stable artifacts fetched for {} ({:?})",
        args.tag, args.platform
    );
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
    run(root, "Rust tests", "cargo", &["test", "--locked"])
}

fn spellcheck(root: &Path) -> Result<()> {
    run(root, "spelling", "typos", &[])
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
    spellcheck(root)?;
    lint(root)?;
    run(root, "TypeScript", "bunx", &["tsc", "--noEmit"])?;
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
        Task::Spellcheck => spellcheck(Path::new(".")),
        Task::Test => test(Path::new(".")),
        Task::Release(args) => match args.command {
            ReleaseTask::Prepare(args) => prepare(&args),
            ReleaseTask::Verify(args) => verify(&args),
            ReleaseTask::BootstrapDiagnostics(args) => bootstrap_diagnostics(&args),
            ReleaseTask::Build(args) => build(&args),
            ReleaseTask::Visual(args) => visual(&args),
            ReleaseTask::AssertResults(args) => assert_results(&args),
            ReleaseTask::StableResolve(args) => stable_resolve(&args),
            ReleaseTask::StableFetch(args) => stable_fetch(&args),
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

    use super::{commit, repository, stable_version, unpack};

    #[test]
    fn accepts_only_pinned_commits() {
        assert!(commit("0123456789abcdef0123456789abcdef01234567").is_ok());
        assert!(commit("main").is_err());
        assert!(commit("0123456789ABCDEF0123456789ABCDEF01234567").is_err());
    }

    #[test]
    fn accepts_only_stable_registry_coordinates() {
        assert!(stable_version("1.2.3").is_ok());
        assert!(stable_version("1.2.3-rc.1").is_err());
        assert!(stable_version("latest").is_err());
        assert!(repository("P4suta/aozora").is_ok());
        assert!(repository("P4suta").is_err());
        assert!(repository("P4suta/../aozora").is_err());
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
