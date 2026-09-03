use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::Instant;

pub type AppResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const SCHEMA_VERSION: u32 = 1;
const MAX_FILE_HASH_BYTES: u64 = 16 * 1024 * 1024;
const MAX_SYSROOT_HASH_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SCAN_ENTRIES: usize = 50_000;

#[derive(Debug, Parser)]
#[command(
    name = "reprotrace",
    version,
    about = "Explain observed Cargo rebuild evidence"
)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Run a command while recording Cargo and rustc evidence.
    Record {
        /// Trace destination. The default is .reprotrace/trace.json.
        #[arg(short, long, default_value = ".reprotrace/trace.json")]
        output: PathBuf,
        /// Command and arguments. Put -- before the command.
        #[arg(last = true, required = true)]
        command: Vec<OsString>,
    },
    /// Validate and print a trace as stable formatted JSON.
    Inspect { trace: PathBuf },
    /// Compare two traces and explain observed differences.
    Explain { trace_a: PathBuf, trace_b: PathBuf },
    /// Print the program version.
    Version,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Trace {
    pub schema_version: u32,
    pub tool: String,
    pub platform: String,
    pub command: Vec<String>,
    pub workspace: String,
    pub compiler: CompilerIdentity,
    pub environment: Vec<EnvironmentEntry>,
    pub omitted_environment: Vec<String>,
    pub packages: Vec<String>,
    pub inputs: Vec<InputEvidence>,
    pub fingerprints: Vec<FingerprintEvidence>,
    #[serde(default)]
    pub fingerprints_before: Vec<FingerprintEvidence>,
    #[serde(default)]
    pub fingerprint_scan_before: ScanStatus,
    #[serde(default)]
    pub fingerprint_scan_after: ScanStatus,
    pub rustc_invocations: Vec<RustcInvocation>,
    pub cache: CacheEvidence,
    pub result: ResultEvidence,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CompilerIdentity {
    pub rustc: String,
    pub sysroot_digest: Option<String>,
    pub sysroot_digest_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentEntry {
    pub name: String,
    pub value_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputEvidence {
    pub path: String,
    pub digest: Option<String>,
    pub bytes: Option<u64>,
    pub complete: bool,
    pub readable: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FingerprintEvidence {
    pub path: String,
    pub digest: Option<String>,
    pub bytes: Option<u64>,
    pub complete: bool,
    pub readable: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ScanStatus {
    pub status: String,
    pub entries: usize,
    pub truncated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RustcInvocation {
    pub argv: Vec<String>,
    pub cwd: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheEvidence {
    pub provider: String,
    pub outcome: String,
    pub hits: u64,
    pub misses: u64,
    pub log_status: String,
    pub expected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResultEvidence {
    pub status: String,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CacheLogSummary {
    pub hits: u64,
    pub misses: u64,
    pub malformed: bool,
}

#[derive(Debug)]
struct WorkspaceInfo {
    root: PathBuf,
    target_dir: PathBuf,
    warnings: Vec<String>,
}

#[derive(Debug)]
struct WalkResult {
    files: Vec<PathBuf>,
    entries: usize,
    truncated: bool,
}

#[derive(Debug)]
struct HashResult {
    digest: Option<String>,
    bytes: Option<u64>,
    complete: bool,
    readable: bool,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WrapperLogRecord {
    argv: Vec<String>,
    cwd: String,
    status: String,
    exit_code: Option<i32>,
    elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
struct WrapperLogRecordOut<'a> {
    argv: &'a [String],
    cwd: &'a str,
    status: &'a str,
    exit_code: Option<i32>,
    elapsed_ms: u128,
}

pub fn entry<I, T>(args: I) -> AppResult<i32>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    if env::var_os("REPROTRACE_WRAPPER_LOG").is_some() {
        return run_wrapper();
    }

    let cli = Cli::try_parse_from(args)?;
    match cli.command {
        CliCommand::Record { output, command } => {
            let path = if output.is_absolute() {
                output
            } else {
                env::current_dir()?.join(output)
            };
            if command.is_empty() {
                return Err(invalid_input("record requires a command after --"));
            }
            let trace = record_command(&path, &command)?;
            println!("recorded {}", path.display());
            if trace.schema_version != SCHEMA_VERSION {
                return Err(invalid_input("recorded trace has an unsupported schema"));
            }
            if trace.result.status != "success" {
                return Ok(trace.result.exit_code.unwrap_or(1));
            }
        }
        CliCommand::Inspect { trace } => {
            let trace = read_trace(&trace)?;
            println!("{}", serde_json::to_string_pretty(&trace)?);
        }
        CliCommand::Explain { trace_a, trace_b } => {
            let a = read_trace(&trace_a)?;
            let b = read_trace(&trace_b)?;
            print!("{}", explain_traces(&a, &b));
        }
        CliCommand::Version => {
            println!("reprotrace {}", env!("CARGO_PKG_VERSION"));
        }
    }
    Ok(0)
}

pub fn record_command(output: &Path, command: &[OsString]) -> AppResult<Trace> {
    if command.is_empty() {
        return Err(invalid_input("record requires a command"));
    }

    let cwd = env::current_dir()?;
    let workspace_info = discover_workspace(&cwd)?;
    let workspace = lexical_normalize(&workspace_info.root);
    let target_dir = lexical_normalize(&workspace_info.target_dir);
    let command_display = command
        .iter()
        .map(|value| normalize_argument(&value.to_string_lossy(), &workspace))
        .collect::<Vec<_>>();
    let packages = selected_packages(&command_display);
    let environment = capture_environment();
    let compiler = inspect_compiler();
    let (fingerprints_before, fingerprint_scan_before) =
        snapshot_fingerprints(&target_dir, &workspace);

    let output_parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(output_parent)?;
    let log_path = output_parent.join(format!(".reprotrace-rustc-{}.jsonl", std::process::id()));
    let _log_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&log_path)?;

    let original_wrapper = env::var_os("RUSTC_WRAPPER");
    let cache_provider = detect_cache_provider(original_wrapper.as_deref(), &workspace);
    let current_exe = env::current_exe()?;
    let mut child = Command::new(&command[0]);
    child
        .args(&command[1..])
        .current_dir(&cwd)
        .env("RUSTC_WRAPPER", &current_exe)
        .env("REPROTRACE_WRAPPER_LOG", &log_path)
        .env("REPROTRACE_WORKSPACE", &workspace)
        .env_remove("REPROTRACE_INNER_WRAPPER");
    if let Some(wrapper) = original_wrapper.as_deref() {
        if !wrapper.is_empty() && !same_executable(wrapper, &current_exe) {
            child.env("REPROTRACE_INNER_WRAPPER", wrapper);
        }
    }

    let start = Instant::now();
    let command_result = child.status();
    let command_elapsed = start.elapsed().as_millis();
    let (result, mut warnings) = match command_result {
        Ok(status) => (result_from_status(status, command_elapsed), Vec::new()),
        Err(error) => (
            ResultEvidence {
                status: "spawn-error".to_string(),
                exit_code: None,
                signal: None,
                elapsed_ms: command_elapsed,
            },
            vec![format!("could not start command: {error}")],
        ),
    };

    let (fingerprints, fingerprint_scan_after) = snapshot_fingerprints(&target_dir, &workspace);
    let (inputs, input_warnings) = collect_inputs(&target_dir, &workspace);
    warnings.extend(workspace_info.warnings);
    warnings.extend(input_warnings);
    let (rustc_invocations, wrapper_warnings) = read_wrapper_log(&log_path);
    warnings.extend(wrapper_warnings);
    if let Err(error) = fs::remove_file(&log_path) {
        warnings.push(format!("could not remove temporary wrapper log: {error}"));
    }

    let trace = Trace {
        schema_version: SCHEMA_VERSION,
        tool: "reprotrace".to_string(),
        platform: "linux".to_string(),
        command: command_display,
        workspace: "<workspace>".to_string(),
        compiler,
        environment: environment.entries,
        omitted_environment: environment.omitted,
        packages,
        inputs,
        fingerprints,
        fingerprints_before,
        fingerprint_scan_before,
        fingerprint_scan_after,
        rustc_invocations,
        cache: CacheEvidence {
            provider: cache_provider,
            outcome: "unknown".to_string(),
            hits: 0,
            misses: 0,
            log_status: "not-collected".to_string(),
            expected: "unknown".to_string(),
        },
        result,
        warnings,
    };

    write_trace(output, &trace)?;
    let verified = read_trace(output)?;
    if verified != trace {
        return Err(invalid_input(
            "trace read-back did not match the recorded value",
        ));
    }
    Ok(trace)
}

pub fn read_trace(path: &Path) -> AppResult<Trace> {
    let contents = fs::read_to_string(path)?;
    let trace: Trace = serde_json::from_str(&contents)
        .map_err(|error| invalid_input(format!("malformed trace: {error}")))?;
    if trace.schema_version != SCHEMA_VERSION {
        return Err(invalid_input(format!(
            "unsupported trace schema version {}",
            trace.schema_version
        )));
    }
    if trace.tool != "reprotrace" {
        return Err(invalid_input("trace was not produced by reprotrace"));
    }
    Ok(trace)
}

pub fn explain_traces(a: &Trace, b: &Trace) -> String {
    let subject = if b.packages.is_empty() {
        "workspace package".to_string()
    } else {
        b.packages.join(", ")
    };
    let observed_rebuild = !b.rustc_invocations.is_empty();
    let mut lines = Vec::new();
    if observed_rebuild {
        lines.push(format!("package {subject} rebuilt"));
    } else {
        lines.push(format!("package {subject} had no observed rustc rebuild"));
    }
    lines.push(String::new());
    lines.push("CAUSES".to_string());

    let differences = trace_differences(a, b);
    if differences.is_empty() {
        lines.push("1. no captured environment, compiler, or input difference".to_string());
        if observed_rebuild {
            lines.push(
                "2. unknown cause: Cargo or another build input used evidence not captured here"
                    .to_string(),
            );
        }
    } else {
        for (index, difference) in differences.iter().enumerate() {
            lines.push(format!("{}. observed difference: {difference}", index + 1));
        }
        lines.push(format!(
            "{}. inference: one or more observed differences may explain the rustc invocation; this trace does not attribute Cargo's internal decision to one item",
            differences.len() + 1
        ));
    }
    if !b.fingerprint_scan_after.is_complete() || !b.fingerprint_scan_before.is_complete() {
        lines.push(format!(
            "{}. unknown: fingerprint evidence was {}",
            lines.len() - 1,
            b.fingerprint_limit_description()
        ));
    }

    lines.push(String::new());
    lines.push("CACHE IMPACT".to_string());
    lines.push(format!(
        "{}: {} (hits {}, misses {})",
        b.cache.provider, b.cache.outcome, b.cache.hits, b.cache.misses
    ));
    if differences.is_empty() {
        lines.push("expected: hit or no build work; not proven by this trace".to_string());
    } else {
        lines.push(
            "expected: miss is consistent with changed evidence; cache-key attribution is unknown"
                .to_string(),
        );
    }
    if !b.warnings.is_empty() {
        lines.push(String::new());
        lines.push("LIMITATIONS".to_string());
        for warning in &b.warnings {
            lines.push(format!("- {warning}"));
        }
    }
    format!("{}\n", lines.join("\n"))
}

pub fn parse_sccache_log_bytes(bytes: &[u8]) -> CacheLogSummary {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return CacheLogSummary {
            malformed: true,
            ..CacheLogSummary::default()
        };
    };
    let mut summary = CacheLogSummary::default();
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if !line.trim().is_empty() && !lower.contains("sccache") {
            summary.malformed = true;
            continue;
        }
        if lower.contains("cache hit") {
            summary.hits += 1;
        } else if lower.contains("cache miss") {
            summary.misses += 1;
        }
    }
    summary
}

fn write_trace(path: &Path, trace: &Trace) -> AppResult<()> {
    if let Some(parent) = path.parent().filter(|value| !value.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    let json = serde_json::to_vec_pretty(trace)?;
    file.write_all(&json)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn run_wrapper() -> AppResult<i32> {
    let mut args = env::args_os().skip(1);
    let Some(actual_rustc) = args.next() else {
        return Err(invalid_input("rustc wrapper received no compiler path"));
    };
    let rustc_args = args.collect::<Vec<_>>();
    let workspace = env::var_os("REPROTRACE_WORKSPACE")
        .map(PathBuf::from)
        .unwrap_or(env::current_dir()?);
    let workspace = lexical_normalize(&workspace);
    let actual_display = normalize_argument(&actual_rustc.to_string_lossy(), &workspace);
    let mut argv = vec![actual_display];
    argv.extend(
        rustc_args
            .iter()
            .map(|value| normalize_argument(&value.to_string_lossy(), &workspace)),
    );

    let inner_wrapper = env::var_os("REPROTRACE_INNER_WRAPPER");
    let start = Instant::now();
    let status = if let Some(inner) = inner_wrapper.as_deref() {
        let mut command = Command::new(inner);
        command.arg(&actual_rustc).args(&rustc_args).status()?
    } else {
        let mut command = Command::new(&actual_rustc);
        command.args(&rustc_args).status()?
    };
    let elapsed_ms = start.elapsed().as_millis();
    let result = result_from_status(status, elapsed_ms);
    let cwd_display = normalize_path_for_record(&env::current_dir()?, &workspace);
    let record = WrapperLogRecordOut {
        argv: &argv,
        cwd: &cwd_display,
        status: &result.status,
        exit_code: result.exit_code,
        elapsed_ms,
    };
    if let Err(error) = append_wrapper_record(&record) {
        eprintln!("reprotrace wrapper logging failed: {error}");
    }
    Ok(result.exit_code.unwrap_or(1))
}

fn append_wrapper_record(record: &WrapperLogRecordOut<'_>) -> AppResult<()> {
    let path = env::var_os("REPROTRACE_WRAPPER_LOG")
        .ok_or_else(|| invalid_input("wrapper log path is missing"))?;
    let mut file = OpenOptions::new().append(true).open(path)?;
    let mut line = serde_json::to_vec(record)?;
    line.push(b'\n');
    file.write_all(&line)?;
    Ok(())
}

fn read_wrapper_log(path: &Path) -> (Vec<RustcInvocation>, Vec<String>) {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            return (
                Vec::new(),
                vec![format!("could not read rustc wrapper log: {error}")],
            )
        }
    };
    let mut records = Vec::new();
    let mut warnings = Vec::new();
    for (line_number, line) in contents.lines().enumerate() {
        match serde_json::from_str::<WrapperLogRecord>(line) {
            Ok(record) => records.push(RustcInvocation {
                argv: record.argv,
                cwd: record.cwd,
                status: record.status,
                exit_code: record.exit_code,
                elapsed_ms: record.elapsed_ms,
            }),
            Err(error) => warnings.push(format!(
                "malformed rustc wrapper log line {}: {error}",
                line_number + 1
            )),
        }
    }
    (records, warnings)
}

fn result_from_status(status: ExitStatus, elapsed_ms: u128) -> ResultEvidence {
    #[cfg(unix)]
    let signal = std::os::unix::process::ExitStatusExt::signal(&status);
    #[cfg(not(unix))]
    let signal = None;
    ResultEvidence {
        status: if status.success() {
            "success".to_string()
        } else if signal.is_some() {
            "signaled".to_string()
        } else {
            "failure".to_string()
        },
        exit_code: status.code(),
        signal,
        elapsed_ms,
    }
}

fn discover_workspace(cwd: &Path) -> AppResult<WorkspaceInfo> {
    let Some(manifest) = find_manifest(cwd) else {
        return Ok(WorkspaceInfo {
            root: cwd.to_path_buf(),
            target_dir: cwd.join("target"),
            warnings: vec!["no Cargo.toml found above the working directory".to_string()],
        });
    };
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .arg("--manifest-path")
        .arg(&manifest)
        .output()?;
    if !output.status.success() {
        return Ok(WorkspaceInfo {
            root: manifest
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| cwd.to_path_buf()),
            target_dir: cwd.join("target"),
            warnings: vec![format!(
                "cargo metadata failed with status {}",
                output.status
            )],
        });
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        invalid_input(format!("cargo metadata returned malformed JSON: {error}"))
    })?;
    let root = value
        .get("workspace_root")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| cwd.to_path_buf())
        });
    let target_dir = value
        .get("target_directory")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    Ok(WorkspaceInfo {
        root,
        target_dir,
        warnings: Vec::new(),
    })
}

fn find_manifest(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(path) = current {
        let candidate = path.join("Cargo.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        current = path.parent();
    }
    None
}

fn inspect_compiler() -> CompilerIdentity {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let identity = Command::new(&rustc)
        .arg("--version")
        .arg("--verbose")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unavailable".to_string());
    let sysroot = Command::new(&rustc)
        .arg("--print")
        .arg("sysroot")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
        .filter(|path| path.is_dir());
    let (sysroot_digest, complete) = sysroot.as_deref().map(digest_tree).unwrap_or((None, false));
    CompilerIdentity {
        rustc: identity,
        sysroot_digest,
        sysroot_digest_complete: complete,
    }
}

fn capture_environment() -> CapturedEnvironment {
    const NAMES: &[&str] = &[
        "AR",
        "CARGO_BUILD_BUILD_DIR",
        "CARGO_BUILD_RUSTC",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_HOME",
        "CARGO_INCREMENTAL",
        "CARGO_NET_OFFLINE",
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_PROFILE_DEV_OPT_LEVEL",
        "CARGO_REGISTRY_TOKEN",
        "CARGO_TARGET_DIR",
        "CARGO_TERM_COLOR",
        "CC",
        "CXX",
        "HOME",
        "LD",
        "PATH",
        "RUSTC",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTC_WRAPPER",
        "RUSTDOC",
        "RUSTDOCFLAGS",
        "RUSTFLAGS",
        "SOURCE_DATE_EPOCH",
        "SCCACHE_BASEDIRS",
        "SCCACHE_ERROR_LOG",
        "SCCACHE_LOG",
    ];
    let mut entries = Vec::new();
    let mut omitted = Vec::new();
    for name in NAMES {
        let Some(value) = env::var_os(name) else {
            continue;
        };
        if is_sensitive_name(name) {
            omitted.push((*name).to_string());
        } else {
            entries.push(EnvironmentEntry {
                name: (*name).to_string(),
                value_digest: digest_bytes(value.to_string_lossy().as_bytes()),
            });
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    omitted.sort();
    CapturedEnvironment { entries, omitted }
}

#[derive(Debug)]
struct CapturedEnvironment {
    entries: Vec<EnvironmentEntry>,
    omitted: Vec<String>,
}

fn detect_cache_provider(wrapper: Option<&OsStr>, workspace: &Path) -> String {
    if let Some(wrapper) = wrapper {
        if wrapper
            .to_string_lossy()
            .to_ascii_lowercase()
            .contains("sccache")
        {
            return "sccache".to_string();
        }
        if !wrapper.is_empty() {
            return "custom-wrapper".to_string();
        }
    }
    for name in ["config.toml", "config"] {
        let path = workspace.join(".cargo").join(name);
        if let Ok(contents) = fs::read_to_string(path) {
            if contents.to_ascii_lowercase().contains("sccache") {
                return "sccache".to_string();
            }
        }
    }
    "none".to_string()
}

fn snapshot_fingerprints(
    target_dir: &Path,
    workspace: &Path,
) -> (Vec<FingerprintEvidence>, ScanStatus) {
    if !target_dir.exists() {
        return (
            Vec::new(),
            ScanStatus {
                status: "missing".to_string(),
                entries: 0,
                truncated: false,
                error: Some("target directory does not exist".to_string()),
            },
        );
    }
    let walk = match walk_files(target_dir, MAX_SCAN_ENTRIES) {
        Ok(walk) => walk,
        Err(error) => {
            return (
                Vec::new(),
                ScanStatus {
                    status: "unreadable".to_string(),
                    entries: 0,
                    truncated: false,
                    error: Some(error.to_string()),
                },
            )
        }
    };
    let mut evidence = Vec::new();
    for path in walk.files {
        if !path.components().any(|component| {
            matches!(component, Component::Normal(value) if value == OsStr::new(".fingerprint"))
        }) {
            continue;
        }
        let hash = hash_file(&path, MAX_FILE_HASH_BYTES);
        evidence.push(FingerprintEvidence {
            path: normalize_path_for_record(&path, workspace),
            digest: hash.digest,
            bytes: hash.bytes,
            complete: hash.complete,
            readable: hash.readable,
            error: hash.error,
        });
    }
    evidence.sort_by(|a, b| a.path.cmp(&b.path));
    (
        evidence,
        ScanStatus {
            status: if walk.truncated {
                "truncated".to_string()
            } else {
                "complete".to_string()
            },
            entries: walk.entries,
            truncated: walk.truncated,
            error: None,
        },
    )
}

fn collect_inputs(target_dir: &Path, workspace: &Path) -> (Vec<InputEvidence>, Vec<String>) {
    let walk = match walk_files(target_dir, MAX_SCAN_ENTRIES) {
        Ok(walk) => walk,
        Err(error) => {
            return (
                Vec::new(),
                vec![format!("could not scan dep-info files: {error}")],
            )
        }
    };
    let target_dir = lexical_normalize(target_dir);
    let mut inputs = BTreeMap::new();
    let mut warnings = Vec::new();
    for dep_info in walk
        .files
        .iter()
        .filter(|path| path.extension() == Some(OsStr::new("d")))
    {
        let contents = match fs::read_to_string(dep_info) {
            Ok(contents) => contents,
            Err(error) => {
                warnings.push(format!(
                    "could not read dep-info {}: {error}",
                    normalize_path_for_record(dep_info, workspace)
                ));
                continue;
            }
        };
        let paths = match parse_dep_info(&contents) {
            Ok(paths) => paths,
            Err(error) => {
                warnings.push(format!(
                    "could not parse dep-info {}: {error}",
                    normalize_path_for_record(dep_info, workspace)
                ));
                continue;
            }
        };
        for raw_path in paths {
            let absolute = if Path::new(&raw_path).is_absolute() {
                lexical_normalize(Path::new(&raw_path))
            } else {
                lexical_normalize(&workspace.join(&raw_path))
            };
            if absolute.starts_with(&target_dir) {
                continue;
            }
            let display = normalize_path_for_record(&absolute, workspace);
            let hash = hash_file(&absolute, MAX_FILE_HASH_BYTES);
            let candidate = InputEvidence {
                path: display.clone(),
                digest: hash.digest,
                bytes: hash.bytes,
                complete: hash.complete,
                readable: hash.readable,
                error: hash.error,
            };
            inputs
                .entry(display)
                .and_modify(|existing: &mut InputEvidence| {
                    if !existing.readable && candidate.readable {
                        *existing = candidate.clone();
                    }
                })
                .or_insert(candidate);
        }
    }
    if walk.truncated {
        warnings.push("dep-info scan reached its entry limit".to_string());
    }
    (inputs.into_values().collect(), warnings)
}

fn parse_dep_info(contents: &str) -> Result<Vec<String>, String> {
    let mut logical = String::new();
    for line in contents.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        logical.push_str(line);
        if line.ends_with('\\') {
            logical.pop();
            logical.push(' ');
        } else {
            logical.push(' ');
        }
    }
    let colon = logical
        .char_indices()
        .find(|(index, character)| *character == ':' && !is_escaped(&logical, *index))
        .map(|(index, _)| index)
        .ok_or_else(|| "missing target separator".to_string())?;
    Ok(parse_make_words(&logical[colon + 1..]))
}

fn parse_make_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn is_escaped(value: &str, index: usize) -> bool {
    let bytes = value.as_bytes();
    let mut backslashes = 0;
    let mut cursor = index;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 == 1
}

fn walk_files(root: &Path, max_entries: usize) -> io::Result<WalkResult> {
    if !root.exists() {
        return Ok(WalkResult {
            files: Vec::new(),
            entries: 0,
            truncated: false,
        });
    }
    let mut stack = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut entries = 0;
    let mut truncated = false;
    while let Some(path) = stack.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            files.push(path);
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        let mut children = Vec::new();
        for entry in fs::read_dir(&path)? {
            let child = entry?.path();
            entries += 1;
            if entries >= max_entries {
                truncated = true;
                break;
            }
            children.push(child);
        }
        children.sort_by(|a, b| b.to_string_lossy().cmp(&a.to_string_lossy()));
        stack.extend(children);
        if truncated {
            break;
        }
    }
    files.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    Ok(WalkResult {
        files,
        entries,
        truncated,
    })
}

fn hash_file(path: &Path, max_bytes: u64) -> HashResult {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return HashResult {
                digest: None,
                bytes: None,
                complete: false,
                readable: false,
                error: Some(error.to_string()),
            }
        }
    };
    if !metadata.is_file() {
        return HashResult {
            digest: None,
            bytes: Some(metadata.len()),
            complete: false,
            readable: false,
            error: Some("path is not a regular file".to_string()),
        };
    }
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            return HashResult {
                digest: None,
                bytes: Some(metadata.len()),
                complete: false,
                readable: false,
                error: Some(error.to_string()),
            }
        }
    };
    let mut hasher = Sha256::new();
    let mut remaining = max_bytes;
    let mut buffer = [0u8; 8192];
    while remaining > 0 {
        let read_size = remaining.min(buffer.len() as u64) as usize;
        let count = match file.read(&mut buffer[..read_size]) {
            Ok(count) => count,
            Err(error) => {
                return HashResult {
                    digest: None,
                    bytes: Some(metadata.len()),
                    complete: false,
                    readable: false,
                    error: Some(error.to_string()),
                }
            }
        };
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        remaining -= count as u64;
    }
    let complete = metadata.len() <= max_bytes;
    let mut digest = hasher.finalize().to_vec();
    if !complete {
        digest.extend_from_slice(b":prefix");
    }
    HashResult {
        digest: Some(hex_digest(&digest)),
        bytes: Some(metadata.len()),
        complete,
        readable: true,
        error: None,
    }
}

