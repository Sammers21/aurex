use config::{Config, JarMode};
use maven::ResolvedArtifact;
use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::Duration,
};

pub mod config;
mod jar;
pub mod maven;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildStage {
    Resolve,
    Compile,
    Resources,
    Package,
}

impl BuildStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resolve => "resolve",
            Self::Compile => "compile",
            Self::Resources => "resources",
            Self::Package => "package",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildEventDetail {
    None,
    Artifacts(usize),
    Sources(usize),
    Resources(usize),
    Artifact(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildEvent {
    Started(BuildStage),
    Finished(BuildStage, BuildEventDetail),
    Output { stage: BuildStage, text: String },
    Done { jar_path: PathBuf },
}

pub trait BuildReporter {
    fn report(&mut self, event: BuildEvent);

    fn tick(&mut self) {}
}

pub struct NoopBuildReporter;

impl BuildReporter for NoopBuildReporter {
    fn report(&mut self, _event: BuildEvent) {}
}

pub fn init(root: &str) {
    try_init(root).unwrap_or_else(|err| panic!("{err}"))
}

pub fn try_init(root: &str) -> Result<(), String> {
    // Create the dir for main class
    fs::create_dir_all(format!("{}/src/com/example", root))
        .map_err(|err| format!("Failed to create source directory: {err}"))?;
    fs::write(
        format!("{}/src/com/example/Main.java", root),
        "package com.example;

public class Main {
    public static void main(String[] args) {
        System.out.println(\"Hello, world!\");
    }
}",
    )
    .map_err(|err| format!("Failed to create the Main.java file: {err}"))?;
    fs::write(
        format!("{}/ax.toml", root),
        "[package]
name = \"hello-world\"
version = \"0.0.1\"

[dependencies]",
    )
    .map_err(|err| format!("Failed to create the ax.toml file: {err}"))?;
    Ok(())
}

pub fn build(config: Config) -> PathBuf {
    let mut reporter = NoopBuildReporter;
    build_with_reporter(config, &mut reporter).unwrap_or_else(|err| panic!("{err}"))
}

pub fn build_with_reporter(
    config: Config,
    reporter: &mut dyn BuildReporter,
) -> Result<PathBuf, String> {
    build_project(config, reporter)
}

pub fn run(config: Config) {
    run_with_args(config, std::iter::empty::<OsString>())
}

pub fn run_with_args<I, S>(config: Config, app_args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut reporter = NoopBuildReporter;
    run_with_reporter_args(config, app_args, &mut reporter).unwrap_or_else(|err| panic!("{err}"))
}

pub fn run_with_reporter(config: Config, reporter: &mut dyn BuildReporter) -> Result<(), String> {
    run_with_reporter_args(config, std::iter::empty::<OsString>(), reporter)
}

pub fn run_with_reporter_args<I, S>(
    config: Config,
    app_args: I,
    reporter: &mut dyn BuildReporter,
) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let jar_path = build_with_reporter(config, reporter)?;
    let status = Command::new("java")
        .arg("-jar")
        .arg(&jar_path)
        .args(app_args)
        .status();
    if status.is_err() {
        return Err(format!(
            "Failed to execute `{}`: {}",
            jar_path.display(),
            status.unwrap_err()
        ));
    }
    let status = status.unwrap();
    if !status.success() {
        return Err(format!(
            "java -jar `{}` failed with status {status}",
            jar_path.display()
        ));
    }
    Ok(())
}

pub fn java() -> Result<(), String> {
    let info = java_info()?;
    println!("java: {}", info.executable.display());
    print!("{}", info.version_output);
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaInfo {
    pub executable: PathBuf,
    pub version_output: String,
}

pub fn java_info() -> Result<JavaInfo, String> {
    let output = Command::new("java")
        .arg("-version")
        .output()
        .map_err(|err| format!("Failed to start java: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut message = format!("java -version failed with status {}", output.status);
        if !stderr.trim().is_empty() {
            message.push_str(": ");
            message.push_str(stderr.trim());
        }
        return Err(message);
    }

    let mut version_output = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !version_output.is_empty() && !version_output.ends_with('\n') && !stderr.is_empty() {
        version_output.push('\n');
    }
    version_output.push_str(&stderr);
    if !version_output.ends_with('\n') {
        version_output.push('\n');
    }

    Ok(JavaInfo {
        executable: resolve_command("java").unwrap_or_else(|| PathBuf::from("java")),
        version_output,
    })
}

fn resolve_command(command: &str) -> Option<PathBuf> {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 {
        return existing_file(command_path.to_path_buf());
    }

    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        for candidate in command_candidates(&directory, command) {
            if let Some(existing) = existing_file(candidate) {
                return Some(existing);
            }
        }
    }

    None
}

fn existing_file(path: PathBuf) -> Option<PathBuf> {
    if !path.is_file() {
        return None;
    }
    fs::canonicalize(&path).ok().or(Some(path))
}

fn command_candidates(directory: &Path, command: &str) -> Vec<PathBuf> {
    let mut candidates = vec![directory.join(command)];
    if Path::new(command).extension().is_some() {
        return candidates;
    }

    for extension in executable_extensions() {
        let mut file_name = OsString::from(command);
        file_name.push(extension);
        candidates.push(directory.join(file_name));
    }

    candidates
}

#[cfg(windows)]
fn executable_extensions() -> Vec<OsString> {
    let pathext = env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
    pathext
        .to_string_lossy()
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(OsString::from)
        .collect()
}

#[cfg(not(windows))]
fn executable_extensions() -> Vec<OsString> {
    Vec::new()
}

fn build_project(config: Config, reporter: &mut dyn BuildReporter) -> Result<PathBuf, String> {
    reporter.report(BuildEvent::Started(BuildStage::Resolve));
    let resolve_config = config.clone();
    let artifacts = run_with_heartbeat(reporter, move || {
        maven::resolve_dependencies(&resolve_config)
    })?;
    reporter.report(BuildEvent::Finished(
        BuildStage::Resolve,
        BuildEventDetail::Artifacts(artifacts.len()),
    ));

    reporter.report(BuildEvent::Started(BuildStage::Compile));
    let compile_config = config.clone();
    let compile_artifacts = artifacts.clone();
    let compile_result = run_with_heartbeat(reporter, move || {
        compile_sources(&compile_config, &compile_artifacts)
    })?;
    if !compile_result.output.trim().is_empty() {
        reporter.report(BuildEvent::Output {
            stage: BuildStage::Compile,
            text: compile_result.output,
        });
    }
    reporter.report(BuildEvent::Finished(
        BuildStage::Compile,
        BuildEventDetail::Sources(compile_result.source_count),
    ));

    reporter.report(BuildEvent::Started(BuildStage::Resources));
    let resource_config = config.clone();
    let resource_count = run_with_heartbeat(reporter, move || copy_resources(&resource_config))?;
    reporter.report(BuildEvent::Finished(
        BuildStage::Resources,
        BuildEventDetail::Resources(resource_count),
    ));

    reporter.report(BuildEvent::Started(BuildStage::Package));
    let package_config = config.clone();
    let package_artifacts = artifacts.clone();
    let jar_path = run_with_heartbeat(reporter, move || {
        let jar_path = package_config.jar_file();
        match package_config.jar_mode() {
            JarMode::Classpath => jar::create_classpath_jar(
                &jar_path,
                &package_config.classes_dir(),
                &package_config.main_class_name(),
                &package_artifacts,
            )?,
            JarMode::Fat => jar::create_fat_jar(
                &jar_path,
                &package_config.classes_dir(),
                &package_config.main_class_name(),
                &package_artifacts,
            )?,
        }
        Ok(jar_path)
    })?;
    reporter.report(BuildEvent::Finished(
        BuildStage::Package,
        BuildEventDetail::Artifact(jar_path.clone()),
    ));
    reporter.report(BuildEvent::Done {
        jar_path: jar_path.clone(),
    });

    Ok(jar_path)
}

fn run_with_heartbeat<T, F>(reporter: &mut dyn BuildReporter, operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(operation());
    });

    loop {
        match receiver.recv_timeout(Duration::from_millis(80)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => reporter.tick(),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("build task stopped before reporting a result".to_string());
            }
        }
    }
}

