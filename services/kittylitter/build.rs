use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=KITTYLITTER_BUILD_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_EVENT_PATH");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");
    if let Some(head_ref) = git_head_ref_path() {
        println!("cargo:rerun-if-changed={}", head_ref.display());
    }

    let package_version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
    if let Some(sha) = explicit_build_sha()
        .or_else(github_pull_request_head_sha)
        .or_else(github_sha)
        .or_else(git_sha)
    {
        println!("cargo:rustc-env=KITTYLITTER_BUILD_VERSION={package_version}+{sha}");
    }
}

fn explicit_build_sha() -> Option<String> {
    env::var("KITTYLITTER_BUILD_SHA")
        .ok()
        .and_then(|sha| normalize_sha(&sha))
}

fn github_sha() -> Option<String> {
    env::var("GITHUB_SHA")
        .ok()
        .and_then(|sha| normalize_sha(&sha))
}

fn github_pull_request_head_sha() -> Option<String> {
    let path = env::var("GITHUB_EVENT_PATH").ok()?;
    let event = fs::read_to_string(path).ok()?;
    let pull_request = event.find("\"pull_request\"")?;
    let head = event[pull_request..].find("\"head\"")? + pull_request;
    let sha = event[head..].find("\"sha\"")? + head;
    let colon = event[sha..].find(':')? + sha;
    let value = event[colon + 1..].trim_start();
    let value = value.strip_prefix('"')?;
    let value = value.split('"').next()?;
    normalize_sha(value)
}

fn git_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?;
    normalize_sha(&sha)
}

fn git_head_ref_path() -> Option<PathBuf> {
    let head = fs::read_to_string("../../.git/HEAD").ok()?;
    let reference = head.trim().strip_prefix("ref: ")?;
    Some(PathBuf::from("../../.git").join(reference))
}

fn normalize_sha(raw: &str) -> Option<String> {
    let sha = raw.trim();
    if sha.len() < 7 || !sha.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(sha.chars().take(12).collect())
}
