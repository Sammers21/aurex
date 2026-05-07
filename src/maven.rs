use crate::config::{Config, Repository as ConfigRepository};
use reqwest::{blocking::Client, StatusCode};
use roxmltree::{Document, Node};
use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    path::{Path, PathBuf},
    time::Duration,
};

const MAVEN_CENTRAL: &str = "https://repo1.maven.org/maven2";

pub type MavenResult<T> = Result<T, String>;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Coordinate {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
}

impl Coordinate {
    pub fn new(group_id: String, artifact_id: String, version: String) -> MavenResult<Self> {
        let coord = Self {
            group_id: group_id.trim().to_string(),
            artifact_id: artifact_id.trim().to_string(),
            version: version.trim().to_string(),
        };
        if coord.group_id.is_empty() || coord.artifact_id.is_empty() || coord.version.is_empty() {
            return Err(
                "Maven coordinates must include groupId, artifactId, and version".to_string(),
            );
        }
        reject_snapshot(&coord.version)?;
        Ok(coord)
    }

    pub fn parse_dependency(name: &str, version: &str) -> MavenResult<Self> {
        let parts: Vec<&str> = name.split(':').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid dependency coordinate `{name}`. Expected `groupId:artifactId`"
            ));
        }
        Self::new(
            parts[0].to_string(),
            parts[1].to_string(),
            version.to_string(),
        )
    }

    pub fn artifact_path(&self, extension: &str) -> String {
        format!(
            "{}/{}/{}/{}-{}.{}",
            self.group_id.replace('.', "/"),
            self.artifact_id,
            self.version,
            self.artifact_id,
            self.version,
            extension
        )
    }

    fn ga_key(&self) -> String {
        format!("{}:{}", self.group_id, self.artifact_id)
    }

    fn jar_file_name(&self) -> String {
        format!("{}-{}.jar", self.artifact_id, self.version)
    }

    fn pom_file_name(&self) -> String {
        format!(
            "{}-{}-{}.pom",
            self.group_id, self.artifact_id, self.version
        )
    }
}

impl fmt::Display for Coordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.group_id, self.artifact_id, self.version)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Exclusion {
    pub group_id: String,
    pub artifact_id: String,
}