fn digest_tree(root: &Path) -> (Option<String>, bool) {
    let walk = match walk_files(root, MAX_SCAN_ENTRIES) {
        Ok(walk) => walk,
        Err(_) => return (None, false),
    };
    let mut hasher = Sha256::new();
    let mut remaining = MAX_SYSROOT_HASH_BYTES;
    let mut complete = !walk.truncated;
    for path in walk.files {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        hasher.update(relative.to_string_lossy().as_bytes());
        let hash = hash_file(&path, remaining);
        if let Some(digest) = hash.digest {
            hasher.update(digest.as_bytes());
        }
        hasher.update(hash.bytes.unwrap_or(0).to_le_bytes());
        if hash.bytes.unwrap_or(0) > remaining {
            remaining = 0;
            complete = false;
        } else {
            remaining -= hash.bytes.unwrap_or(0);
        }
        if remaining == 0 {
            complete = false;
            break;
        }
    }
    (Some(hex_digest(&hasher.finalize())), complete)
}

fn normalize_argument(value: &str, workspace: &Path) -> String {
    if is_sensitive_argument(value) {
        if let Some((key, _)) = value.split_once('=') {
            return format!("{key}=<redacted>");
        }
        return "<redacted>".to_string();
    }
    if value.starts_with('/') {
        return normalize_path_for_record(Path::new(value), workspace);
    }
    if let Some((key, value)) = value.split_once('=') {
        if value.starts_with('/') {
            return format!(
                "{key}={}",
                normalize_path_for_record(Path::new(value), workspace)
            );
        }
    }
    value.to_string()
}

