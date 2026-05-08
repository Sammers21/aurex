use std::{
    fs,
    path::{Path, PathBuf},
    process::{self, Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[test]
fn ax_add_and_remove_edit_dependencies_table() {
    let project = fresh_test_dir("add-remove");
    write_project_file(
        &project,
        "ax.toml",
        r#"[package]
name = "deps"
root = "./src"
main = "com.example.Main"

[dependencies]
"org.old:lib" = "1.0.0"
"#,
    );

    let add = run_ax_with_args(
        &project,
        &["add", "org.example:demo@1.2.3", "org.old:lib@2.0.0"],
    );
    assert!(add.status.success(), "ax add failed: {}", stderr(&add));
    assert!(stdout(&add).contains("added org.example:demo@1.2.3"));

    let manifest = read_project_file(&project, "ax.toml");
    assert!(manifest.contains("\"org.example:demo\" = \"1.2.3\""));
    assert!(manifest.contains("\"org.old:lib\" = \"2.0.0\""));

    let remove = run_ax_with_args(&project, &["remove", "org.example:demo"]);
    assert!(
        remove.status.success(),
        "ax remove failed: {}",
        stderr(&remove)
    );

    let manifest = read_project_file(&project, "ax.toml");
    assert!(!manifest.contains("org.example:demo"));
    assert!(manifest.contains("\"org.old:lib\" = \"2.0.0\""));

    let missing = run_ax_with_args(&project, &["remove", "org.missing:nope"]);
    assert!(!missing.status.success(), "missing remove should fail");
    assert!(stderr(&missing).contains("is not present"));
}

#[test]
fn ax_clean_removes_only_target_directory() {
    let project = fresh_test_dir("clean");
    write_project_file(&project, "target/classes/App.class", "compiled");
    write_project_file(&project, "app.jar", "jar");

    let output = run_ax_with_args(&project, &["clean"]);

    assert!(
        output.status.success(),
        "ax clean failed: {}",
        stderr(&output)
    );
    assert!(!project.join("target").exists());
    assert!(project.join("app.jar").exists());
}

#[test]
fn ax_test_and_t_alias_run_junit_console() {
    let project = fresh_test_dir("test");
    let repo = project.join("repo");
    install_fake_junit_console(&project, &repo);
    write_project_with_repository(&project, &repo);
    write_project_file(
        &project,
        "src/com/example/Main.java",
        r#"package com.example;

public class Main {
    public static String message() {
        return "tested";
    }
}
"#,
    );
    write_project_file(
        &project,
        "src/test/java/com/example/MainTest.java",
        r#"package com.example;

import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

public class MainTest {
    @Test
    public void messageMatches() {
        Assertions.assertEquals("tested", Main.message());
    }
}
"#,
    );

    let test = run_ax_with_args(&project, &["test"]);
    assert!(test.status.success(), "ax test failed: {}", stderr(&test));
    assert!(stdout(&test).contains("fake junit ran 1 tests"));

    let alias = run_ax_with_args(&project, &["t"]);
    assert!(alias.status.success(), "ax t failed: {}", stderr(&alias));
    assert!(stdout(&alias).contains("fake junit ran 1 tests"));
}

#[test]
fn ax_fmt_uses_google_java_format_when_no_eclipse_config_exists() {
    let project = fresh_test_dir("fmt-google");
    let repo = project.join("repo");
    install_fake_google_java_format(&project, &repo);
    write_project_with_repository(&project, &repo);
    write_project_file(
        &project,
        "src/com/example/Main.java",
        r#"package com.example;

public class Main{public static void main(String[] args){System.out.println("hi");}}
"#,
    );

    let output = run_ax_with_args(&project, &["fmt"]);

    assert!(
        output.status.success(),
        "ax fmt failed: {}",
        stderr(&output)
    );
    assert!(stdout(&output).contains("google-java-format"));
    assert!(
        read_project_file(&project, "src/com/example/Main.java").contains("public class Main {\n")
    );
}

#[test]
fn ax_fmt_uses_eclipse_jdt_when_eclipse_config_exists() {
    let project = fresh_test_dir("fmt-eclipse");
    let repo = project.join("repo");
    install_fake_eclipse_jdt(&project, &repo);
    write_project_with_repository(&project, &repo);
    write_project_file(
        &project,
        "eclipse-formatter.xml",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<profiles version="21">
  <profile kind="CodeFormatterProfile" name="Aurex" version="21">
    <setting id="org.eclipse.jdt.core.formatter.tabulation.char" value="space"/>
  </profile>
</profiles>
"#,
    );
    write_project_file(
        &project,
        "src/com/example/Main.java",
        r#"package com.example;

public class Main{public static void main(String[] args){System.out.println("hi");}}
"#,
    );

    let output = run_ax_with_args(&project, &["fmt"]);

    assert!(
        output.status.success(),
        "ax fmt failed: {}",
        stderr(&output)
    );
    assert!(stdout(&output).contains("Eclipse JDT"));
    assert!(
        read_project_file(&project, "src/com/example/Main.java").contains("public class Main {\n")
    );
}

fn fresh_test_dir(name: &str) -> PathBuf {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = PathBuf::from("target").join("command-tests").join(format!(
        "{}-{}-{id}",
        process::id(),
        name
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_project_with_repository(project: &Path, repo: &Path) {
    write_project_file(
        project,
        "ax.toml",
        &format!(
            r#"[package]
name = "commands"
root = "./src"
main = "com.example.Main"

[[repositories]]
name = "local"
url = "{}"

[dependencies]
"#,
            file_url(repo)
        ),
    );
}

fn install_fake_junit_console(project: &Path, repo: &Path) {
    let jar = project.join("fake-junit.jar");
    compile_jar(
        project,
        "fake-junit",
        &jar,
        &[
            (
                "org/junit/jupiter/api/Test.java",
                r#"package org.junit.jupiter.api;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.METHOD)
public @interface Test {
}
"#,
            ),
            (
                "org/junit/jupiter/api/Assertions.java",
                r#"package org.junit.jupiter.api;

import java.util.Objects;

public final class Assertions {
    private Assertions() {
    }

    public static void assertEquals(Object expected, Object actual) {
        if (!Objects.equals(expected, actual)) {
            throw new AssertionError("expected <" + expected + "> but was <" + actual + ">");
        }
    }
}
"#,
            ),
            (
                "org/junit/platform/console/ConsoleLauncher.java",
                r#"package org.junit.platform.console;

import java.io.File;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.stream.Stream;
import org.junit.jupiter.api.Test;

public final class ConsoleLauncher {
    private ConsoleLauncher() {
    }

    public static void main(String[] args) throws Exception {
        int count = 0;
        for (String entry : System.getProperty("java.class.path").split(File.pathSeparator)) {
            Path root = Paths.get(entry);
            if (!Files.isDirectory(root) || !root.toString().contains("test-classes")) {
                continue;
            }
            try (Stream<Path> stream = Files.walk(root)) {
                for (Path file : (Iterable<Path>) stream.filter(path -> path.toString().endsWith(".class"))::iterator) {
                    String name = root.relativize(file).toString()
                            .replace(File.separatorChar, '.')
                            .replaceAll("\\.class$", "");
                    if (name.contains("$")) {
                        continue;
                    }
                    Class<?> type = Class.forName(name);
                    Object instance = null;
                    for (Method method : type.getDeclaredMethods()) {
                        if (!method.isAnnotationPresent(Test.class)) {
                            continue;
                        }
                        if (!Modifier.isStatic(method.getModifiers())) {
                            if (instance == null) {
                                instance = type.getDeclaredConstructor().newInstance();
                            }
                        }
                        method.setAccessible(true);
                        try {
                            method.invoke(instance);
                        } catch (InvocationTargetException error) {
                            Throwable cause = error.getCause();
                            if (cause instanceof Exception) {
                                throw (Exception) cause;
                            }
                            if (cause instanceof Error) {
                                throw (Error) cause;
                            }
                            throw new RuntimeException(cause);
                        }
                        count++;
                    }
                }
            }
        }
        System.out.println("fake junit ran " + count + " tests");
    }
}
"#,
            ),
        ],
    );
    install_artifact(
        repo,
        "org.junit.platform",
        "junit-platform-console-standalone",
        "1.14.0",
        &jar,
    );
}

fn install_fake_google_java_format(project: &Path, repo: &Path) {
    let jar = project.join("fake-google-format.jar");
    compile_jar(
        project,
        "fake-google-format",
        &jar,
        &[(
            "com/google/googlejavaformat/java/Main.java",
            r#"package com.google.googlejavaformat.java;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.charset.StandardCharsets;

public final class Main {
    private Main() {
    }

    public static void main(String[] args) throws Exception {
        for (String arg : args) {
            if (arg.startsWith("-")) {
                continue;
            }
            Path file = Paths.get(arg);
            String source = new String(Files.readAllBytes(file), StandardCharsets.UTF_8)
                    .replace("public class Main{public static", "public class Main {\n    public static")
                    .replace("{System.out.println", " {\n        System.out.println")
                    .replace(";}}", ";\n    }\n}\n");
            Files.write(file, source.getBytes(StandardCharsets.UTF_8));
        }
    }
}
"#,
        )],
    );
    install_artifact(
        repo,
        "com.google.googlejavaformat",
        "google-java-format",
        "1.35.0",
        &jar,
    );
}

fn install_fake_eclipse_jdt(project: &Path, repo: &Path) {
    let jar = project.join("fake-eclipse-jdt.jar");
    compile_jar(
        project,
        "fake-eclipse-jdt",
        &jar,
        &[
            (
                "org/eclipse/jdt/core/formatter/CodeFormatter.java",
                r#"package org.eclipse.jdt.core.formatter;

import org.eclipse.text.edits.TextEdit;

public abstract class CodeFormatter {
    public static final int K_COMPILATION_UNIT = 8;

    public abstract TextEdit format(int kind, String source, int offset, int length, int indentationLevel, String lineSeparator);
}
"#,
            ),
            (
                "org/eclipse/jdt/core/ToolFactory.java",
                r#"package org.eclipse.jdt.core;

import java.util.Map;
import org.eclipse.jdt.core.formatter.CodeFormatter;
import org.eclipse.text.edits.TextEdit;

public final class ToolFactory {
    private ToolFactory() {
    }

    public static CodeFormatter createCodeFormatter(Map<String, String> options) {
        return new CodeFormatter() {
            @Override
            public TextEdit format(int kind, String source, int offset, int length, int indentationLevel, String lineSeparator) {
                String formatted = source
                        .replace("public class Main{public static", "public class Main {\n    public static")
                        .replace("{System.out.println", " {\n        System.out.println")
                        .replace(";}}", ";\n    }\n}\n");
                return new TextEdit(formatted);
            }
        };
    }
}
"#,
            ),
            (
                "org/eclipse/jface/text/Document.java",
                r#"package org.eclipse.jface.text;

public final class Document {
    private String text;

    public Document(String text) {
        this.text = text;
    }

    public String get() {
        return text;
    }

    public void set(String text) {
        this.text = text;
    }
}
"#,
            ),
            (
                "org/eclipse/text/edits/TextEdit.java",
                r#"package org.eclipse.text.edits;

import org.eclipse.jface.text.Document;

public final class TextEdit {
    private final String formatted;

    public TextEdit(String formatted) {
        this.formatted = formatted;
    }

    public void apply(Document document) {
        document.set(formatted);
    }
}
"#,
            ),
        ],
    );
    install_artifact(
        repo,
        "org.eclipse.jdt",
        "org.eclipse.jdt.core",
        "3.45.0",
        &jar,
    );
}

fn compile_jar(project: &Path, name: &str, jar: &Path, sources: &[(&str, &str)]) {
    let source_root = project.join(format!("{name}-src"));
    let classes = project.join(format!("{name}-classes"));
    for (relative, contents) in sources {
        write_project_file(&source_root, relative, contents);
    }

    let source_files = sources
        .iter()
        .map(|(relative, _)| source_root.join(relative))
        .collect::<Vec<_>>();
    fs::create_dir_all(&classes).unwrap();
    let javac = Command::new("javac")
        .arg("-d")
        .arg(&classes)
        .args(&source_files)
        .status()
        .expect("failed to run javac for fake tool");
    assert!(javac.success(), "failed to compile fake tool jar");

    let jar_status = Command::new("jar")
        .arg("cf")
        .arg(jar)
        .arg("-C")
        .arg(&classes)
        .arg(".")
        .status()
        .expect("failed to run jar for fake tool");
    assert!(jar_status.success(), "failed to package fake tool jar");
}

fn install_artifact(repo: &Path, group: &str, artifact: &str, version: &str, jar: &Path) {
    let artifact_dir = repo
        .join(group.replace('.', "/"))
        .join(artifact)
        .join(version);
    fs::create_dir_all(&artifact_dir).unwrap();
    fs::copy(jar, artifact_dir.join(format!("{artifact}-{version}.jar"))).unwrap();
    fs::write(
        artifact_dir.join(format!("{artifact}-{version}.pom")),
        format!(
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>{group}</groupId>
  <artifactId>{artifact}</artifactId>
  <version>{version}</version>
</project>
"#
        ),
    )
    .unwrap();
}

fn write_project_file(project: &Path, relative: &str, contents: &str) {
    let path = project.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn read_project_file(project: &Path, relative: &str) -> String {
    fs::read_to_string(project.join(relative))
        .unwrap()
        .replace("\r\n", "\n")
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

fn file_url(path: &Path) -> String {
    let mut path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    if !path.starts_with('/') {
        path = format!("/{path}");
    }
    format!("file://{path}")
}
