use std::{collections::HashMap, path::PathBuf};

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
    let toml = std::fs::read_to_string(format!("{}/aurex.toml", root));
    if toml.is_err() {
        panic!("Failed to read the aurex.toml file: {}", toml.unwrap_err());
    }
    let config: ConfigToml = toml::from_str(&toml.unwrap()).unwrap();
    return Config {
        root: root.to_string(),
        config_toml: config,
    };
}

impl Config {
    pub fn src_path(&self) -> String {
        return format!("{}/src", self.root);
    }

    pub fn src_dir(&self) -> PathBuf {
        PathBuf::from(self.src_path())
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
        let cfg = &self.config_toml;
        let package = &cfg.package;
        let binding = "com/example/Main.java".to_string();
        let main = package.main.as_ref().unwrap_or(&binding);
        return format!("{}/src/{}", self.root, main);
    }

    pub fn main_class_name(&self) -> String {
        let cfg = &self.config_toml;
        let package = &cfg.package;
        let binding = "com/example/Main".to_string();
        let main = package.main.as_ref().unwrap_or(&binding);
        return main.replace("/", ".").replace(".java", "");
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

    pub fn deps_dir(&self) -> PathBuf {
        self.target_dir().join("deps")
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
