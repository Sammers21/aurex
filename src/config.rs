use std::{collections::HashMap, fs, path::PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Config {
    pub root: String,
    pub config_toml: ConfigToml,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConfigToml {
    pub package: Package,
    pub dependencies: Option<HashMap<String, String>>,
    pub build: Option<Build>,
    pub repositories: Option<Vec<Repository>>,
    pub resources: Option<Resources>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub main: Option<String>,
    pub root: Option<String>,
    pub test_root: Option<String>,
    pub version: Option<String>,
    pub jar_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Build {
    pub jar_mode: Option<JarMode>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Resources {
    pub dirs: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JarMode {
    Classpath,
    Fat,
}

impl Default for JarMode {
    fn default() -> Self {
        Self::Classpath
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Repository {
    pub name: Option<String>,
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub fn read_toml(root: &str) -> Config {
    try_read_toml(root).unwrap_or_else(|err| panic!("{err}"))
}

pub fn try_read_toml(root: &str) -> Result<Config, String> {
    let path = PathBuf::from(root).join("ax.toml");
    let toml = fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read `{}`: {err}", path.display()))?;
    let config: ConfigToml = toml::from_str(&toml)
        .map_err(|err| format!("Failed to parse `{}`: {err}", path.display()))?;
    let config = Config {
        root: root.to_string(),
        config_toml: config,
    };
    config.validate()?;
    Ok(config)
}

impl Config {
    fn validate(&self) -> Result<(), String> {
        validate_main_class_name(&self.main_class_name())
    }

    fn project_path(&self, relative: &str) -> PathBuf {
        PathBuf::from(&self.root).join(relative)
    }

    pub fn src_path(&self) -> String {
        path_to_string(self.src_dir())
    }

    pub fn src_dir(&self) -> PathBuf {
        self.project_path(self.config_toml.package.root.as_deref().unwrap_or("./src"))
    }

    pub fn test_root_dir(&self) -> PathBuf {
        self.project_path(
            self.config_toml
                .package
                .test_root
                .as_deref()
                .unwrap_or("./src/test/java"),
        )
    }

    pub fn jar_path(&self) -> String {
        let cfg = &self.config_toml;
        let name = cfg.package.name.clone();
        let version = cfg.package.version.clone().unwrap_or("0.0.1".to_string());
        let jar_name = format!("{}-{}.jar", name, version);
        let jar_name = cfg.package.jar_name.as_ref().unwrap_or(&jar_name);
        return format!("{}/{}", self.root, jar_name);
    }

    pub fn jar_file(&self) -> PathBuf {
        PathBuf::from(self.jar_path())
    }

    pub fn main_path(&self) -> String {
        path_to_string(self.main_file())
    }

    pub fn main_file(&self) -> PathBuf {
        let mut relative = PathBuf::new();
        for segment in self.main_class_name().split('.') {
            relative.push(segment);
        }
        relative.set_extension("java");
        self.src_dir().join(relative)
    }

    pub fn main_class_name(&self) -> String {
        let cfg = &self.config_toml;
        let package = &cfg.package;
        package
            .main
            .clone()
            .unwrap_or_else(|| "com.example.Main".to_string())
    }

    pub fn target_path(&self) -> String {
        return format!("{}/target", self.root);
    }

    pub fn target_dir(&self) -> PathBuf {
        PathBuf::from(self.target_path())
    }

    pub fn classes_dir(&self) -> PathBuf {
        self.target_dir().join("classes")
    }

    pub fn test_classes_dir(&self) -> PathBuf {
        self.target_dir().join("test-classes")
    }

    pub fn deps_dir(&self) -> PathBuf {
        self.target_dir().join("deps")
    }

    pub fn tools_dir(&self) -> PathBuf {
        self.target_dir().join("tools")
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.target_dir().join("MANIFEST.MF")
    }

    pub fn jar_mode(&self) -> JarMode {
        self.config_toml
            .build
            .as_ref()
            .and_then(|build| build.jar_mode)
            .unwrap_or_default()
    }

    pub fn repositories(&self) -> &[Repository] {
        self.config_toml.repositories.as_deref().unwrap_or(&[])
    }

    pub fn resource_dirs(&self) -> Vec<PathBuf> {
        self.config_toml
            .resources
            .as_ref()
            .and_then(|resources| resources.dirs.as_ref())
            .into_iter()
            .flatten()
            .map(|dir| PathBuf::from(&self.root).join(dir))
            .collect()
    }
}

fn validate_main_class_name(main: &str) -> Result<(), String> {
    if main.contains('/') || main.contains('\\') || main.ends_with(".java") {
        return Err(format!(
            "`[package].main` must be a fully qualified Java class name like `com.example.Main`, not a source path: `{main}`"
        ));
    }
    if main.trim() != main || main.is_empty() {
        return Err(
            "`[package].main` must not be empty or contain surrounding whitespace".to_string(),
        );
    }

    for segment in main.split('.') {
        if !is_java_identifier(segment) {
            return Err(format!(
                "`[package].main` must be a fully qualified Java class name like `com.example.Main`: invalid segment `{segment}` in `{main}`"
            ));
        }
    }

    Ok(())
}

fn is_java_identifier(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        process,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn package_defaults_use_class_name_and_src_roots() {
        let root = test_dir("defaults");
        write_config(
            &root,
            r#"[package]
name = "demo"

[dependencies]
"#,
        );

        let config = try_read_toml(root.to_str().unwrap()).unwrap();

        assert_eq!(config.main_class_name(), "com.example.Main");
        assert_eq!(config.src_dir(), root.join("./src"));
        assert_eq!(config.test_root_dir(), root.join("./src/test/java"));
        assert_eq!(
            PathBuf::from(config.main_path()),
            root.join("./src/com/example/Main.java")
        );
    }

    #[test]
    fn package_root_and_test_root_are_configurable() {
        let root = test_dir("custom-roots");
        write_config(
            &root,
            r#"[package]
name = "demo"
root = "./app/src"
test_root = "./tests/java"
main = "io.github.sammers.pla.Main"
"#,
        );

        let config = try_read_toml(root.to_str().unwrap()).unwrap();

        assert_eq!(config.main_class_name(), "io.github.sammers.pla.Main");
        assert_eq!(config.src_dir(), root.join("./app/src"));
        assert_eq!(config.test_root_dir(), root.join("./tests/java"));
        assert_eq!(
            PathBuf::from(config.main_path()),
            root.join("./app/src/io/github/sammers/pla/Main.java")
        );
    }

    #[test]
    fn package_main_rejects_path_style_values() {
        let root = test_dir("path-main");
        write_config(
            &root,
            r#"[package]
name = "demo"
main = "com/example/Main.java"
"#,
        );

        let error = try_read_toml(root.to_str().unwrap()).unwrap_err();

        assert!(error.contains("fully qualified Java class name"));
        assert!(error.contains("not a source path"));
    }

    #[test]
    fn package_main_rejects_invalid_class_names() {
        let root = test_dir("bad-class");
        write_config(
            &root,
            r#"[package]
name = "demo"
main = "com.example.1Main"
"#,
        );

        let error = try_read_toml(root.to_str().unwrap()).unwrap_err();

        assert!(error.contains("invalid segment"));
    }

    fn test_dir(name: &str) -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = PathBuf::from("target").join("config-tests").join(format!(
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

    fn write_config(root: &Path, contents: &str) {
        fs::write(root.join("ax.toml"), contents).unwrap();
    }
}
