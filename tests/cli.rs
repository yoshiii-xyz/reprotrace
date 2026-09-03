use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_reprotrace"))
}

fn write_package(root: &Path, name: &str) {
    fs::create_dir_all(root.join("src")).expect("create source directory");
    fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
    .expect("write manifest");
    fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"fixture\"); }\n",
    )
    .expect("write source");
}

fn record(root: &Path, trace: &Path, extra_env: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(binary());
    command
        .current_dir(root)
        .arg("record")
        .arg("--output")
        .arg(trace)
        .arg("--")
        .arg("cargo")
        .arg("build")
        .arg("--offline");
    for (name, value) in extra_env {
        command.env(name, value);
    }
    command.output().expect("run record command")
}

#[test]
fn records_source_change_and_explains_it() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_package(directory.path(), "fixture");
    let first = directory.path().join("first.json");
    let second = directory.path().join("second.json");

    let output = record(directory.path(), &first, &[]);
    assert!(output.status.success(), "record failed: {:?}", output);
    fs::write(
        directory.path().join("src/main.rs"),
        "fn main() { println!(\"changed\"); }\n",
    )
    .expect("change source");
    let output = record(directory.path(), &second, &[]);
    assert!(output.status.success(), "record failed: {:?}", output);

    let explanation = Command::new(binary())
        .current_dir(directory.path())
        .args(["explain"])
        .arg(&first)
        .arg(&second)
        .output()
        .expect("run explain command");
    assert!(explanation.status.success());
    let text = String::from_utf8(explanation.stdout).expect("UTF-8 explanation");
    assert!(text.contains("input changed: src/main.rs"), "{text}");
    assert!(text.contains("rebuilt"), "{text}");
}

#[test]
fn records_environment_change_and_expects_a_cache_miss() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_package(directory.path(), "environment-fixture");
    let first = directory.path().join("first.json");
    let second = directory.path().join("second.json");

    let output = record(
        directory.path(),
        &first,
        &[("CARGO_BUILD_RUSTFLAGS", "-C debuginfo=0")],
    );
    assert!(output.status.success(), "record failed: {:?}", output);
    let output = record(
        directory.path(),
        &second,
        &[("CARGO_BUILD_RUSTFLAGS", "-C debuginfo=1")],
    );
    assert!(output.status.success(), "record failed: {:?}", output);

    let explanation = Command::new(binary())
        .args(["explain"])
        .arg(&first)
        .arg(&second)
        .output()
        .expect("run explain command");
    let text = String::from_utf8(explanation.stdout).expect("UTF-8 explanation");
    assert!(
        text.contains("environment changed: CARGO_BUILD_RUSTFLAGS"),
        "{text}"
    );
    assert!(text.contains("expected: miss"), "{text}");
}

#[test]
fn records_generated_file_change() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::create_dir_all(directory.path().join("src")).expect("create source directory");
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname = \"generated-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n",
    )
    .expect("write manifest");
    fs::write(
        directory.path().join("build.rs"),
        "const GENERATED: &str = include_str!(\"generated.txt\");\nfn main() { println!(\"cargo:rerun-if-changed=generated.txt\"); println!(\"{}\", GENERATED.len()); }\n",
    )
    .expect("write build script");
    fs::write(directory.path().join("generated.txt"), "first\n").expect("write generated file");
    fs::write(
        directory.path().join("src/main.rs"),
        "fn main() { println!(\"generated\"); }\n",
    )
    .expect("write source");
    let first = directory.path().join("first.json");
    let second = directory.path().join("second.json");

    let output = record(directory.path(), &first, &[]);
    assert!(output.status.success(), "record failed: {:?}", output);
    fs::write(directory.path().join("generated.txt"), "second\n").expect("change generated file");
    let output = record(directory.path(), &second, &[]);
    assert!(output.status.success(), "record failed: {:?}", output);

    let explanation = Command::new(binary())
        .args(["explain"])
        .arg(&first)
        .arg(&second)
        .output()
        .expect("run explain command");
    let text = String::from_utf8(explanation.stdout).expect("UTF-8 explanation");
    assert!(text.contains("input changed: generated.txt"), "{text}");
}

#[test]
fn unchanged_rebuild_reports_no_observed_rustc_work() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_package(directory.path(), "unchanged-fixture");
    let first = directory.path().join("first.json");
    let second = directory.path().join("second.json");
    let output = record(directory.path(), &first, &[]);
    assert!(output.status.success(), "record failed: {:?}", output);
    let output = record(directory.path(), &second, &[]);
    assert!(output.status.success(), "record failed: {:?}", output);

    let explanation = Command::new(binary())
        .args(["explain"])
        .arg(&first)
        .arg(&second)
        .output()
        .expect("run explain command");
    let text = String::from_utf8(explanation.stdout).expect("UTF-8 explanation");
    assert!(text.contains("had no observed rustc rebuild"), "{text}");
    assert!(text.contains("no captured environment, compiler, or input difference"));
}

#[test]
fn records_workspace_package_selection_and_redacts_secret_environment() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::create_dir_all(directory.path().join("first/src")).expect("create first source");
    fs::create_dir_all(directory.path().join("second/src")).expect("create second source");
    fs::write(
        directory.path().join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"first\", \"second\"]\n",
    )
    .expect("write workspace manifest");
    write_package(&directory.path().join("first"), "first");
    write_package(&directory.path().join("second"), "second");
    let trace = directory.path().join("workspace.json");

    let mut command = Command::new(binary());
    command
        .current_dir(directory.path())
        .env("CARGO_REGISTRY_TOKEN", "super-secret-value")
        .args(["record", "--output"])
        .arg(&trace)
        .args(["--", "cargo", "build", "--offline", "-p", "first"]);
    let output = command.output().expect("run workspace record");
    assert!(output.status.success(), "record failed: {:?}", output);

    let contents = fs::read_to_string(&trace).expect("read trace");
    assert!(!contents.contains("super-secret-value"));
    let value: serde_json::Value = serde_json::from_str(&contents).expect("trace JSON");
    assert_eq!(value["packages"], serde_json::json!(["first"]));
    assert_eq!(
        value["omitted_environment"],
        serde_json::json!(["CARGO_REGISTRY_TOKEN"])
    );
}

#[test]
fn inspect_is_stable_and_malformed_input_fails() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_package(directory.path(), "inspect-fixture");
    let trace = directory.path().join("trace.json");
    let output = record(directory.path(), &trace, &[]);
    assert!(output.status.success(), "record failed: {:?}", output);

    let first = Command::new(binary())
        .args(["inspect"])
        .arg(&trace)
        .output()
        .expect("run first inspect");
    let second = Command::new(binary())
        .args(["inspect"])
        .arg(&trace)
        .output()
        .expect("run second inspect");
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);

    let malformed = directory.path().join("malformed.json");
    fs::write(&malformed, "not JSON").expect("write malformed trace");
    let output = Command::new(binary())
        .args(["inspect"])
        .arg(&malformed)
        .output()
        .expect("run malformed inspect");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("malformed trace"));
}
