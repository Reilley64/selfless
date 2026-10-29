use std::env;
use std::process::Command;

fn main() {
    let mut reads_git = false;
    for (name, args) in [
        ("GIT_BRANCH", &["rev-parse", "--abbrev-ref", "HEAD"][..]),
        ("GIT_COMMIT", &["rev-parse", "HEAD"][..]),
        (
            "GIT_COMMIT_TIME",
            &[
                "show",
                "--no-patch",
                "--format=%cd",
                "--date=format-local:%Y-%m-%dT%H:%M:%SZ",
                "HEAD",
            ][..],
        ),
    ] {
        println!("cargo::rerun-if-env-changed={name}");
        let value = env::var(name).unwrap_or_else(|_| {
            reads_git = true;
            git(args)
                .expect("set GIT_BRANCH, GIT_COMMIT and GIT_COMMIT_TIME outside a git checkout")
        });
        let value = if name == "GIT_COMMIT" {
            &value[..8]
        } else {
            &value
        };
        println!("cargo::rustc-env={name}={value}");
    }
    if reads_git {
        let head = git(&["rev-parse", "--git-path", "HEAD"]).unwrap();
        println!("cargo::rerun-if-changed={head}");
        if let Some(reference) = git(&["symbolic-ref", "--quiet", "HEAD"]) {
            let reference = git(&["rev-parse", "--git-path", &reference]).unwrap();
            println!("cargo::rerun-if-changed={reference}");
        }
    }
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .env("TZ", "UTC")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).unwrap().trim().to_owned())
}
