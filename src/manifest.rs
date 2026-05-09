use crate::maven::Coordinate;
use std::{fs, path::Path};
use toml_edit::{DocumentMut, Item, Table, value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencySpec {
    pub key: String,
    pub version: String,
}

pub fn add_dependencies(root: &Path, specs: &[String]) -> Result<Vec<DependencySpec>, String> {
    let parsed = specs
        .iter()
        .map(|spec| parse_add_spec(spec))
        .collect::<Result<Vec<_>, _>>()?;
    let path = root.join("ax.toml");
    let mut document = read_document(&path)?;
    let dependencies = dependencies_table_mut(&mut document)?;

    for spec in &parsed {
        dependencies.insert(&spec.key, value(&spec.version));
    }

    fs::write(&path, document.to_string())
        .map_err(|err| format!("Failed to write `{}`: {err}", path.display()))?;
    Ok(parsed)
}

pub fn remove_dependencies(root: &Path, specs: &[String]) -> Result<Vec<String>, String> {
    let parsed = specs
        .iter()
        .map(|spec| parse_remove_spec(spec))
        .collect::<Result<Vec<_>, _>>()?;
    let path = root.join("ax.toml");
    let mut document = read_document(&path)?;
    let Some(dependencies) = document
        .as_table_mut()
        .get_mut("dependencies")
        .and_then(Item::as_table_mut)
    else {
        return Err("`ax.toml` does not contain a `[dependencies]` table".to_string());
    };

    for key in &parsed {
        if !dependencies.contains_key(key) {
            return Err(format!("Dependency `{key}` is not present in `ax.toml`"));
        }
    }
    for key in &parsed {
        dependencies.remove(key);
    }

    fs::write(&path, document.to_string())
        .map_err(|err| format!("Failed to write `{}`: {err}", path.display()))?;
    Ok(parsed)
}

fn parse_add_spec(spec: &str) -> Result<DependencySpec, String> {
    let Some((key, version)) = spec.split_once('@') else {
        return Err(format!(
            "Invalid dependency spec `{spec}`. Expected `group:artifact@version`"
        ));
    };
    let coordinate = Coordinate::parse_dependency(key, version)?;
    Ok(DependencySpec {
        key: format!("{}:{}", coordinate.group_id, coordinate.artifact_id),
        version: coordinate.version,
    })
}

fn parse_remove_spec(spec: &str) -> Result<String, String> {
    if spec.contains('@') {
        return Err(format!(
            "Invalid dependency spec `{spec}`. Expected `group:artifact`"
        ));
    }
    let (group_id, artifact_id) = parse_group_artifact(spec)?;
    Ok(format!("{group_id}:{artifact_id}"))
}

fn parse_group_artifact(spec: &str) -> Result<(String, String), String> {
    let parts = spec.split(':').collect::<Vec<_>>();
    if parts.len() != 2 || parts[0].trim().is_empty() || parts[1].trim().is_empty() {
        return Err(format!(
            "Invalid dependency coordinate `{spec}`. Expected `group:artifact`"
        ));
    }
    Ok((parts[0].trim().to_string(), parts[1].trim().to_string()))
}

fn read_document(path: &Path) -> Result<DocumentMut, String> {
    let contents = fs::read_to_string(path)
        .map_err(|err| format!("Failed to read `{}`: {err}", path.display()))?;
    contents
        .parse::<DocumentMut>()
        .map_err(|err| format!("Failed to parse `{}`: {err}", path.display()))
}

fn dependencies_table_mut(document: &mut DocumentMut) -> Result<&mut Table, String> {
    let item = document
        .as_table_mut()
        .entry("dependencies")
        .or_insert_with(|| Item::Table(Table::new()));
    item.as_table_mut()
        .ok_or_else(|| "`dependencies` exists but is not a table".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_add_specs() {
        let spec = parse_add_spec("org.example:demo@1.2.3").unwrap();

        assert_eq!(
            spec,
            DependencySpec {
                key: "org.example:demo".to_string(),
                version: "1.2.3".to_string()
            }
        );
    }

    #[test]
    fn rejects_add_specs_without_versions() {
        let error = parse_add_spec("org.example:demo").unwrap_err();

        assert!(error.contains("group:artifact@version"));
    }

    #[test]
    fn parses_remove_specs() {
        assert_eq!(
            parse_remove_spec("org.example:demo").unwrap(),
            "org.example:demo"
        );
    }

    #[test]
    fn rejects_remove_specs_with_versions() {
        let error = parse_remove_spec("org.example:demo@1.2.3").unwrap_err();

        assert!(error.contains("group:artifact"));
    }
}
