use aurex::{build, config};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};
use zip::ZipArchive;

#[test]
fn commons_lang_dependency_runs_from_maven_central() {
    let project = fresh_test_dir("commons-lang");
    write_project_file(
        &project,
        "ax.toml",
        r#"[package]
name = "commons-lang"
version = "0.1.0"
jar_name = "commons-lang.jar"
root = "./src"
main = "com.example.Main"

[dependencies]
"org.apache.commons:commons-lang3" = "3.14.0"
"#,
    );
    write_project_file(
        &project,
        "src/com/example/Main.java",
        r#"package com.example;

import org.apache.commons.lang3.StringUtils;

public class Main {
    public static void main(String[] args) {
        System.out.println(StringUtils.capitalize("aurex"));
    }
}
"#,
    );

    let jar = build(config::read_toml(project.to_str().unwrap()));
    let output = Command::new("java")
        .arg("-jar")
        .arg(jar)
        .output()
        .expect("failed to run generated jar");

    assert!(
        output.status.success(),
        "java failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), "Aurex\n");
}

#[test]
fn configured_resource_directory_is_packaged_in_fat_jar() {
    let project = fresh_test_dir("resource-dir");
    write_project_file(
        &project,
        "ax.toml",
        r#"[package]
name = "resource-dir"
version = "0.1.0"
jar_name = "resource-dir.jar"
root = "./src"
main = "com.example.Main"

[build]
jar_mode = "fat"

[resources]
dirs = ["settings"]
"#,
    );
    write_project_file(&project, "settings/message.txt", "Hello from resources\n");
    write_project_file(
        &project,
        "src/com/example/Main.java",
        r#"package com.example;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;

public class Main {
    public static void main(String[] args) throws Exception {
        try (InputStream input = Main.class.getClassLoader().getResourceAsStream("message.txt")) {
            if (input == null) {
                throw new IllegalStateException("missing resource");
            }
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            byte[] buffer = new byte[1024];
            int read;
            while ((read = input.read(buffer)) != -1) {
                output.write(buffer, 0, read);
            }
            System.out.print(new String(output.toByteArray(), StandardCharsets.UTF_8));
        }
    }
}
"#,
    );

    let jar = build(config::read_toml(project.to_str().unwrap()));
    let output = Command::new("java")
        .arg("-jar")
        .arg(jar)
        .output()
        .expect("failed to run generated jar");

    assert!(
        output.status.success(),
        "java failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), "Hello from resources\n");
}

#[test]
fn file_repository_dependency_runs_in_classpath_mode() {
    let root = fresh_test_dir("file-repo-classpath");
    let repository_url = create_local_maven_repository(&root);
    let project = root.join("app");
    write_local_dependency_project(&project, &repository_url, None);

    let output = run_ax(&project, "run");

    assert!(
        output.status.success(),
        "ax run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), "Hello from local repo\n");
    assert!(project.join("target/deps/message-1.0.0.jar").exists());
    assert!(project.join("local-app.jar").exists());

    let manifest = jar_entry_text(&project.join("local-app.jar"), "META-INF/MANIFEST.MF");
    assert!(manifest.contains("Main-Class: com.example.Main\n"));
    assert!(manifest.contains("Class-Path: target/deps/message-1.0.0.jar\n"));
}

#[test]
fn file_repository_dependency_runs_in_fat_mode() {
    let root = fresh_test_dir("file-repo-fat");
    let repository_url = create_local_maven_repository(&root);
    let project = root.join("app");
    write_local_dependency_project(&project, &repository_url, Some("fat"));

    let output = run_ax(&project, "run");

    assert!(
        output.status.success(),
        "ax run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), "Hello from local repo\n");
    assert!(project.join("local-app.jar").exists());

    let manifest = jar_entry_text(&project.join("local-app.jar"), "META-INF/MANIFEST.MF");
    assert!(manifest.contains("Main-Class: com.example.Main\n"));
    assert!(!manifest.contains("Class-Path:"));

    let entries = jar_entries(&project.join("local-app.jar"));
    assert!(
        entries
            .iter()
            .any(|entry| entry == "com/acme/Message.class")
    );
}

#[test]
fn file_repository_transitive_dependencies_are_downloaded_and_run() {
    let root = fresh_test_dir("file-repo-transitive");
    let repository_url = create_local_maven_repository(&root);
    let project = root.join("app");
    write_transitive_dependency_project(&project, &repository_url, None);

    let output = run_ax(&project, "run");

    assert!(
        output.status.success(),
        "ax run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), "Hello from local repo\n");
    assert!(project.join("target/deps/formatter-1.0.0.jar").exists());
    assert!(project.join("target/deps/message-1.0.0.jar").exists());
}

#[test]
fn cli_build_creates_self_contained_fat_jar_for_transitive_dependencies() {
    let root = fresh_test_dir("file-repo-fat-build");
    let repository_url = create_local_maven_repository(&root);
    let project = root.join("app");
    write_transitive_dependency_project(&project, &repository_url, Some("fat"));

    let build = run_ax(&project, "build");
    assert!(
        build.status.success(),
        "ax build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let jar = project.join("local-app.jar");
    let entries = jar_entries(&jar);
    assert!(
        entries
            .iter()
            .any(|entry| entry == "com/acme/Formatter.class")
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry == "com/acme/Message.class")
    );
    assert!(!jar_entry_text(&jar, "META-INF/MANIFEST.MF").contains("Class-Path:"));

    fs::remove_dir_all(project.join("target").join("deps")).unwrap();
    let output = Command::new("java")
        .arg("-jar")
        .arg(&jar)
        .output()
        .expect("failed to run generated fat jar");

    assert!(
        output.status.success(),
        "java failed after removing target/deps: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), "Hello from local repo\n");
}

fn fresh_test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from("target").join("integration-tests").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_project_file(project: &Path, relative: &str, contents: &str) {
    let path = project.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn write_local_dependency_project(project: &Path, repository_url: &str, jar_mode: Option<&str>) {
    let build_section = jar_mode
        .map(|mode| format!("\n[build]\njar_mode = \"{mode}\"\n"))
        .unwrap_or_default();
    write_project_file(
        project,
        "ax.toml",
        &format!(
            r#"[package]
name = "local-app"
version = "0.1.0"
jar_name = "local-app.jar"
root = "./src"
main = "com.example.Main"
{build_section}
[[repositories]]
name = "local"
url = "{repository_url}"

[dependencies]
"com.acme:message" = "1.0.0"
"#
        ),
    );
    write_project_file(
        project,
        "src/com/example/Main.java",
        r#"package com.example;

import com.acme.Message;

public class Main {
    public static void main(String[] args) {
        System.out.println(Message.text());
    }
}
"#,
    );
}

fn write_transitive_dependency_project(
    project: &Path,
    repository_url: &str,
    jar_mode: Option<&str>,
) {
    let build_section = jar_mode
        .map(|mode| format!("\n[build]\njar_mode = \"{mode}\"\n"))
        .unwrap_or_default();
    write_project_file(
        project,
        "ax.toml",
        &format!(
            r#"[package]
name = "local-app"
version = "0.1.0"
jar_name = "local-app.jar"
root = "./src"
main = "com.example.Main"
{build_section}
[[repositories]]
name = "local"
url = "{repository_url}"

[dependencies]
"com.acme:formatter" = "1.0.0"
"#
        ),
    );
    write_project_file(
        project,
        "src/com/example/Main.java",
        r#"package com.example;

import com.acme.Formatter;

public class Main {
    public static void main(String[] args) {
        System.out.println(Formatter.formatted());
    }
}
"#,
    );
}

fn create_local_maven_repository(root: &Path) -> String {
    let message_source_dir = root.join("message-src/com/acme");
    let message_classes_dir = root.join("message-classes");
    let message_artifact_dir = root.join("repo/com/acme/message/1.0.0");
    fs::create_dir_all(&message_source_dir).unwrap();
    fs::create_dir_all(&message_classes_dir).unwrap();
    fs::create_dir_all(&message_artifact_dir).unwrap();
    fs::write(
        message_source_dir.join("Message.java"),
        r#"package com.acme;

public class Message {
    public static String text() {
        return "Hello from local repo";
    }
}
"#,
    )
    .unwrap();

    let javac = Command::new("javac")
        .arg("-d")
        .arg(&message_classes_dir)
        .arg(message_source_dir.join("Message.java"))
        .status()
        .expect("failed to run javac for test dependency");
    assert!(javac.success(), "failed to compile test dependency");

    let jar = Command::new("jar")
        .arg("cf")
        .arg(message_artifact_dir.join("message-1.0.0.jar"))
        .arg("-C")
        .arg(&message_classes_dir)
        .arg(".")
        .status()
        .expect("failed to run jar for test dependency");
    assert!(jar.success(), "failed to package test dependency");

    fs::write(
        message_artifact_dir.join("message-1.0.0.pom"),
        r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId>
  <artifactId>message</artifactId>
  <version>1.0.0</version>
</project>
"#,
    )
    .unwrap();

    let formatter_source_dir = root.join("formatter-src/com/acme");
    let formatter_classes_dir = root.join("formatter-classes");
    let formatter_artifact_dir = root.join("repo/com/acme/formatter/1.0.0");
    fs::create_dir_all(&formatter_source_dir).unwrap();
    fs::create_dir_all(&formatter_classes_dir).unwrap();
    fs::create_dir_all(&formatter_artifact_dir).unwrap();
    fs::write(
        formatter_source_dir.join("Formatter.java"),
        r#"package com.acme;

public class Formatter {
    public static String formatted() {
        return Message.text();
    }
}
"#,
    )
    .unwrap();

    let javac = Command::new("javac")
        .arg("-cp")
        .arg(&message_classes_dir)
        .arg("-d")
        .arg(&formatter_classes_dir)
        .arg(formatter_source_dir.join("Formatter.java"))
        .status()
        .expect("failed to run javac for transitive test dependency");
    assert!(
        javac.success(),
        "failed to compile transitive test dependency"
    );

    let jar = Command::new("jar")
        .arg("cf")
        .arg(formatter_artifact_dir.join("formatter-1.0.0.jar"))
        .arg("-C")
        .arg(&formatter_classes_dir)
        .arg(".")
        .status()
        .expect("failed to package transitive test dependency");
    assert!(
        jar.success(),
        "failed to package transitive test dependency"
    );

    fs::write(
        formatter_artifact_dir.join("formatter-1.0.0.pom"),
        r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId>
  <artifactId>formatter</artifactId>
  <version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>com.acme</groupId>
      <artifactId>message</artifactId>
      <version>1.0.0</version>
    </dependency>
  </dependencies>
</project>
"#,
    )
    .unwrap();

    file_url(&root.join("repo"))
}

fn file_url(path: &Path) -> String {
    let mut path = path
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    if !path.starts_with('/') {
        path = format!("/{path}");
    }
    format!("file://{path}")
}

fn run_ax(project: &Path, subcommand: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ax"))
        .arg(subcommand)
        .current_dir(project)
        .output()
        .expect("failed to run ax")
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

fn jar_entries(jar_path: &Path) -> Vec<String> {
    let file = File::open(jar_path).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    (0..archive.len())
        .map(|index| archive.by_index(index).unwrap().name().to_string())
        .collect()
}

fn jar_entry_text(jar_path: &Path, name: &str) -> String {
    let file = File::open(jar_path).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let mut entry = archive.by_name(name).unwrap();
    let mut contents = String::new();
    entry.read_to_string(&mut contents).unwrap();
    contents.replace("\r\n", "\n")
}
