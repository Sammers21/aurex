use config::{Config, JarMode};
use maven::{Coordinate, ResolvedArtifact};
use std::{
    collections::HashSet,
    env,
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::Duration,
};

pub mod config;
mod jar;
pub mod manifest;
pub mod maven;

const JUNIT_CONSOLE_GROUP: &str = "org.junit.platform";
const JUNIT_CONSOLE_ARTIFACT: &str = "junit-platform-console-standalone";
const JUNIT_CONSOLE_VERSION: &str = "1.14.0";
const GOOGLE_FORMAT_GROUP: &str = "com.google.googlejavaformat";
const GOOGLE_FORMAT_ARTIFACT: &str = "google-java-format";
const GOOGLE_FORMAT_VERSION: &str = "1.35.0";
const ECLIPSE_JDT_GROUP: &str = "org.eclipse.jdt";
const ECLIPSE_JDT_ARTIFACT: &str = "org.eclipse.jdt.core";
const ECLIPSE_JDT_VERSION: &str = "3.45.0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatTool {
    GoogleJavaFormat,
    EclipseJdt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatOutcome {
    pub file_count: usize,
    pub tool: Option<FormatTool>,
}

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
root = \"./src\"
main = \"com.example.Main\"

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

pub fn clean(root: &str) -> Result<bool, String> {
    let target_dir = PathBuf::from(root).join("target");
    if !target_dir.exists() {
        return Ok(false);
    }
    if !target_dir.is_dir() {
        return Err(format!(
            "`{}` exists but is not a directory",
            target_dir.display()
        ));
    }
    fs::remove_dir_all(&target_dir).map_err(|err| {
        format!(
            "Failed to remove target directory `{}`: {err}",
            target_dir.display()
        )
    })?;
    Ok(true)
}

pub fn test_project(config: Config) -> Result<(), String> {
    let artifacts = maven::resolve_dependencies(&config)?;
    compile_sources(&config, &artifacts)?;

    let junit_artifacts = resolve_tool_artifacts(
        &config,
        Coordinate::new(
            JUNIT_CONSOLE_GROUP.to_string(),
            JUNIT_CONSOLE_ARTIFACT.to_string(),
            JUNIT_CONSOLE_VERSION.to_string(),
        )?,
    )?;
    compile_test_sources(&config, &artifacts, &junit_artifacts)?;
    run_junit_console(&config, &artifacts, &junit_artifacts)
}

pub fn format_project(config: Config) -> Result<FormatOutcome, String> {
    let java_files = collect_format_java_files(&config)?;
    if java_files.is_empty() {
        return Ok(FormatOutcome {
            file_count: 0,
            tool: None,
        });
    }

    let eclipse_config = PathBuf::from(&config.root).join("eclipse-formatter.xml");
    let tool = if eclipse_config.exists() {
        format_with_eclipse_jdt(&config, &java_files, &eclipse_config)?;
        FormatTool::EclipseJdt
    } else {
        format_with_google_java_format(&config, &java_files)?;
        FormatTool::GoogleJavaFormat
    };

    Ok(FormatOutcome {
        file_count: java_files.len(),
        tool: Some(tool),
    })
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
    let main_file = config.main_file();
    if !main_file.exists() {
        return Err(format!(
            "Main source file `{}` does not exist. `[package].main` is `{}` and `[package].root` is `{}`",
            main_file.display(),
            config.main_class_name(),
            src_dir.display()
        ));
    }

    let mut java_files = Vec::new();
    collect_java_files_excluding(&src_dir, &production_excludes(config), &mut java_files)?;
    java_files.sort();

    if java_files.is_empty() {
        return Err(format!(
            "No Java source files found in `{}`",
            src_dir.display()
        ));
    }
    let source_count = java_files.len();

    let mut classpath_entries = vec![src_dir];
    classpath_entries.extend(artifacts.iter().map(|artifact| artifact.jar_path.clone()));
    let output = compile_java_files(&java_files, &classes_dir, &classpath_entries)?;

    Ok(CompileResult {
        source_count,
        output,
    })
}

