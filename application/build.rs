use std::env;
use std::process::Command;

fn main() {
    println!("cargo::rerun-if-changed=../telemetry");
    let status = Command::new("weaver")
        .args([
            "registry",
            "generate",
            "--quiet",
            "--registry",
            "../telemetry/registry",
            "--templates",
            "../telemetry/templates",
            "rust",
        ])
        .arg(env::var("OUT_DIR").unwrap())
        .status()
        .expect("weaver is installed, for example with `mise install`");
    assert!(
        status.success(),
        "weaver failed to generate the telemetry constants"
    );
}
