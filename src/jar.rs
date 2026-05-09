use crate::maven::ResolvedArtifact;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::FileOptions};

pub fn create_classpath_jar(
    jar_path: &Path,
    classes_dir: &Path,
    main_class: &str,
    artifacts: &[ResolvedArtifact],
) -> Result<(), String> {
    let class_path = artifacts
        .iter()
        .map(|artifact| manifest_relative_path(jar_path, &artifact.jar_path))
        .collect::<Vec<_>>();
    let manifest = build_manifest(main_class, &class_path);
    let mut writer = create_jar_writer(jar_path)?;
    let mut seen = HashSet::new();

    add_manifest(&mut writer, &manifest, &mut seen)?;
    add_class_tree(&mut writer, classes_dir, classes_dir, &mut seen, None)?;
    writer
        .finish()
        .map_err(|err| format!("Failed to finish jar `{}`: {err}", jar_path.display()))?;

    Ok(())
}

pub fn create_fat_jar(
    jar_path: &Path,
    classes_dir: &Path,
    main_class: &str,
    artifacts: &[ResolvedArtifact],
) -> Result<(), String> {
    let manifest = build_manifest(main_class, &[]);
    let mut writer = create_jar_writer(jar_path)?;
    let mut seen = HashSet::new();
    let mut service_files = HashMap::new();

    add_manifest(&mut writer, &manifest, &mut seen)?;
    add_class_tree(
        &mut writer,
        classes_dir,
        classes_dir,
        &mut seen,
        Some(&mut service_files),
    )?;

    for artifact in artifacts {
        merge_dependency_jar(
            &mut writer,
            &artifact.jar_path,
            &mut seen,
            &mut service_files,
        )?;
    }

    for (name, chunks) in service_files {
        if seen.insert(name.clone()) {
            writer
                .start_file(name, file_options())
                .map_err(|err| format!("Failed to add service file to fat jar: {err}"))?;
            for chunk in chunks {
                writer
                    .write_all(&chunk)
                    .map_err(|err| format!("Failed to write service file to fat jar: {err}"))?;
                if !chunk.ends_with(b"\n") {
                    writer.write_all(b"\n").map_err(|err| {
                        format!("Failed to write service file newline to fat jar: {err}")
                    })?;
                }
            }
        }
    }

    writer
        .finish()
        .map_err(|err| format!("Failed to finish jar `{}`: {err}", jar_path.display()))?;

    Ok(())
}

fn create_jar_writer(jar_path: &Path) -> Result<ZipWriter<File>, String> {
    if let Some(parent) = jar_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "Failed to create jar output directory `{}`: {err}",
                    parent.display()
                )
            })?;
        }
    }

    let file = File::create(jar_path)
        .map_err(|err| format!("Failed to create jar `{}`: {err}", jar_path.display()))?;
    Ok(ZipWriter::new(file))
}

fn add_manifest(
    writer: &mut ZipWriter<File>,
    manifest: &str,
    seen: &mut HashSet<String>,
) -> Result<(), String> {
    seen.insert("META-INF/MANIFEST.MF".to_string());
    writer
        .start_file("META-INF/MANIFEST.MF", file_options())
        .map_err(|err| format!("Failed to add manifest to jar: {err}"))?;
    writer
        .write_all(manifest.as_bytes())
        .map_err(|err| format!("Failed to write manifest to jar: {err}"))
}

fn add_class_tree(
    writer: &mut ZipWriter<File>,
    root: &Path,
    current: &Path,
    seen: &mut HashSet<String>,
    mut service_files: Option<&mut HashMap<String, Vec<Vec<u8>>>>,
) -> Result<(), String> {
    if !current.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(current)
        .map_err(|err| format!("Failed to read `{}`: {err}", current.display()))?
    {
        let entry = entry.map_err(|err| format!("Failed to read class directory entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            add_class_tree(writer, root, &path, seen, service_files.as_deref_mut())?;
            continue;
        }

        let name = zip_name(root, &path)?;
        if let Some(service_files) = service_files.as_deref_mut() {
            if is_service_file(&name) {
                service_files
                    .entry(name)
                    .or_default()
                    .push(fs::read(&path).map_err(|err| {
                        format!("Failed to read service file `{}`: {err}", path.display())
                    })?);
                continue;
            }
        }

        if seen.insert(name.clone()) {
            writer
                .start_file(name, file_options())
                .map_err(|err| format!("Failed to add class/resource to jar: {err}"))?;
            let mut file = File::open(&path)
                .map_err(|err| format!("Failed to open `{}`: {err}", path.display()))?;
            std::io::copy(&mut file, writer)
                .map_err(|err| format!("Failed to copy `{}` into jar: {err}", path.display()))?;
        }
    }

    Ok(())
}

fn merge_dependency_jar(
    writer: &mut ZipWriter<File>,
    jar_path: &Path,
    seen: &mut HashSet<String>,
    service_files: &mut HashMap<String, Vec<Vec<u8>>>,
) -> Result<(), String> {
    let file = File::open(jar_path).map_err(|err| {
        format!(
            "Failed to open dependency jar `{}`: {err}",
            jar_path.display()
        )
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| {
        format!(
            "Failed to read dependency jar `{}`: {err}",
            jar_path.display()
        )
    })?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| {
            format!(
                "Failed to read entry #{index} from dependency jar `{}`: {err}",
                jar_path.display()
            )
        })?;
        if entry.is_dir() {
            continue;
        }

        let name = entry.name().replace('\\', "/");
        if should_skip_dependency_entry(&name) {
            continue;
        }

        if is_service_file(&name) {
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents).map_err(|err| {
                format!(
                    "Failed to read service file `{name}` from `{}`: {err}",
                    jar_path.display()
                )
            })?;
            service_files.entry(name).or_default().push(contents);
            continue;
        }

        if seen.insert(name.clone()) {
            writer
                .start_file(name.clone(), file_options())
                .map_err(|err| format!("Failed to add `{name}` to fat jar: {err}"))?;
            std::io::copy(&mut entry, writer).map_err(|err| {
                format!(
                    "Failed to copy `{name}` from `{}` into fat jar: {err}",
                    jar_path.display()
                )
            })?;
        }
    }

    Ok(())
}