fn compile_test_sources(
    config: &Config,
    artifacts: &[ResolvedArtifact],
    tool_artifacts: &[ResolvedArtifact],
) -> Result<CompileResult, String> {
    let test_root = config.test_root_dir();
    let test_classes_dir = config.test_classes_dir();
    let mut java_files = Vec::new();
    if test_root.exists() {
        collect_java_files_excluding(&test_root, &[config.target_dir()], &mut java_files)?;
    }
    java_files.sort();

    if test_classes_dir.exists() {
        fs::remove_dir_all(&test_classes_dir).map_err(|err| {
            format!(
                "Failed to clean test class output directory `{}`: {err}",
                test_classes_dir.display()
            )
        })?;
    }
    fs::create_dir_all(&test_classes_dir).map_err(|err| {
        format!(
            "Failed to create test class output directory `{}`: {err}",
            test_classes_dir.display()
        )
    })?;

    if java_files.is_empty() {
        return Ok(CompileResult {
            source_count: 0,
            output: String::new(),
        });
    }

    let mut classpath_entries = vec![config.classes_dir(), test_root, config.src_dir()];
    classpath_entries.extend(artifacts.iter().map(|artifact| artifact.jar_path.clone()));
    classpath_entries.extend(
        tool_artifacts
            .iter()
            .map(|artifact| artifact.jar_path.clone()),
    );
    let output = compile_java_files(&java_files, &test_classes_dir, &classpath_entries)?;

    Ok(CompileResult {
        source_count: java_files.len(),
        output,
    })
}

fn compile_java_files(
    java_files: &[PathBuf],
    classes_dir: &Path,
    classpath_entries: &[PathBuf],
) -> Result<String, String> {
    if classes_dir.exists() {
        fs::remove_dir_all(classes_dir).map_err(|err| {
            format!(
                "Failed to clean class output directory `{}`: {err}",
                classes_dir.display()
            )
        })?;
    }
    fs::create_dir_all(classes_dir).map_err(|err| {
        format!(
            "Failed to create class output directory `{}`: {err}",
            classes_dir.display()
        )
    })?;

    let classpath = join_classpath(classpath_entries)?;

    let mut javac = Command::new("javac");
    javac.arg("-cp").arg(classpath).arg("-d").arg(classes_dir);
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

    Ok(command_output_text(&output))
}

fn run_junit_console(
    config: &Config,
    artifacts: &[ResolvedArtifact],
    tool_artifacts: &[ResolvedArtifact],
) -> Result<(), String> {
    let mut classpath_entries = vec![config.classes_dir(), config.test_classes_dir()];
    classpath_entries.extend(artifacts.iter().map(|artifact| artifact.jar_path.clone()));
    classpath_entries.extend(
        tool_artifacts
            .iter()
            .map(|artifact| artifact.jar_path.clone()),
    );
    let classpath = join_classpath(&classpath_entries)?;

    let status = Command::new("java")
        .arg("-cp")
        .arg(classpath)
        .arg("org.junit.platform.console.ConsoleLauncher")
        .arg("--scan-class-path")
        .status()
        .map_err(|err| format!("Failed to start JUnit ConsoleLauncher: {err}"))?;

    if !status.success() {
        return Err(format!("JUnit ConsoleLauncher failed with status {status}"));
    }

    Ok(())
}

fn collect_format_java_files(config: &Config) -> Result<Vec<PathBuf>, String> {
    let mut java_files = Vec::new();
    let mut seen = HashSet::new();
    let target_dir = config.target_dir();

    for root in [config.src_dir(), config.test_root_dir()] {
        if !root.exists() {
            if root == config.src_dir() {
                return Err(format!(
                    "Source directory `{}` does not exist",
                    root.display()
                ));
            }
            continue;
        }
        collect_format_java_files_in(&root, &target_dir, &mut seen, &mut java_files)?;
    }

    java_files.sort();
    Ok(java_files)
}