fn normalize_path_for_record(path: &Path, workspace: &Path) -> String {
    let normalized_path = lexical_normalize(path);
    let normalized_workspace = lexical_normalize(workspace);
    if let Ok(relative) = normalized_path.strip_prefix(&normalized_workspace) {
        if relative.as_os_str().is_empty() {
            ".".to_string()
        } else {
            relative.to_string_lossy().replace('\\', "/")
        }
    } else {
        format!(
            "<external>/{}",
            digest_bytes(normalized_path.to_string_lossy().as_bytes())
        )
    }
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized
}

fn selected_packages(command: &[String]) -> Vec<String> {
    let mut packages = BTreeSet::new();
    let mut index = 0;
    while index < command.len() {
        let value = &command[index];
        if value == "-p" || value == "--package" {
            if let Some(package) = command.get(index + 1) {
                packages.insert(package.clone());
            }
            index += 1;
        } else if let Some(package) = value.strip_prefix("--package=") {
            packages.insert(package.to_string());
        }
        index += 1;
    }
    packages.into_iter().collect()
}

fn same_executable(value: &OsStr, current: &Path) -> bool {
    let candidate = Path::new(value);
    if candidate == current {
        return true;
    }
    candidate
        .file_name()
        .zip(current.file_name())
        .is_some_and(|(left, right)| left == right)
}

