use std::{
    fs,
    path::{Path, PathBuf},
    process::{self, Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[test]
fn ax_run_keeps_java_stdout_clean_when_captured() {
    let project = fresh_test_dir("run-stdout");
    write_basic_project(&project, "app says hello");

    let output = run_ax(&project, "run");

    assert!(
        output.status.success(),
        "ax run failed: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "app says hello\n");
    let stderr = stderr(&output);
    assert_eq!(
        stderr.matches("ax run").count(),
        1,
        "ax run should print one build header: {stderr}"
    );
    assert_no_ansi(&stderr);
}

#[test]
fn ax_run_passes_arguments_to_java_main() {
    let project = fresh_test_dir("run-args");
    write_args_project(&project);

    let output = run_ax_with_args(&project, &["run", "--config", "config.prod.yaml"]);

    assert!(
        output.status.success(),
        "ax run failed: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "--config|config.prod.yaml\n");
}

#[test]
fn ax_run_preserves_javac_warnings_without_polluting_stdout() {
    let project = fresh_test_dir("run-javac-warning");
    write_deprecated_project(&project);

    let output = run_ax(&project, "run");

    assert!(
        output.status.success(),
        "ax run failed: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "ran\n");

    let stderr = stderr(&output);
    assert!(
        stderr.contains("deprecated"),
        "missing javac warning output: {stderr}"
    );
    assert_eq!(
        stderr.matches("ax run").count(),
        1,
        "ax run should print one build header: {stderr}"
    );
    assert_no_ansi(&stderr);
}

#[test]
fn ax_build_plain_mode_writes_deterministic_stderr_without_ansi_when_captured() {
    let project = fresh_test_dir("build-plain");
    write_basic_project(&project, "built");

    let output = run_ax(&project, "build");

    assert!(
        output.status.success(),
        "ax build failed: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "");

    let stderr = stderr(&output);
    assert!(
        stderr.contains("ax build"),
        "missing command header: {stderr}"
    );
    assert!(
        stderr.contains("resolve ok"),
        "missing resolve log: {stderr}"
    );
    assert!(
        stderr.contains("compile ok"),
        "missing compile log: {stderr}"
    );
    assert!(
        stderr.contains("resources ok"),
        "missing resources log: {stderr}"
    );
    assert!(
        stderr.contains("package ok"),
        "missing package log: {stderr}"
    );
    assert!(stderr.contains("done "), "missing done log: {stderr}");
    assert_no_ansi(&stderr);
}

#[test]
fn ax_help_explains_commands_and_examples() {
    let output = Command::new(env!("CARGO_BIN_EXE_ax"))
        .arg("help")
        .output()
        .expect("failed to run ax help");

    assert!(
        output.status.success(),
        "ax help failed: {}",
        stderr(&output)
    );
    assert_eq!(stderr(&output), "");

    let stdout = stdout(&output);
    assert!(
        stdout.contains("Aurex (ax) builds small Java applications"),
        "missing overview: {stdout}"
    );
    for expected in [
        "Create a new Aurex project",
        "Add or update Maven dependencies",
        "Remove Maven dependencies",
        "Compile sources and package the project jar",
        "Build the project and run the jar",
        "Compile and run JUnit 5 tests",
        "Remove the target directory",
        "Format Java sources",
    ] {
        assert!(
            stdout.contains(expected),
            "missing help text `{expected}`: {stdout}"
        );
    }
    assert!(
        stdout.contains("Examples:"),
        "missing examples section: {stdout}"
    );
    assert!(
        stdout.contains("ax help build"),
        "missing command help example: {stdout}"
    );
    assert!(
        stdout.contains("ax reads ax.toml in the current directory"),
        "missing project file hint: {stdout}"
    );
    assert_no_ansi(&stdout);
}

fn fresh_test_dir(name: &str) -> PathBuf {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = PathBuf::from("target")
        .join("cli-output-tests")
        .join(format!("{}-{}-{id}", process::id(), name));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_basic_project(project: &Path, message: &str) {
    write_project_file(
        project,
        "ax.toml",
        r#"[package]
name = "cli-output"
version = "0.1.0"
jar_name = "cli-output.jar"
root = "./src"
main = "com.example.Main"

[dependencies]
"#,
    );
    write_project_file(
        project,
        "src/com/example/Main.java",
        &format!(
            r#"package com.example;

public class Main {{
    public static void main(String[] args) {{
        System.out.println("{message}");
    }}
}}
"#
        ),
    );
}

fn write_args_project(project: &Path) {
    write_project_file(
        project,
        "ax.toml",
        r#"[package]
name = "cli-args"
version = "0.1.0"
jar_name = "cli-args.jar"
root = "./src"
main = "com.example.Main"

[dependencies]
"#,
    );
    write_project_file(
        project,
        "src/com/example/Main.java",
        r#"package com.example;

public class Main {
    public static void main(String[] args) {
        System.out.println(String.join("|", args));
    }
}
"#,
    );
}

fn write_deprecated_project(project: &Path) {
    write_project_file(
        project,
        "ax.toml",
        r#"[package]
name = "cli-warning"
version = "0.1.0"
jar_name = "cli-warning.jar"
root = "./src"
main = "com.example.Main"

[dependencies]
"#,
    );
    write_project_file(
        project,
        "src/com/example/Main.java",
        r#"package com.example;

public class Main {
    public static void main(String[] args) {
        java.util.Date date = new java.util.Date();
        date.getYear();
        System.out.println("ran");
    }
}
"#,
    );
}

fn write_project_file(project: &Path, relative: &str, contents: &str) {
    let path = project.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn run_ax(project: &Path, subcommand: &str) -> Output {
    run_ax_with_args(project, &[subcommand])
}

fn run_ax_with_args(project: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ax"))
        .args(args)
        .current_dir(project)
        .output()
        .expect("failed to run ax")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n")
}

fn assert_no_ansi(output: &str) {
    assert!(
        !output.contains("\x1b["),
        "captured output should not contain ANSI escapes: {output:?}"
    );
}
