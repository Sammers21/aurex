use config::{Config, JarMode};
use maven::ResolvedArtifact;
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub mod config;
mod jar;
pub mod maven;

pub fn init(root: &str) {
    // Create the dir for main class
    std::fs::create_dir_all(format!("{}/src/com/example", root)).unwrap();
    let jf = std::fs::write(
        format!("{}/src/com/example/Main.java", root),
        "package com.example;

public class Main {
    public static void main(String[] args) {
        System.out.println(\"Hello, world!\");
    }
}",
    );
    if jf.is_err() {
        panic!("Failed to create the Main.java file: {}", jf.unwrap_err());
    }
    let gt = std::fs::write(
        format!("{}/aurex.toml", root),
        "[package]
name = \"hello-world\"
version = \"0.0.1\"

[dependencies]",
    );
    if gt.is_err() {
        panic!("Failed to create the aurex.toml file: {}", gt.unwrap_err());
    }
}

pub fn build(config: Config) -> PathBuf {
    build_project(config).unwrap_or_else(|err| panic!("{err}"))
}

pub fn run(config: Config) {
    let jar_path = build(config);
    let status = Command::new("java").arg("-jar").arg(&jar_path).status();
    if status.is_err() {
        panic!(
            "Failed to execute `{}`: {}",
            jar_path.display(),
            status.unwrap_err()
        );
    }
    let status = status.unwrap();
    if !status.success() {
        panic!(
            "java -jar `{}` failed with status {status}",
            jar_path.display()
        );
    }
}

pub fn java() -> Result<(), String> {
    let info = java_info()?;
    println!("java: {}", info.executable.display());
    print!("{}", info.version_output);
    Ok(())
}

struct JavaInfo {
    executable: PathBuf,
    version_output: String,
}

fn java_info() -> Result<JavaInfo, String> {
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

fn build_project(config: Config) -> Result<PathBuf, String> {
    let artifacts = maven::resolve_dependencies(&config)?;
    compile_sources(&config, &artifacts)?;
    copy_resources(&config)?;

    let jar_path = config.jar_file();
    match config.jar_mode() {
        JarMode::Classpath => jar::create_classpath_jar(
            &jar_path,
            &config.classes_dir(),
            &config.main_class_name(),
            &artifacts,
        )?,
        JarMode::Fat => jar::create_fat_jar(
            &jar_path,
            &config.classes_dir(),
            &config.main_class_name(),
            &artifacts,
        )?,
    }

    Ok(jar_path)
}

fn compile_sources(config: &Config, artifacts: &[ResolvedArtifact]) -> Result<(), String> {
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

    let status = javac
        .status()
        .map_err(|err| format!("Failed to start javac: {err}"))?;
    if !status.success() {
        return Err(format!("javac failed with status {status}"));
    }

    Ok(())
}

fn copy_resources(config: &Config) -> Result<(), String> {
    for resource_dir in config.resource_dirs() {
        if !resource_dir.exists() {
            return Err(format!(
                "Resource directory `{}` does not exist",
                resource_dir.display()
            ));
        }
        copy_resource_tree(&resource_dir, &resource_dir, &config.classes_dir())?;
    }
    Ok(())
}

fn copy_resource_tree(root: &Path, current: &Path, classes_dir: &Path) -> Result<(), String> {
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
            copy_resource_tree(root, &path, classes_dir)?;
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
    }
    Ok(())
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