fn is_sensitive_name(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "token",
        "password",
        "passwd",
        "secret",
        "private_key",
        "credential",
        "auth",
        "cookie",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

fn is_sensitive_argument(value: &str) -> bool {
    if let Some((key, _)) = value.split_once('=') {
        return is_sensitive_name(key);
    }
    is_sensitive_name(value.trim_start_matches('-'))
}

fn digest_bytes(bytes: &[u8]) -> String {
    hex_digest(bytes)
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn trace_differences(a: &Trace, b: &Trace) -> Vec<String> {
    let mut differences = Vec::new();
    let env_a = a
        .environment
        .iter()
        .map(|entry| (entry.name.as_str(), entry.value_digest.as_str()))
        .collect::<BTreeMap<_, _>>();
    let env_b = b
        .environment
        .iter()
        .map(|entry| (entry.name.as_str(), entry.value_digest.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut env_names = BTreeSet::new();
    env_names.extend(env_a.keys().copied());
    env_names.extend(env_b.keys().copied());
    env_names.extend(a.omitted_environment.iter().map(String::as_str));
    env_names.extend(b.omitted_environment.iter().map(String::as_str));
    for name in env_names {
        let omitted_a = a.omitted_environment.iter().any(|item| item == name);
        let omitted_b = b.omitted_environment.iter().any(|item| item == name);
        if omitted_a || omitted_b {
            if omitted_a != omitted_b {
                differences.push(format!(
                    "environment presence changed: {name} (value omitted)"
                ));
            }
            continue;
        }
        match (env_a.get(name), env_b.get(name)) {
            (Some(left), Some(right)) if left != right => {
                differences.push(format!("environment changed: {name}"));
            }
            (None, Some(_)) => differences.push(format!("environment added: {name}")),
            (Some(_), None) => differences.push(format!("environment removed: {name}")),
            _ => {}
        }
    }

    if a.compiler.rustc != b.compiler.rustc {
        differences.push("compiler identity changed".to_string());
    }
    if a.compiler.sysroot_digest != b.compiler.sysroot_digest {
        differences.push("sysroot evidence changed".to_string());
    }

    let inputs_a = a
        .inputs
        .iter()
        .map(|input| (input.path.as_str(), input))
        .collect::<BTreeMap<_, _>>();
    let inputs_b = b
        .inputs
        .iter()
        .map(|input| (input.path.as_str(), input))
        .collect::<BTreeMap<_, _>>();
    let mut input_paths = BTreeSet::new();
    input_paths.extend(inputs_a.keys().copied());
    input_paths.extend(inputs_b.keys().copied());
    for path in input_paths {
        match (inputs_a.get(path), inputs_b.get(path)) {
            (Some(left), Some(right)) if left != right => {
                differences.push(format!("input changed: {path}"));
            }
            (None, Some(_)) => differences.push(format!("input added: {path}")),
            (Some(_), None) => differences.push(format!("input removed: {path}")),
            _ => {}
        }
    }
    differences
}

impl ScanStatus {
    fn is_complete(&self) -> bool {
        self.status == "complete"
    }
}

impl Trace {
    fn fingerprint_limit_description(&self) -> String {
        match (
            &self.fingerprint_scan_before.error,
            &self.fingerprint_scan_after.error,
        ) {
            (Some(before), Some(after)) => format!("limited ({before}; {after})"),
            (Some(error), None) | (None, Some(error)) => format!("limited ({error})"),
            (None, None)
                if self.fingerprint_scan_before.truncated
                    || self.fingerprint_scan_after.truncated =>
            {
                "truncated by the scan limit".to_string()
            }
            _ => "not complete".to_string(),
        }
    }
}

fn invalid_input(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn dep_info_parser_handles_escaped_spaces_and_continuations() {
        let input = r#"target: src/main.rs generated\ file.rs \
            include/header.h
"#;
        assert_eq!(
            parse_dep_info(input).expect("dep-info should parse"),
            vec![
                "src/main.rs".to_string(),
                "generated file.rs".to_string(),
                "include/header.h".to_string()
            ]
        );
    }

    #[test]
    fn paths_inside_workspace_are_relative_and_external_paths_are_hashed() {
        let workspace = Path::new("/tmp/project");
        assert_eq!(
            normalize_path_for_record(Path::new("/tmp/project/src/lib.rs"), workspace),
            "src/lib.rs"
        );
        let external = normalize_path_for_record(Path::new("/tmp/other/lib.rs"), workspace);
        assert!(external.starts_with("<external>/"));
        assert!(!external.contains("/tmp/other"));
    }

    #[test]
    fn sensitive_values_are_not_persisted() {
        assert!(is_sensitive_name("CARGO_REGISTRY_TOKEN"));
        assert!(is_sensitive_argument("--password=secret-value"));
        assert_eq!(
            normalize_argument("--password=secret-value", Path::new("/tmp/project")),
            "--password=<redacted>"
        );
    }

    #[test]
    fn cache_log_parser_handles_present_and_malformed_fixtures() {
        let present = include_bytes!("../fixtures/sccache/present.log");
        let malformed = include_bytes!("../fixtures/sccache/malformed.log");
        assert_eq!(
            parse_sccache_log_bytes(present),
            CacheLogSummary {
                hits: 1,
                misses: 1,
                malformed: false
            }
        );
        assert!(parse_sccache_log_bytes(malformed).malformed);
    }

    #[test]
    fn explanation_is_deterministic_and_reports_unknown_cause() {
        let base = test_trace(Vec::new(), vec!["src/main.rs".to_string()]);
        let mut changed = test_trace(Vec::new(), vec!["src/main.rs".to_string()]);
        changed.inputs[0].digest = Some("different".to_string());
        changed.rustc_invocations.push(RustcInvocation {
            argv: vec!["rustc".to_string()],
            cwd: "<workspace>".to_string(),
            status: "success".to_string(),
            exit_code: Some(0),
            elapsed_ms: 1,
        });
        let first = explain_traces(&base, &changed);
        let second = explain_traces(&base, &changed);
        assert_eq!(first, second);
        assert!(first.contains("input changed: src/main.rs"));
        assert!(first.contains("cache-key attribution is unknown"));
    }

    #[test]
    fn malformed_trace_is_rejected() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("malformed.json");
        fs::write(&path, "not json").expect("write fixture");
        assert!(read_trace(&path).is_err());
    }

    #[test]
    fn missing_fingerprint_directory_is_reported() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("target");
        let (evidence, scan) = snapshot_fingerprints(&target, directory.path());
        assert!(evidence.is_empty());
        assert_eq!(scan.status, "missing");
        assert!(scan.error.is_some());
    }

    #[test]
    fn absent_cache_wrapper_is_explicit() {
        let directory = tempfile::tempdir().expect("temporary directory");
        assert_eq!(detect_cache_provider(None, directory.path()), "none");
    }

    fn test_trace(environment: Vec<EnvironmentEntry>, paths: Vec<String>) -> Trace {
        Trace {
            schema_version: SCHEMA_VERSION,
            tool: "reprotrace".to_string(),
            platform: "linux".to_string(),
            command: vec!["cargo".to_string(), "build".to_string()],
            workspace: "<workspace>".to_string(),
            compiler: CompilerIdentity {
                rustc: "rustc test".to_string(),
                sysroot_digest: Some("sysroot".to_string()),
                sysroot_digest_complete: true,
            },
            environment,
            omitted_environment: Vec::new(),
            packages: vec!["foo".to_string()],
            inputs: paths
                .into_iter()
                .map(|path| InputEvidence {
                    path,
                    digest: Some("same".to_string()),
                    bytes: Some(1),
                    complete: true,
                    readable: true,
                    error: None,
                })
                .collect(),
            fingerprints: Vec::new(),
            fingerprints_before: Vec::new(),
            fingerprint_scan_before: ScanStatus {
                status: "complete".to_string(),
                entries: 0,
                truncated: false,
                error: None,
            },
            fingerprint_scan_after: ScanStatus {
                status: "complete".to_string(),
                entries: 0,
                truncated: false,
                error: None,
            },
            rustc_invocations: Vec::new(),
            cache: CacheEvidence {
                provider: "none".to_string(),
                outcome: "not-configured".to_string(),
                hits: 0,
                misses: 0,
                log_status: "not-collected".to_string(),
                expected: "unknown".to_string(),
            },
            result: ResultEvidence {
                status: "success".to_string(),
                exit_code: Some(0),
                signal: None,
                elapsed_ms: 1,
            },
            warnings: Vec::new(),
        }
    }
}