struct CompileResult {
    source_count: usize,
    output: String,
}

fn compile_sources(
    config: &Config,
    artifacts: &[ResolvedArtifact],
) -> Result<CompileResult, String> {
    let src_dir = config.src_dir();
    let classes_dir = config.classes_dir();
    let mut java_files = Vec::new();
    collect_java_files(&src_dir, &mut java_files)?;
    java_files.sort();

    if java_files.is_empty() {
        return Err(format!(
            "No Java source files found in `{}`",
            src_dir.display()
        ));
    }
    let source_count = java_files.len();

    if classes_dir.exists() {
        fs::remove_dir_all(&classes_dir).map_err(|err| {
            format!(
                "Failed to clean class output directory `{}`: {err}",
                classes_dir.display()
            )
        })?;
    }
    fs::create_dir_all(&classes_dir).map_err(|err| {
        format!(
            "Failed to create class output directory `{}`: {err}",
            classes_dir.display()
        )
    })?;

    let mut classpath_entries = vec![src_dir];
    classpath_entries.extend(artifacts.iter().map(|artifact| artifact.jar_path.clone()));
    let classpath = std::env::join_paths(classpath_entries.iter())
        .map_err(|err| format!("Failed to construct javac classpath: {err}"))?;

    let mut javac = Command::new("javac");
    javac.arg("-cp").arg(classpath).arg("-d").arg(&classes_dir);
    for java_file in java_files {
        javac.arg(java_file);
    }

    let output = javac
        .output()
        .map_err(|err| format!("Failed to start javac: {err}"))?;
    if !output.status.success() {
        let mut message = format!("javac failed with status {}", output.status);
        append_command_output(&mut message, &output);
        return Err(message);
    }

    Ok(CompileResult {
        source_count,
        output: command_output_text(&output),
    })
}