fn collect_format_java_files_in(
    dir: &Path,
    target_dir: &Path,
    seen: &mut HashSet<PathBuf>,
    java_files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if is_same_or_child(dir, target_dir) {
        return Ok(());
    }

    for entry in
        fs::read_dir(dir).map_err(|err| format!("Failed to read `{}`: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("Failed to read source directory entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_format_java_files_in(&path, target_dir, seen, java_files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "java")
        {
            let normalized = normalize_path(&path);
            if seen.insert(normalized) {
                java_files.push(path);
            }
        }
    }

    Ok(())
}

fn format_with_google_java_format(config: &Config, java_files: &[PathBuf]) -> Result<(), String> {
    let artifacts = resolve_tool_artifacts(
        config,
        Coordinate::new(
            GOOGLE_FORMAT_GROUP.to_string(),
            GOOGLE_FORMAT_ARTIFACT.to_string(),
            GOOGLE_FORMAT_VERSION.to_string(),
        )?,
    )?;
    let classpath_entries = artifact_paths(&artifacts);
    let classpath = join_classpath(&classpath_entries)?;

    let mut java = Command::new("java");
    for export in google_java_format_module_exports() {
        java.arg(export);
    }
    java.arg("-cp")
        .arg(classpath)
        .arg("com.google.googlejavaformat.java.Main")
        .arg("--replace");
    for java_file in java_files {
        java.arg(java_file);
    }

    let output = java
        .output()
        .map_err(|err| format!("Failed to start google-java-format: {err}"))?;
    if !output.status.success() {
        let mut message = format!("google-java-format failed with status {}", output.status);
        append_command_output(&mut message, &output);
        return Err(message);
    }

    Ok(())
}

fn format_with_eclipse_jdt(
    config: &Config,
    java_files: &[PathBuf],
    eclipse_config: &Path,
) -> Result<(), String> {
    let artifacts = resolve_tool_artifacts(
        config,
        Coordinate::new(
            ECLIPSE_JDT_GROUP.to_string(),
            ECLIPSE_JDT_ARTIFACT.to_string(),
            ECLIPSE_JDT_VERSION.to_string(),
        )?,
    )?;
    let options = parse_eclipse_formatter_options(eclipse_config)?;
    let tools_dir = config.tools_dir();
    fs::create_dir_all(&tools_dir).map_err(|err| {
        format!(
            "Failed to create formatter tool directory `{}`: {err}",
            tools_dir.display()
        )
    })?;

    let options_path = tools_dir.join("eclipse-formatter-options.properties");
    write_eclipse_options_properties(&options_path, &options)?;
    let runner_classes = compile_eclipse_formatter_runner(&tools_dir, &artifacts)?;

    let mut classpath_entries = vec![runner_classes];
    classpath_entries.extend(artifact_paths(&artifacts));
    let classpath = join_classpath(&classpath_entries)?;

    let mut java = Command::new("java");
    java.arg("-cp")
        .arg(classpath)
        .arg("dev.aurex.tools.EclipseFormatterRunner")
        .arg(&options_path);
    for java_file in java_files {
        java.arg(java_file);
    }

    let output = java
        .output()
        .map_err(|err| format!("Failed to start Eclipse JDT formatter: {err}"))?;
    if !output.status.success() {
        let mut message = format!("Eclipse JDT formatter failed with status {}", output.status);
        append_command_output(&mut message, &output);
        return Err(message);
    }

    Ok(())
}

fn resolve_tool_artifacts(
    config: &Config,
    coordinate: Coordinate,
) -> Result<Vec<ResolvedArtifact>, String> {
    let repositories = maven::repositories_from_config(config.repositories())?;
    let mut resolver = maven::MavenResolver::new(repositories, config.tools_dir())?;
    resolver.resolve_roots(&[coordinate])
}

fn artifact_paths(artifacts: &[ResolvedArtifact]) -> Vec<PathBuf> {
    artifacts
        .iter()
        .map(|artifact| artifact.jar_path.clone())
        .collect()
}

fn google_java_format_module_exports() -> &'static [&'static str] {
    &[
        "--add-exports=jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED",
        "--add-exports=jdk.compiler/com.sun.tools.javac.code=ALL-UNNAMED",
        "--add-exports=jdk.compiler/com.sun.tools.javac.file=ALL-UNNAMED",
        "--add-exports=jdk.compiler/com.sun.tools.javac.parser=ALL-UNNAMED",
        "--add-exports=jdk.compiler/com.sun.tools.javac.tree=ALL-UNNAMED",
        "--add-exports=jdk.compiler/com.sun.tools.javac.util=ALL-UNNAMED",
    ]
}

fn parse_eclipse_formatter_options(path: &Path) -> Result<Vec<(String, String)>, String> {
    let xml = fs::read_to_string(path)
        .map_err(|err| format!("Failed to read `{}`: {err}", path.display()))?;
    let document = roxmltree::Document::parse(&xml)
        .map_err(|err| format!("Failed to parse `{}`: {err}", path.display()))?;
    let mut options = Vec::new();
    for node in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "setting")
    {
        let Some(id) = node.attribute("id") else {
            continue;
        };
        let Some(value) = node.attribute("value") else {
            continue;
        };
        options.push((id.to_string(), value.to_string()));
    }
    options.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(options)
}

fn write_eclipse_options_properties(
    path: &Path,
    options: &[(String, String)],
) -> Result<(), String> {
    let mut file = fs::File::create(path)
        .map_err(|err| format!("Failed to create `{}`: {err}", path.display()))?;
    for (key, value) in options {
        writeln!(
            file,
            "{}={}",
            java_properties_escape(key),
            java_properties_escape(value)
        )
        .map_err(|err| format!("Failed to write `{}`: {err}", path.display()))?;
    }
    Ok(())
}