fn should_skip_dependency_entry(name: &str) -> bool {
    if name == "META-INF/MANIFEST.MF" || name == "META-INF/INDEX.LIST" {
        return true;
    }

    let upper = name.to_ascii_uppercase();
    upper.starts_with("META-INF/")
        && (upper.ends_with(".SF")
            || upper.ends_with(".DSA")
            || upper.ends_with(".RSA")
            || upper.ends_with(".EC"))
}

fn is_service_file(name: &str) -> bool {
    name.starts_with("META-INF/services/") && name.len() > "META-INF/services/".len()
}

fn zip_name(root: &Path, file: &Path) -> Result<String, String> {
    let relative = file.strip_prefix(root).map_err(|err| {
        format!(
            "Failed to calculate jar entry name for `{}` relative to `{}`: {err}",
            file.display(),
            root.display()
        )
    })?;
    Ok(path_to_slash_string(relative))
}

fn manifest_relative_path(jar_path: &Path, dependency_path: &Path) -> String {
    let jar_dir = jar_path.parent().unwrap_or_else(|| Path::new("."));
    let path = dependency_path
        .strip_prefix(jar_dir)
        .unwrap_or(dependency_path);
    path_to_slash_string(path)
}

fn path_to_slash_string(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn build_manifest(main_class: &str, class_path: &[String]) -> String {
    let mut manifest = String::new();
    manifest.push_str(&manifest_line("Manifest-Version", "1.0"));
    manifest.push_str(&manifest_line("Main-Class", main_class));
    if !class_path.is_empty() {
        manifest.push_str(&manifest_line("Class-Path", &class_path.join(" ")));
    }
    manifest.push('\n');
    manifest
}

fn manifest_line(name: &str, value: &str) -> String {
    wrap_manifest_line(&format!("{name}: {value}"))
}

fn wrap_manifest_line(line: &str) -> String {
    const MAX_LINE_LENGTH: usize = 72;
    const MAX_CONTINUATION_PAYLOAD: usize = MAX_LINE_LENGTH - 1;

    if line.len() <= MAX_LINE_LENGTH {
        return format!("{line}\n");
    }

    let bytes = line.as_bytes();
    let mut output = String::new();
    let mut offset = 0;
    let mut first_line = true;

    while offset < bytes.len() {
        if first_line {
            let end = (offset + MAX_LINE_LENGTH).min(bytes.len());
            output.push_str(std::str::from_utf8(&bytes[offset..end]).unwrap());
            output.push('\n');
            offset = end;
            first_line = false;
        } else {
            let end = (offset + MAX_CONTINUATION_PAYLOAD).min(bytes.len());
            output.push(' ');
            output.push_str(std::str::from_utf8(&bytes[offset..end]).unwrap());
            output.push('\n');
            offset = end;
        }
    }

    output
}

fn file_options() -> FileOptions {
    FileOptions::default().compression_method(CompressionMethod::Deflated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maven::{Coordinate, ResolvedArtifact};
    use std::{
        path::PathBuf,
        process,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn wraps_long_manifest_lines() {
        let line = manifest_line(
            "Class-Path",
            "target/deps/a-1.0.0.jar target/deps/b-1.0.0.jar target/deps/c-1.0.0.jar",
        );

        assert!(line.lines().all(|line| line.len() <= 72));
        assert!(line.lines().skip(1).all(|line| line.starts_with(' ')));
    }

    #[test]
    fn skips_signature_files_in_fat_jars() {
        assert!(should_skip_dependency_entry("META-INF/TEST.SF"));
        assert!(should_skip_dependency_entry("META-INF/TEST.RSA"));
        assert!(should_skip_dependency_entry("META-INF/MANIFEST.MF"));
        assert!(!should_skip_dependency_entry("com/example/Main.class"));
    }

    #[test]
    fn renders_manifest_paths_with_slashes() {
        let path = PathBuf::from("target").join("deps").join("demo-1.0.jar");

        assert_eq!(path_to_slash_string(&path), "target/deps/demo-1.0.jar");
    }

    #[test]
    fn creates_classpath_jar_with_manifest_and_project_classes() {
        let root = test_dir("classpath");
        let classes_dir = root.join("classes");
        let jar_path = root.join("app.jar");
        write_file(&classes_dir.join("com/example/Main.class"), b"app-main");

        let artifacts = vec![artifact(
            "org.example",
            "demo",
            "1.0.0",
            root.join("target/deps/demo-1.0.0.jar"),
        )];

        create_classpath_jar(&jar_path, &classes_dir, "com.example.Main", &artifacts).unwrap();

        let manifest = read_zip_text(&jar_path, "META-INF/MANIFEST.MF");
        assert!(manifest.contains("Manifest-Version: 1.0\n"));
        assert!(manifest.contains("Main-Class: com.example.Main\n"));
        assert!(manifest.contains("Class-Path: target/deps/demo-1.0.0.jar\n"));
        assert_eq!(
            read_zip_bytes(&jar_path, "com/example/Main.class"),
            b"app-main"
        );
    }

    #[test]
    fn creates_fat_jar_with_dependency_classes_and_without_classpath() {
        let root = test_dir("fat");
        let classes_dir = root.join("classes");
        let jar_path = root.join("app.jar");
        let dependency_jar = root.join("deps/demo-1.0.0.jar");
        write_file(&classes_dir.join("com/example/Main.class"), b"app-main");
        write_zip(
            &dependency_jar,
            &[
                ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n"),
                ("META-INF/DEMO.SF", b"signature"),
                ("com/example/Main.class", b"dependency-main"),
                ("com/acme/Message.class", b"message"),
            ],
        );

        let artifacts = vec![artifact("org.example", "demo", "1.0.0", dependency_jar)];

        create_fat_jar(&jar_path, &classes_dir, "com.example.Main", &artifacts).unwrap();

        let manifest = read_zip_text(&jar_path, "META-INF/MANIFEST.MF");
        assert!(manifest.contains("Main-Class: com.example.Main\n"));
        assert!(!manifest.contains("Class-Path:"));

        let entries = zip_entries(&jar_path);
        assert!(
            entries
                .iter()
                .any(|entry| entry == "com/acme/Message.class")
        );
        assert!(!entries.iter().any(|entry| entry == "META-INF/DEMO.SF"));
        assert_eq!(
            read_zip_bytes(&jar_path, "com/example/Main.class"),
            b"app-main"
        );
    }

    #[test]
    fn merges_service_files_in_fat_jars() {
        let root = test_dir("services");
        let classes_dir = root.join("classes");
        let jar_path = root.join("app.jar");
        let dependency_jar = root.join("deps/demo-1.0.0.jar");
        write_file(
            &classes_dir.join("META-INF/services/com.example.Plugin"),
            b"com.example.AppPlugin\n",
        );
        write_zip(
            &dependency_jar,
            &[(
                "META-INF/services/com.example.Plugin",
                b"com.example.DependencyPlugin",
            )],
        );

        let artifacts = vec![artifact("org.example", "demo", "1.0.0", dependency_jar)];

        create_fat_jar(&jar_path, &classes_dir, "com.example.Main", &artifacts).unwrap();

        assert_eq!(
            read_zip_text(&jar_path, "META-INF/services/com.example.Plugin"),
            "com.example.AppPlugin\ncom.example.DependencyPlugin\n"
        );
    }

    fn artifact(
        group_id: &str,
        artifact_id: &str,
        version: &str,
        jar_path: PathBuf,
    ) -> ResolvedArtifact {
        ResolvedArtifact {
            coordinate: Coordinate::new(
                group_id.to_string(),
                artifact_id.to_string(),
                version.to_string(),
            )
            .unwrap(),
            jar_path,
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = PathBuf::from("target").join("jar-tests").join(format!(
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

    fn write_file(path: &Path, contents: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        for (name, contents) in entries {
            writer.start_file(*name, file_options()).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }

    fn read_zip_text(path: &Path, name: &str) -> String {
        String::from_utf8(read_zip_bytes(path, name))
            .unwrap()
            .replace("\r\n", "\n")
    }

    fn read_zip_bytes(path: &Path, name: &str) -> Vec<u8> {
        let file = File::open(path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let mut entry = archive.by_name(name).unwrap();
        let mut contents = Vec::new();
        entry.read_to_end(&mut contents).unwrap();
        contents
    }

    fn zip_entries(path: &Path) -> Vec<String> {
        let file = File::open(path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_string())
            .collect()
    }
}