fn append_command_output(message: &mut String, output: &std::process::Output) {
    let output_text = command_output_text(output);
    if !output_text.trim().is_empty() {
        message.push_str(":\n");
        message.push_str(output_text.trim_end());
    }
}

fn command_output_text(output: &std::process::Output) -> String {
    let mut text = String::new();
    append_output_stream(&mut text, &output.stdout);
    append_output_stream(&mut text, &output.stderr);
    text
}

fn append_output_stream(text: &mut String, stream: &[u8]) {
    if stream.is_empty() {
        return;
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&String::from_utf8_lossy(stream));
}

fn copy_resources(config: &Config) -> Result<usize, String> {
    let mut copied = 0;
    for resource_dir in config.resource_dirs() {
        if !resource_dir.exists() {
            return Err(format!(
                "Resource directory `{}` does not exist",
                resource_dir.display()
            ));
        }
        copied += copy_resource_tree(&resource_dir, &resource_dir, &config.classes_dir())?;
    }
    Ok(copied)
}

fn copy_resource_tree(root: &Path, current: &Path, classes_dir: &Path) -> Result<usize, String> {
    let mut copied = 0;
    for entry in fs::read_dir(current).map_err(|err| {
        format!(
            "Failed to read resource directory `{}`: {err}",
            current.display()
        )
    })? {
        let entry =
            entry.map_err(|err| format!("Failed to read resource directory entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            copied += copy_resource_tree(root, &path, classes_dir)?;
            continue;
        }
        if path
            .extension()
            .is_some_and(|extension| extension == "java")
        {
            continue;
        }

        let relative = path.strip_prefix(root).map_err(|err| {
            format!(
                "Failed to calculate resource path for `{}` relative to `{}`: {err}",
                path.display(),
                root.display()
            )
        })?;
        let output = classes_dir.join(relative);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "Failed to create resource output directory `{}`: {err}",
                    parent.display()
                )
            })?;
        }
        fs::copy(&path, &output).map_err(|err| {
            format!(
                "Failed to copy resource `{}` to `{}`: {err}",
                path.display(),
                output.display()
            )
        })?;
        copied += 1;
    }
    Ok(copied)
}

fn collect_java_files(dir: &Path, java_files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.exists() {
        return Err(format!(
            "Source directory `{}` does not exist",
            dir.display()
        ));
    }

    for entry in
        fs::read_dir(dir).map_err(|err| format!("Failed to read `{}`: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("Failed to read source directory entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_java_files(&path, java_files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "java")
        {
            java_files.push(path);
        }
    }

    Ok(())
}