fn java_properties_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

fn compile_eclipse_formatter_runner(
    tools_dir: &Path,
    artifacts: &[ResolvedArtifact],
) -> Result<PathBuf, String> {
    let source_path = tools_dir.join("EclipseFormatterRunner.java");
    let classes_dir = tools_dir.join("eclipse-formatter-runner");
    fs::write(&source_path, ECLIPSE_FORMATTER_RUNNER).map_err(|err| {
        format!(
            "Failed to write Eclipse formatter runner `{}`: {err}",
            source_path.display()
        )
    })?;
    if classes_dir.exists() {
        fs::remove_dir_all(&classes_dir).map_err(|err| {
            format!(
                "Failed to clean Eclipse formatter runner output `{}`: {err}",
                classes_dir.display()
            )
        })?;
    }
    fs::create_dir_all(&classes_dir).map_err(|err| {
        format!(
            "Failed to create Eclipse formatter runner output `{}`: {err}",
            classes_dir.display()
        )
    })?;

    let classpath_entries = artifact_paths(artifacts);
    let classpath = join_classpath(&classpath_entries)?;
    let output = Command::new("javac")
        .arg("-cp")
        .arg(classpath)
        .arg("-d")
        .arg(&classes_dir)
        .arg(&source_path)
        .output()
        .map_err(|err| format!("Failed to start javac for Eclipse formatter runner: {err}"))?;
    if !output.status.success() {
        let mut message = format!(
            "javac failed to compile Eclipse formatter runner with status {}",
            output.status
        );
        append_command_output(&mut message, &output);
        return Err(message);
    }

    Ok(classes_dir)
}

const ECLIPSE_FORMATTER_RUNNER: &str = r#"package dev.aurex.tools;

import java.nio.charset.StandardCharsets;
import java.io.BufferedReader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.HashMap;
import java.util.Map;
import java.util.Properties;
import org.eclipse.jdt.core.ToolFactory;
import org.eclipse.jdt.core.formatter.CodeFormatter;
import org.eclipse.jface.text.Document;
import org.eclipse.text.edits.TextEdit;

public final class EclipseFormatterRunner {
    private EclipseFormatterRunner() {
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 2) {
            throw new IllegalArgumentException("Expected options properties path and at least one Java file");
        }

        Properties properties = new Properties();
        try (BufferedReader reader = Files.newBufferedReader(Paths.get(args[0]), StandardCharsets.UTF_8)) {
            properties.load(reader);
        }
        Map<String, String> options = new HashMap<>();
        for (String name : properties.stringPropertyNames()) {
            options.put(name, properties.getProperty(name));
        }

        CodeFormatter formatter = ToolFactory.createCodeFormatter(options);
        for (int index = 1; index < args.length; index++) {
            Path file = Paths.get(args[index]);
            String source = new String(Files.readAllBytes(file), StandardCharsets.UTF_8);
            TextEdit edit = formatter.format(
                    CodeFormatter.K_COMPILATION_UNIT,
                    source,
                    0,
                    source.length(),
                    0,
                    System.lineSeparator()
            );
            if (edit == null) {
                throw new IllegalStateException("Eclipse JDT could not format " + file);
            }
            Document document = new Document(source);
            edit.apply(document);
            Files.write(file, document.get().getBytes(StandardCharsets.UTF_8));
        }
    }
}
"#;

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

fn production_excludes(config: &Config) -> Vec<PathBuf> {
    let mut excludes = Vec::new();
    let src_dir = config.src_dir();
    let test_root = config.test_root_dir();
    if is_same_or_child(&test_root, &src_dir) {
        excludes.push(test_root);
    }
    let target_dir = config.target_dir();
    if is_same_or_child(&target_dir, &src_dir) {
        excludes.push(target_dir);
    }
    excludes
}

fn collect_java_files_excluding(
    dir: &Path,
    excludes: &[PathBuf],
    java_files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if excludes
        .iter()
        .any(|exclude| is_same_or_child(dir, exclude))
    {
        return Ok(());
    }
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
            collect_java_files_excluding(&path, excludes, java_files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "java")
        {
            java_files.push(path);
        }
    }

    Ok(())
}

fn is_same_or_child(path: &Path, parent: &Path) -> bool {
    let path = normalize_path(path);
    let parent = normalize_path(parent);
    path == parent || path.starts_with(parent)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn join_classpath(entries: &[PathBuf]) -> Result<OsString, String> {
    env::join_paths(entries.iter()).map_err(|err| format!("Failed to construct classpath: {err}"))
}