impl Exclusion {
    fn matches(&self, group_id: &str, artifact_id: &str) -> bool {
        (self.group_id == "*" || self.group_id == group_id)
            && (self.artifact_id == "*" || self.artifact_id == artifact_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedArtifact {
    pub coordinate: Coordinate,
    pub jar_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MavenRepository {
    pub name: String,
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl MavenRepository {
    fn from_config(index: usize, repository: &ConfigRepository) -> MavenResult<Self> {
        let has_username = repository
            .username
            .as_deref()
            .is_some_and(|value| !value.is_empty());
        let has_password = repository
            .password
            .as_deref()
            .is_some_and(|value| !value.is_empty());
        if has_username != has_password {
            return Err(format!(
                "Repository `{}` must configure both username and password for basic auth",
                repository
                    .name
                    .as_deref()
                    .unwrap_or_else(|| repository.url.as_str())
            ));
        }
        if repository.url.trim().is_empty() {
            return Err(format!("Repository #{index} has an empty url"));
        }
        Ok(Self {
            name: repository
                .name
                .clone()
                .unwrap_or_else(|| format!("repository-{index}")),
            url: repository.url.trim_end_matches('/').to_string(),
            username: repository
                .username
                .clone()
                .filter(|value| !value.is_empty()),
            password: repository
                .password
                .clone()
                .filter(|value| !value.is_empty()),
        })
    }

    fn central() -> Self {
        Self {
            name: "maven-central".to_string(),
            url: MAVEN_CENTRAL.to_string(),
            username: None,
            password: None,
        }
    }

    pub fn artifact_url(&self, artifact_path: &str) -> String {
        format!("{}/{}", self.url.trim_end_matches('/'), artifact_path)
    }

    fn is_file_repository(&self) -> bool {
        self.url.starts_with("file://")
    }

    fn file_base_path(&self) -> PathBuf {
        file_url_to_path(&self.url)
    }
}

pub fn repositories_from_config(
    repositories: &[ConfigRepository],
) -> MavenResult<Vec<MavenRepository>> {
    let mut resolved = Vec::new();
    for (index, repository) in repositories.iter().enumerate() {
        resolved.push(MavenRepository::from_config(index + 1, repository)?);
    }
    resolved.push(MavenRepository::central());
    Ok(resolved)
}

pub fn resolve_dependencies(config: &Config) -> MavenResult<Vec<ResolvedArtifact>> {
    let repositories = repositories_from_config(config.repositories())?;
    let mut resolver = MavenResolver::new(repositories, config.deps_dir())?;
    let mut roots = Vec::new();

    if let Some(dependencies) = &config.config_toml.dependencies {
        let mut entries: Vec<(&String, &String)> = dependencies.iter().collect();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        for (name, version) in entries {
            roots.push(Coordinate::parse_dependency(name, version)?);
        }
    }

    resolver.resolve_roots(&roots)
}

pub struct MavenResolver {
    repositories: Vec<MavenRepository>,
    deps_dir: PathBuf,
    client: Client,
    pom_cache: HashMap<Coordinate, EffectivePom>,
    resolving_poms: HashSet<Coordinate>,
}

impl MavenResolver {
    pub fn new(repositories: Vec<MavenRepository>, deps_dir: PathBuf) -> MavenResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| format!("Failed to create HTTP client: {err}"))?;
        Ok(Self {
            repositories,
            deps_dir,
            client,
            pom_cache: HashMap::new(),
            resolving_poms: HashSet::new(),
        })
    }

    pub fn resolve_roots(&mut self, roots: &[Coordinate]) -> MavenResult<Vec<ResolvedArtifact>> {
        fs::create_dir_all(&self.deps_dir).map_err(|err| {
            format!(
                "Failed to create dependency directory `{}`: {err}",
                self.deps_dir.display()
            )
        })?;

        let mut artifacts = Vec::new();
        let mut selected = HashSet::new();
        let mut resolving = HashSet::new();

        for root in roots {
            self.resolve_artifact(root, &[], &mut selected, &mut artifacts, &mut resolving)?;
        }

        Ok(artifacts)
    }

    fn resolve_artifact(
        &mut self,
        coordinate: &Coordinate,
        inherited_exclusions: &[Exclusion],
        selected: &mut HashSet<String>,
        artifacts: &mut Vec<ResolvedArtifact>,
        resolving: &mut HashSet<Coordinate>,
    ) -> MavenResult<()> {
        if is_excluded(inherited_exclusions, coordinate) {
            return Ok(());
        }
        reject_snapshot(&coordinate.version)?;

        let ga_key = coordinate.ga_key();
        if selected.contains(&ga_key) {
            return Ok(());
        }
        if !resolving.insert(coordinate.clone()) {
            return Ok(());
        }

        let result = (|| {
            let pom = self.load_effective_pom(coordinate)?;
            selected.insert(ga_key);

            if pom.packaging != "pom" {
                let jar_path = self.fetch_jar(coordinate)?;
                artifacts.push(ResolvedArtifact {
                    coordinate: coordinate.clone(),
                    jar_path,
                });
            }

            for dependency in pom.dependencies {
                if !dependency.should_include() {
                    continue;
                }
                let child = dependency.to_coordinate()?;
                let mut child_exclusions = inherited_exclusions.to_vec();
                child_exclusions.extend(dependency.exclusions.clone());
                if is_excluded(&child_exclusions, &child) {
                    continue;
                }
                self.resolve_artifact(&child, &child_exclusions, selected, artifacts, resolving)?;
            }

            Ok(())
        })();

        resolving.remove(coordinate);
        result
    }

    fn load_effective_pom(&mut self, coordinate: &Coordinate) -> MavenResult<EffectivePom> {
        if let Some(cached) = self.pom_cache.get(coordinate) {
            return Ok(cached.clone());
        }
        if !self.resolving_poms.insert(coordinate.clone()) {
            return Err(format!(
                "Detected a parent/BOM cycle while resolving {coordinate}"
            ));
        }

        let result = (|| {
            let bytes = self.fetch_pom(coordinate)?;
            let xml = std::str::from_utf8(&bytes)
                .map_err(|err| format!("POM for {coordinate} is not valid UTF-8: {err}"))?;
            let raw = parse_pom(xml)?;
            let parent = match &raw.parent {
                Some(parent) => Some(self.load_effective_pom(&parent.to_coordinate()?)?),
                None => None,
            };
            let effective = self.build_effective_pom(coordinate, raw, parent.as_ref())?;
            self.pom_cache.insert(coordinate.clone(), effective.clone());
            Ok(effective)
        })();

        self.resolving_poms.remove(coordinate);
        result
    }

    fn build_effective_pom(
        &mut self,
        requested: &Coordinate,
        raw: RawPom,
        parent: Option<&EffectivePom>,
    ) -> MavenResult<EffectivePom> {
        let mut properties = parent
            .map(|parent| parent.properties.clone())
            .unwrap_or_default();

        for (key, value) in raw.properties {
            properties.insert(key, value);
        }

        if let Some(parent) = parent {
            properties.insert(
                "project.parent.groupId".to_string(),
                parent.coordinate.group_id.clone(),
            );
            properties.insert(
                "project.parent.artifactId".to_string(),
                parent.coordinate.artifact_id.clone(),
            );
            properties.insert(
                "project.parent.version".to_string(),
                parent.coordinate.version.clone(),
            );
        }

        let raw_group_id = raw
            .group_id
            .or_else(|| parent.map(|parent| parent.coordinate.group_id.clone()))
            .unwrap_or_else(|| requested.group_id.clone());
        let raw_artifact_id = raw
            .artifact_id
            .unwrap_or_else(|| requested.artifact_id.clone());
        let raw_version = raw
            .version
            .or_else(|| parent.map(|parent| parent.coordinate.version.clone()))
            .unwrap_or_else(|| requested.version.clone());

        resolve_all_properties(&mut properties);
        let group_id = interpolate(&raw_group_id, &properties);
        let artifact_id = interpolate(&raw_artifact_id, &properties);
        let version = interpolate(&raw_version, &properties);
        reject_snapshot(&version)?;

        let coordinate = Coordinate::new(group_id, artifact_id, version)?;
        add_project_properties(&mut properties, &coordinate);
        resolve_all_properties(&mut properties);

        let mut dependency_management = parent
            .map(|parent| parent.dependency_management.clone())
            .unwrap_or_default();

        for raw_dependency in raw.dependency_management {
            let mut dependency = raw_dependency.resolve(&properties)?;
            apply_management(&mut dependency, &dependency_management);

            if dependency.is_import_bom() {
                let bom = dependency.to_coordinate()?;
                let bom_pom = self.load_effective_pom(&bom)?;
                for (key, managed) in bom_pom.dependency_management {
                    dependency_management.insert(key, managed);
                }
                continue;
            }

            let key = dependency.ga_key();
            dependency_management.insert(key, ManagedDependency::from_dependency(&dependency));
        }

        let mut dependencies = Vec::new();
        for raw_dependency in raw.dependencies {
            let mut dependency = raw_dependency.resolve(&properties)?;
            apply_management(&mut dependency, &dependency_management);
            dependencies.push(dependency);
        }

        Ok(EffectivePom {
            coordinate,
            packaging: raw
                .packaging
                .as_deref()
                .map(|value| interpolate(value, &properties))
                .unwrap_or_else(|| "jar".to_string()),
            properties,
            dependency_management,
            dependencies,
        })
    }

    fn fetch_pom(&self, coordinate: &Coordinate) -> MavenResult<Vec<u8>> {
        let local_path = self.deps_dir.join(coordinate.pom_file_name());
        self.fetch_file(&coordinate.artifact_path("pom"), &local_path)
    }

    fn fetch_jar(&self, coordinate: &Coordinate) -> MavenResult<PathBuf> {
        let local_path = self.deps_dir.join(coordinate.jar_file_name());
        self.fetch_file(&coordinate.artifact_path("jar"), &local_path)?;
        Ok(local_path)
    }

    fn fetch_file(&self, artifact_path: &str, local_path: &Path) -> MavenResult<Vec<u8>> {
        if local_path.exists() {
            return fs::read(local_path)
                .map_err(|err| format!("Failed to read `{}`: {err}", local_path.display()));
        }

        fs::create_dir_all(&self.deps_dir).map_err(|err| {
            format!(
                "Failed to create dependency directory `{}`: {err}",
                self.deps_dir.display()
            )
        })?;

        let mut failures = Vec::new();
        for repository in &self.repositories {
            match self.fetch_from_repository(repository, artifact_path) {
                FetchResult::Found(bytes) => {
                    fs::write(local_path, &bytes).map_err(|err| {
                        format!("Failed to write `{}`: {err}", local_path.display())
                    })?;
                    return Ok(bytes);
                }
                FetchResult::NotFound => {}
                FetchResult::Fatal(message) => return Err(message),
                FetchResult::Failed(message) => failures.push(message),
            }
        }

        if failures.is_empty() {
            Err(format!(
                "Could not find Maven artifact `{artifact_path}` in configured repositories or Maven Central"
            ))
        } else {
            Err(format!(
                "Could not download Maven artifact `{artifact_path}`: {}",
                failures.join("; ")
            ))
        }
    }

    fn fetch_from_repository(
        &self,
        repository: &MavenRepository,
        artifact_path: &str,
    ) -> FetchResult {
        if repository.is_file_repository() {
            let path = repository.file_base_path().join(artifact_path);
            return match fs::read(&path) {
                Ok(bytes) => FetchResult::Found(bytes),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => FetchResult::NotFound,
                Err(err) => FetchResult::Failed(format!(
                    "{} failed to read `{}`: {err}",
                    repository.name,
                    path.display()
                )),
            };
        }

        let url = repository.artifact_url(artifact_path);
        let mut request = self.client.get(&url);
        if let Some(username) = &repository.username {
            request = request.basic_auth(username, repository.password.as_deref());
        }

        let response = match request.send() {
            Ok(response) => response,
            Err(err) => {
                return FetchResult::Failed(format!(
                    "{} failed to fetch `{url}`: {err}",
                    repository.name
                ))
            }
        };

        match response.status() {
            status if status.is_success() => match response.bytes() {
                Ok(bytes) => FetchResult::Found(bytes.to_vec()),
                Err(err) => FetchResult::Failed(format!(
                    "{} failed to read `{url}` response body: {err}",
                    repository.name
                )),
            },
            StatusCode::NOT_FOUND => FetchResult::NotFound,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => FetchResult::Fatal(format!(
                "{} denied access to `{url}` with status {}",
                repository.name,
                response.status()
            )),
            status => FetchResult::Failed(format!(
                "{} returned status {status} for `{url}`",
                repository.name
            )),
        }
    }
}

enum FetchResult {
    Found(Vec<u8>),
    NotFound,
    Failed(String),
    Fatal(String),
}

#[derive(Clone, Debug)]
struct EffectivePom {
    coordinate: Coordinate,
    packaging: String,
    properties: HashMap<String, String>,
    dependency_management: HashMap<String, ManagedDependency>,
    dependencies: Vec<EffectiveDependency>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ManagedDependency {
    version: Option<String>,
    scope: Option<String>,
    optional: Option<bool>,
    dep_type: Option<String>,
    exclusions: Vec<Exclusion>,
}

impl ManagedDependency {
    fn from_dependency(dependency: &EffectiveDependency) -> Self {
        Self {
            version: dependency.version.clone(),
            scope: dependency.scope.clone(),
            optional: Some(dependency.optional),
            dep_type: dependency.dep_type.clone(),
            exclusions: dependency.exclusions.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EffectiveDependency {
    group_id: String,
    artifact_id: String,
    version: Option<String>,
    scope: Option<String>,
    optional: bool,
    dep_type: Option<String>,
    classifier: Option<String>,
    exclusions: Vec<Exclusion>,
}

impl EffectiveDependency {
    fn ga_key(&self) -> String {
        format!("{}:{}", self.group_id, self.artifact_id)
    }

    fn should_include(&self) -> bool {
        if self.optional || self.classifier.is_some() {
            return false;
        }

        let dep_type = self.dep_type.as_deref().unwrap_or("jar");
        if dep_type != "jar" {
            return false;
        }

        matches!(
            self.scope.as_deref().unwrap_or("compile"),
            "" | "compile" | "runtime"
        )
    }

    fn is_import_bom(&self) -> bool {
        self.scope.as_deref() == Some("import") && self.dep_type.as_deref() == Some("pom")
    }

    fn to_coordinate(&self) -> MavenResult<Coordinate> {
        let version = self.version.clone().ok_or_else(|| {
            format!(
                "Dependency {}:{} is missing a version and no dependencyManagement entry provides one",
                self.group_id, self.artifact_id
            )
        })?;
        Coordinate::new(self.group_id.clone(), self.artifact_id.clone(), version)
    }
}

#[derive(Clone, Debug)]
struct RawPom {
    group_id: Option<String>,
    artifact_id: Option<String>,
    version: Option<String>,
    packaging: Option<String>,
    parent: Option<RawParent>,
    properties: HashMap<String, String>,
    dependency_management: Vec<RawDependency>,
    dependencies: Vec<RawDependency>,
}

#[derive(Clone, Debug)]
struct RawParent {
    group_id: String,
    artifact_id: String,
    version: String,
}

impl RawParent {
    fn to_coordinate(&self) -> MavenResult<Coordinate> {
        Coordinate::new(
            self.group_id.clone(),
            self.artifact_id.clone(),
            self.version.clone(),
        )
    }
}

#[derive(Clone, Debug)]
struct RawDependency {
    group_id: Option<String>,
    artifact_id: Option<String>,
    version: Option<String>,
    scope: Option<String>,
    optional: Option<String>,
    dep_type: Option<String>,
    classifier: Option<String>,
    exclusions: Vec<Exclusion>,
}

impl RawDependency {
    fn resolve(&self, properties: &HashMap<String, String>) -> MavenResult<EffectiveDependency> {
        let group_id = self
            .group_id
            .as_deref()
            .map(|value| interpolate(value, properties))
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "POM dependency is missing groupId".to_string())?;
        let artifact_id = self
            .artifact_id
            .as_deref()
            .map(|value| interpolate(value, properties))
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "POM dependency is missing artifactId".to_string())?;
        let version = self
            .version
            .as_deref()
            .map(|value| interpolate(value, properties))
            .filter(|value| !value.is_empty());
        if let Some(version) = &version {
            reject_snapshot(version)?;
        }

        Ok(EffectiveDependency {
            group_id,
            artifact_id,
            version,
            scope: self
                .scope
                .as_deref()
                .map(|value| interpolate(value, properties)),
            optional: self
                .optional
                .as_deref()
                .map(|value| interpolate(value, properties).eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            dep_type: self
                .dep_type
                .as_deref()
                .map(|value| interpolate(value, properties)),
            classifier: self
                .classifier
                .as_deref()
                .map(|value| interpolate(value, properties))
                .filter(|value| !value.is_empty()),
            exclusions: self.exclusions.clone(),
        })
    }
}

fn parse_pom(xml: &str) -> MavenResult<RawPom> {
    let document = Document::parse(xml).map_err(|err| format!("Invalid POM XML: {err}"))?;
    let project = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "project")
        .ok_or_else(|| "Invalid POM XML: missing <project>".to_string())?;

    let parent = direct_child(project, "parent").map(|parent| {
        Ok::<RawParent, String>(RawParent {
            group_id: direct_child_text(parent, "groupId")
                .ok_or_else(|| "POM parent is missing groupId".to_string())?,
            artifact_id: direct_child_text(parent, "artifactId")
                .ok_or_else(|| "POM parent is missing artifactId".to_string())?,
            version: direct_child_text(parent, "version")
                .ok_or_else(|| "POM parent is missing version".to_string())?,
        })
    });

    let properties = direct_child(project, "properties")
        .map(parse_properties)
        .unwrap_or_default();

    let dependency_management = direct_child(project, "dependencyManagement")
        .and_then(|node| direct_child(node, "dependencies"))
        .map(parse_dependencies)
        .unwrap_or_default();

    let dependencies = direct_child(project, "dependencies")
        .map(parse_dependencies)
        .unwrap_or_default();

    Ok(RawPom {
        group_id: direct_child_text(project, "groupId"),
        artifact_id: direct_child_text(project, "artifactId"),
        version: direct_child_text(project, "version"),
        packaging: direct_child_text(project, "packaging"),
        parent: parent.transpose()?,
        properties,
        dependency_management,
        dependencies,
    })
}

fn parse_properties(node: Node<'_, '_>) -> HashMap<String, String> {
    node.children()
        .filter(|child| child.is_element())
        .filter_map(|child| {
            child
                .text()
                .map(|text| (child.tag_name().name().to_string(), text.trim().to_string()))
        })
        .collect()
}

fn parse_dependencies(node: Node<'_, '_>) -> Vec<RawDependency> {
    node.children()
        .filter(|child| child.is_element() && child.tag_name().name() == "dependency")
        .map(parse_dependency)
        .collect()
}

fn parse_dependency(node: Node<'_, '_>) -> RawDependency {
    let exclusions = direct_child(node, "exclusions")
        .map(|exclusions| {
            exclusions
                .children()
                .filter(|child| child.is_element() && child.tag_name().name() == "exclusion")
                .filter_map(|exclusion| {
                    Some(Exclusion {
                        group_id: direct_child_text(exclusion, "groupId")?,
                        artifact_id: direct_child_text(exclusion, "artifactId")?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    RawDependency {
        group_id: direct_child_text(node, "groupId"),
        artifact_id: direct_child_text(node, "artifactId"),
        version: direct_child_text(node, "version"),
        scope: direct_child_text(node, "scope"),
        optional: direct_child_text(node, "optional"),
        dep_type: direct_child_text(node, "type"),
        classifier: direct_child_text(node, "classifier"),
        exclusions,
    }
}

fn direct_child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

fn direct_child_text(node: Node<'_, '_>, name: &str) -> Option<String> {
    direct_child(node, name)
        .and_then(|child| child.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn add_project_properties(properties: &mut HashMap<String, String>, coordinate: &Coordinate) {
    properties.insert("project.groupId".to_string(), coordinate.group_id.clone());
    properties.insert(
        "project.artifactId".to_string(),
        coordinate.artifact_id.clone(),
    );
    properties.insert("project.version".to_string(), coordinate.version.clone());
    properties.insert("pom.groupId".to_string(), coordinate.group_id.clone());
    properties.insert("pom.artifactId".to_string(), coordinate.artifact_id.clone());
    properties.insert("pom.version".to_string(), coordinate.version.clone());
}

fn apply_management(
    dependency: &mut EffectiveDependency,
    dependency_management: &HashMap<String, ManagedDependency>,
) {
    let Some(managed) = dependency_management.get(&dependency.ga_key()) else {
        return;
    };

    if dependency.version.is_none() {
        dependency.version = managed.version.clone();
    }
    if dependency.scope.is_none() {
        dependency.scope = managed.scope.clone();
    }
    if dependency.dep_type.is_none() {
        dependency.dep_type = managed.dep_type.clone();
    }
    if !dependency.optional {
        dependency.optional = managed.optional.unwrap_or(false);
    }
    dependency.exclusions.extend(managed.exclusions.clone());
}

fn is_excluded(exclusions: &[Exclusion], coordinate: &Coordinate) -> bool {
    exclusions
        .iter()
        .any(|exclusion| exclusion.matches(&coordinate.group_id, &coordinate.artifact_id))
}

fn reject_snapshot(version: &str) -> MavenResult<()> {
    if version.to_ascii_uppercase().contains("SNAPSHOT") {
        return Err(format!(
            "SNAPSHOT dependency versions are not supported: `{version}`"
        ));
    }
    Ok(())
}

fn interpolate(value: &str, properties: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut rest = value;

    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            result.push_str(&rest[start..]);
            return result;
        };
        let key = &after_start[..end];
        if let Some(replacement) = properties.get(key) {
            result.push_str(replacement);
        } else {
            result.push_str("${");
            result.push_str(key);
            result.push('}');
        }
        rest = &after_start[end + 1..];
    }

    result.push_str(rest);
    result
}

fn resolve_all_properties(properties: &mut HashMap<String, String>) {
    for _ in 0..10 {
        let snapshot = properties.clone();
        let mut changed = false;
        for value in properties.values_mut() {
            let resolved = interpolate(value, &snapshot);
            if *value != resolved {
                *value = resolved;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

fn file_url_to_path(url: &str) -> PathBuf {
    let without_scheme = url.trim_start_matches("file://");

    #[cfg(windows)]
    {
        let mut path = without_scheme.replace('/', "\\");
        if path.starts_with('\\') && path.as_bytes().get(2) == Some(&b':') {
            path.remove(0);
        }
        PathBuf::from(path)
    }

    #[cfg(not(windows))]
    {
        PathBuf::from(without_scheme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::Path,
        process,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn parses_dependency_coordinate() {
        let coordinate = Coordinate::parse_dependency("org.example:demo", "1.2.3").unwrap();

        assert_eq!(coordinate.group_id, "org.example");
        assert_eq!(coordinate.artifact_id, "demo");
        assert_eq!(coordinate.version, "1.2.3");
        assert_eq!(
            coordinate.artifact_path("jar"),
            "org/example/demo/1.2.3/demo-1.2.3.jar"
        );
    }

    #[test]
    fn rejects_snapshot_versions() {
        let error = Coordinate::parse_dependency("org.example:demo", "1.0.0-SNAPSHOT").unwrap_err();

        assert!(error.contains("SNAPSHOT dependency versions are not supported"));
    }

    #[test]
    fn validates_repository_auth_pair() {
        let repositories = [ConfigRepository {
            name: Some("internal".to_string()),
            url: "https://repo.example.com/maven2".to_string(),
            username: Some("user".to_string()),
            password: None,
        }];

        let error = repositories_from_config(&repositories).unwrap_err();

        assert!(error.contains("must configure both username and password"));
    }

    #[test]
    fn builds_repository_artifact_urls() {
        let repository = MavenRepository {
            name: "internal".to_string(),
            url: "https://repo.example.com/maven2/".to_string(),
            username: None,
            password: None,
        };

        assert_eq!(
            repository.artifact_url("org/example/demo/1.0/demo-1.0.pom"),
            "https://repo.example.com/maven2/org/example/demo/1.0/demo-1.0.pom"
        );
    }

    #[test]
    fn parses_pom_dependencies_and_exclusions() {
        let pom = parse_pom(
            r#"
            <project>
              <modelVersion>4.0.0</modelVersion>
              <groupId>org.example</groupId>
              <artifactId>demo</artifactId>
              <version>1.0.0</version>
              <dependencies>
                <dependency>
                  <groupId>org.libs</groupId>
                  <artifactId>lib</artifactId>
                  <version>${lib.version}</version>
                  <scope>runtime</scope>
                  <exclusions>
                    <exclusion>
                      <groupId>org.skip</groupId>
                      <artifactId>bad</artifactId>
                    </exclusion>
                  </exclusions>
                </dependency>
              </dependencies>
              <properties>
                <lib.version>2.0.0</lib.version>
              </properties>
            </project>
            "#,
        )
        .unwrap();

        let dependency = pom.dependencies[0].resolve(&pom.properties).unwrap();

        assert_eq!(dependency.group_id, "org.libs");
        assert_eq!(dependency.artifact_id, "lib");
        assert_eq!(dependency.version.as_deref(), Some("2.0.0"));
        assert_eq!(dependency.scope.as_deref(), Some("runtime"));
        assert_eq!(
            dependency.exclusions,
            vec![Exclusion {
                group_id: "org.skip".to_string(),
                artifact_id: "bad".to_string()
            }]
        );
    }

    #[test]
    fn filters_dependency_scopes_and_optional_flags() {
        let compile = EffectiveDependency {
            group_id: "g".to_string(),
            artifact_id: "a".to_string(),
            version: Some("1".to_string()),
            scope: None,
            optional: false,
            dep_type: None,
            classifier: None,
            exclusions: vec![],
        };
        let provided = EffectiveDependency {
            scope: Some("provided".to_string()),
            ..compile.clone()
        };
        let optional = EffectiveDependency {
            optional: true,
            ..compile.clone()
        };
        let runtime = EffectiveDependency {
            scope: Some("runtime".to_string()),
            ..compile.clone()
        };
        let test = EffectiveDependency {
            scope: Some("test".to_string()),
            ..compile.clone()
        };
        let classifier = EffectiveDependency {
            classifier: Some("sources".to_string()),
            ..compile.clone()
        };
        let non_jar = EffectiveDependency {
            dep_type: Some("pom".to_string()),
            ..compile.clone()
        };

        assert!(compile.should_include());
        assert!(runtime.should_include());
        assert!(!provided.should_include());
        assert!(!optional.should_include());
        assert!(!test.should_include());
        assert!(!classifier.should_include());
        assert!(!non_jar.should_include());
    }

    #[test]
    fn applies_dependency_management_versions() {
        let mut dependency = EffectiveDependency {
            group_id: "org.example".to_string(),
            artifact_id: "managed".to_string(),
            version: None,
            scope: None,
            optional: false,
            dep_type: None,
            classifier: None,
            exclusions: vec![],
        };
        let mut management = HashMap::new();
        management.insert(
            dependency.ga_key(),
            ManagedDependency {
                version: Some("3.1.4".to_string()),
                scope: Some("runtime".to_string()),
                optional: None,
                dep_type: None,
                exclusions: vec![Exclusion {
                    group_id: "org.skip".to_string(),
                    artifact_id: "*".to_string(),
                }],
            },
        );

        apply_management(&mut dependency, &management);

        assert_eq!(dependency.version.as_deref(), Some("3.1.4"));
        assert_eq!(dependency.scope.as_deref(), Some("runtime"));
        assert!(is_excluded(
            &dependency.exclusions,
            &Coordinate::new(
                "org.skip".to_string(),
                "anything".to_string(),
                "1".to_string()
            )
            .unwrap()
        ));
    }

    #[test]
    fn resolves_transitive_dependencies_from_file_repository() {
        let root = test_dir("transitive");
        let repo = root.join("repo");
        let deps = root.join("deps");
        write_artifact(
            &repo,
            "com.acme",
            "app",
            "1.0.0",
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId>
  <artifactId>app</artifactId>
  <version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>com.acme</groupId>
      <artifactId>core</artifactId>
      <version>1.0.0</version>
    </dependency>
  </dependencies>
</project>
"#,
        );
        write_artifact(
            &repo,
            "com.acme",
            "core",
            "1.0.0",
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId>
  <artifactId>core</artifactId>
  <version>1.0.0</version>
</project>
"#,
        );

        let mut resolver = local_resolver(&repo, &deps);
        let artifacts = resolver
            .resolve_roots(&[Coordinate::new(
                "com.acme".to_string(),
                "app".to_string(),
                "1.0.0".to_string(),
            )
            .unwrap()])
            .unwrap();

        assert_eq!(
            artifact_coordinates(&artifacts),
            vec!["com.acme:app:1.0.0", "com.acme:core:1.0.0"]
        );
        assert!(deps.join("app-1.0.0.jar").exists());
        assert!(deps.join("core-1.0.0.jar").exists());
    }

    #[test]
    fn resolves_parent_and_imported_bom_dependency_management() {
        let root = test_dir("managed");
        let repo = root.join("repo");
        let deps = root.join("deps");
        write_artifact(
            &repo,
            "com.acme",
            "parent",
            "1.0.0",
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId>
  <artifactId>parent</artifactId>
  <version>1.0.0</version>
  <packaging>pom</packaging>
  <properties>
    <managed.version>1.0.0</managed.version>
  </properties>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>com.libs</groupId>
        <artifactId>managed</artifactId>
        <version>${managed.version}</version>
      </dependency>
    </dependencies>
  </dependencyManagement>
</project>
"#,
        );
        write_artifact(
            &repo,
            "com.acme",
            "versions-bom",
            "1.0.0",
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId>
  <artifactId>versions-bom</artifactId>
  <version>1.0.0</version>
  <packaging>pom</packaging>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>com.libs</groupId>
        <artifactId>bom-managed</artifactId>
        <version>1.0.0</version>
      </dependency>
    </dependencies>
  </dependencyManagement>
</project>
"#,
        );
        write_artifact(
            &repo,
            "com.acme",
            "app",
            "1.0.0",
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.acme</groupId>
    <artifactId>parent</artifactId>
    <version>1.0.0</version>
  </parent>
  <artifactId>app</artifactId>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>com.acme</groupId>
        <artifactId>versions-bom</artifactId>
        <version>1.0.0</version>
        <type>pom</type>
        <scope>import</scope>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>com.libs</groupId>
      <artifactId>managed</artifactId>
    </dependency>
    <dependency>
      <groupId>com.libs</groupId>
      <artifactId>bom-managed</artifactId>
    </dependency>
  </dependencies>
</project>
"#,
        );
        write_artifact(
            &repo,
            "com.libs",
            "managed",
            "1.0.0",
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.libs</groupId>
  <artifactId>managed</artifactId>
  <version>1.0.0</version>
</project>
"#,
        );
        write_artifact(
            &repo,
            "com.libs",
            "bom-managed",
            "1.0.0",
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.libs</groupId>
  <artifactId>bom-managed</artifactId>
  <version>1.0.0</version>
</project>
"#,
        );

        let mut resolver = local_resolver(&repo, &deps);
        let artifacts = resolver
            .resolve_roots(&[Coordinate::new(
                "com.acme".to_string(),
                "app".to_string(),
                "1.0.0".to_string(),
            )
            .unwrap()])
            .unwrap();

        assert_eq!(
            artifact_coordinates(&artifacts),
            vec![
                "com.acme:app:1.0.0",
                "com.libs:managed:1.0.0",
                "com.libs:bom-managed:1.0.0"
            ]
        );
        assert!(!deps.join("parent-1.0.0.jar").exists());
        assert!(!deps.join("versions-bom-1.0.0.jar").exists());
    }

    #[test]
    fn reports_dependencies_missing_versions_after_management() {
        let root = test_dir("missing-version");
        let repo = root.join("repo");
        let deps = root.join("deps");
        write_artifact(
            &repo,
            "com.acme",
            "app",
            "1.0.0",
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId>
  <artifactId>app</artifactId>
  <version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>com.libs</groupId>
      <artifactId>missing</artifactId>
    </dependency>
  </dependencies>
</project>
"#,
        );

        let mut resolver = local_resolver(&repo, &deps);
        let error = resolver
            .resolve_roots(&[Coordinate::new(
                "com.acme".to_string(),
                "app".to_string(),
                "1.0.0".to_string(),
            )
            .unwrap()])
            .unwrap_err();

        assert!(error.contains("is missing a version"));
    }

    fn local_resolver(repo: &Path, deps: &Path) -> MavenResolver {
        let repositories = repositories_from_config(&[ConfigRepository {
            name: Some("local".to_string()),
            url: file_url(repo),
            username: None,
            password: None,
        }])
        .unwrap();
        MavenResolver::new(repositories, deps.to_path_buf()).unwrap()
    }

    fn write_artifact(repo: &Path, group_id: &str, artifact_id: &str, version: &str, pom: &str) {
        let artifact_dir = repo
            .join(group_id.replace('.', "/"))
            .join(artifact_id)
            .join(version);
        fs::create_dir_all(&artifact_dir).unwrap();
        fs::write(
            artifact_dir.join(format!("{artifact_id}-{version}.pom")),
            pom,
        )
        .unwrap();
        fs::write(
            artifact_dir.join(format!("{artifact_id}-{version}.jar")),
            format!("{group_id}:{artifact_id}:{version}"),
        )
        .unwrap();
    }

    fn artifact_coordinates(artifacts: &[ResolvedArtifact]) -> Vec<String> {
        artifacts
            .iter()
            .map(|artifact| artifact.coordinate.to_string())
            .collect()
    }

    fn test_dir(name: &str) -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = PathBuf::from("target").join("maven-tests").join(format!(
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
}
