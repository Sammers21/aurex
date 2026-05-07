use std::process::Command;

#[test]
fn java_command_reports_runtime_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_ax"))
        .arg("java")
        .output()
        .expect("failed to execute ax java");

    assert!(
        output.status.success(),
        "ax java failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("java: "),
        "expected resolved java path in output, got: {stdout}"
    );
    assert!(
        stdout.to_ascii_lowercase().contains("version"),
        "expected java version in output, got: {stdout}"
    );
    assert!(
        !stdout.contains("JAVA_HOME"),
        "ax java should report the PATH-resolved runtime only, got: {stdout}"
    );
}
