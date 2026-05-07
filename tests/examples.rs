use aurex::{build, config};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn basic_example_builds_and_runs() {
    assert_example_runs("basic", "basic.jar", "Hello, world!\n", false);
}

#[test]
fn vertx_example_builds_and_runs() {
    assert_example_runs("vertx", "vertx-example.jar", "Hello, Vert.x!\n", false);
}

#[test]
fn text_utils_example_builds_and_runs() {
    assert_example_runs(
        "text-utils",
        "text-utils.jar",
        "Dependency Driven Text Tools (4 words)\n",
        false,
    );
}

#[test]
fn json_report_example_builds_and_runs_as_self_contained_fat_jar() {
    assert_example_runs(
        "json-report",
        "json-report.jar",
        "{\"domain\":\"build\",\"checks\":3,\"passing\":true}\n",
        true,
    );
}

#[test]
fn cli_orders_example_builds_and_runs_as_self_contained_fat_jar() {
    assert_example_runs(
        "cli-orders",
        "cli-orders.jar",
        "north priority 2: 5 orders\n",
        true,
    );
}

fn assert_example_runs(
    example_dir: &str,
    jar_name: &str,
    expected_stdout: &str,
    require_self_contained: bool,
) {
    let dir = PathBuf::from("examples").join(example_dir);
    clean_generated_outputs(&dir, jar_name);

    let jar = build(config::read_toml(dir.to_str().unwrap()));
    let expected_jar = dir.join(jar_name);
    assert_eq!(jar, expected_jar);
    assert!(
        expected_jar.exists(),
        "{} should exist",
        expected_jar.display()
    );

    if require_self_contained {
        let deps_dir = dir.join("target").join("deps");
        assert!(deps_dir.exists(), "{} should exist", deps_dir.display());
        fs::remove_dir_all(deps_dir).unwrap();
    }

    let output = Command::new("java")
        .arg("-jar")
        .arg(&expected_jar)
        .output()
        .expect("failed to execute java");

    assert!(
        output.status.success(),
        "java -jar {} failed: {}",
        expected_jar.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let output_str = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert_eq!(output_str, expected_stdout);
}

fn clean_generated_outputs(dir: &Path, jar_name: &str) {
    let target = dir.join("target");
    if target.exists() {
        fs::remove_dir_all(target).unwrap();
    }

    let jar = dir.join(jar_name);
    if jar.exists() {
        fs::remove_file(jar).unwrap();
    }
}
