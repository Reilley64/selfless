use std::process::Command;

#[test]
fn the_server_exits_naming_each_missing_smtp_setting() {
    let output = Command::new(env!("CARGO_BIN_EXE_selfless"))
        .env(
            "DATABASE__URL",
            "postgres://selfless:selfless@127.0.0.1:1/selfless",
        )
        .env("SMTP__HOST", "smtp.example.com")
        .env("SMTP__PORT", "587")
        .env_remove("SMTP__USERNAME")
        .env_remove("SMTP__PASSWORD")
        .env_remove("SMTP__FROM")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    for missing in ["SMTP__USERNAME", "SMTP__PASSWORD", "SMTP__FROM"] {
        assert!(stderr.contains(missing), "{missing} is not named: {stderr}");
    }
    for present in ["SMTP__HOST", "SMTP__PORT"] {
        assert!(!stderr.contains(present), "{present} is named: {stderr}");
    }
}
