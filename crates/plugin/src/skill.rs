//! Skills registry inspired by OpenClaw's SKILL.md system.
//!
//! A skill is a directory containing a `SKILL.md` manifest that describes
//! a capability: what tools it needs, what prompts to use, and metadata
//! for discovery. Skills allow configuring new agent behaviors without
//! recompiling Rust code.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Parsed skill manifest (from SKILL.md frontmatter + body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Unique skill identifier.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Semantic version.
    #[serde(default = "default_version")]
    pub version: String,
    /// Author or source.
    #[serde(default)]
    pub author: String,
    /// Tools this skill requires (by name).
    #[serde(default)]
    pub tools: Vec<String>,
    /// Tags for discovery.
    #[serde(default)]
    pub tags: Vec<String>,
    /// The system prompt template (markdown body after frontmatter).
    #[serde(default)]
    pub prompt_template: String,
    /// Directory the skill was loaded from.
    #[serde(skip)]
    pub source_dir: PathBuf,
}

fn default_version() -> String {
    "0.1.0".into()
}

impl SkillManifest {
    /// Parse a SKILL.md file. Expects TOML frontmatter between `+++` fences,
    /// followed by a markdown body used as the prompt template.
    ///
    /// ```text
    /// +++
    /// name = "api-tester"
    /// description = "Generates and runs API integration tests"
    /// tools = ["http_request", "shell"]
    /// tags = ["testing", "api"]
    /// +++
    ///
    /// You are an API testing specialist. Given an API spec, generate
    /// and execute test scenarios...
    /// ```
    pub fn parse(content: &str, source_dir: impl Into<PathBuf>) -> Result<Self, String> {
        let content = content.trim();

        // Split frontmatter from body.
        let (frontmatter, body) = if content.starts_with("+++") {
            let rest = &content[3..];
            if let Some(end) = rest.find("+++") {
                let fm = rest[..end].trim();
                let body = rest[end + 3..].trim();
                (fm, body)
            } else {
                return Err("unclosed +++ frontmatter".into());
            }
        } else {
            // No frontmatter — treat entire content as prompt, derive name from dir.
            ("", content)
        };

        let mut manifest: SkillManifest = if frontmatter.is_empty() {
            SkillManifest {
                name: String::new(),
                description: String::new(),
                version: default_version(),
                author: String::new(),
                tools: Vec::new(),
                tags: Vec::new(),
                prompt_template: String::new(),
                source_dir: PathBuf::new(),
            }
        } else {
            toml_parse_frontmatter(frontmatter)?
        };

        manifest.prompt_template = body.to_string();
        manifest.source_dir = source_dir.into();

        if manifest.name.is_empty() {
            // Derive name from directory.
            manifest.name = manifest
                .source_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unnamed".into());
        }

        Ok(manifest)
    }
}

/// Minimal TOML frontmatter parser (avoids pulling in full toml crate).
fn toml_parse_frontmatter(fm: &str) -> Result<SkillManifest, String> {
    let mut name = String::new();
    let mut description = String::new();
    let mut version = default_version();
    let mut author = String::new();
    let mut tools = Vec::new();
    let mut tags = Vec::new();

    for line in fm.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "name" => name = strip_quotes(val),
                "description" => description = strip_quotes(val),
                "version" => version = strip_quotes(val),
                "author" => author = strip_quotes(val),
                "tools" => tools = parse_string_array(val),
                "tags" => tags = parse_string_array(val),
                _ => {}
            }
        }
    }

    Ok(SkillManifest {
        name,
        description,
        version,
        author,
        tools,
        tags,
        prompt_template: String::new(),
        source_dir: PathBuf::new(),
    })
}

fn strip_quotes(s: &str) -> String {
    s.trim_matches('"').trim_matches('\'').to_string()
}

fn parse_string_array(s: &str) -> Vec<String> {
    let s = s.trim().trim_start_matches('[').trim_end_matches(']');
    s.split(',')
        .map(|item| strip_quotes(item.trim()))
        .filter(|item| !item.is_empty())
        .collect()
}

/// Registry of discovered skills.
pub struct SkillRegistry {
    skills: HashMap<String, SkillManifest>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    /// Load all skills from a directory. Each subdirectory containing
    /// a `SKILL.md` is treated as a skill.
    pub fn load_dir(&mut self, dir: &Path) -> Result<usize, String> {
        if !dir.is_dir() {
            return Ok(0);
        }
        let mut count = 0;
        let entries = std::fs::read_dir(dir).map_err(|e| format!("read dir: {e}"))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("entry: {e}"))?;
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("SKILL.md");
                if manifest_path.exists() {
                    let content =
                        std::fs::read_to_string(&manifest_path).map_err(|e| format!("read: {e}"))?;
                    match SkillManifest::parse(&content, &path) {
                        Ok(manifest) => {
                            tracing::info!(skill = %manifest.name, "loaded skill");
                            self.skills.insert(manifest.name.clone(), manifest);
                            count += 1;
                        }
                        Err(e) => {
                            tracing::warn!(path = %manifest_path.display(), error = %e, "skip skill");
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    /// Register a single skill manifest.
    pub fn register(&mut self, manifest: SkillManifest) {
        self.skills.insert(manifest.name.clone(), manifest);
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<&SkillManifest> {
        self.skills.get(name)
    }

    /// List all registered skills.
    pub fn list(&self) -> Vec<&SkillManifest> {
        self.skills.values().collect()
    }

    /// Search skills by tag.
    pub fn search_by_tag(&self, tag: &str) -> Vec<&SkillManifest> {
        self.skills
            .values()
            .filter(|s| s.tags.iter().any(|t| t == tag))
            .collect()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_manifest() {
        let content = r#"+++
name = "api-tester"
description = "Generates and runs API tests"
version = "1.0.0"
tools = ["http_request", "shell"]
tags = ["testing", "api"]
+++

You are an API testing specialist.
Given an API spec, generate test scenarios.
"#;
        let manifest = SkillManifest::parse(content, "/tmp/skills/api-tester").unwrap();
        assert_eq!(manifest.name, "api-tester");
        assert_eq!(manifest.description, "Generates and runs API tests");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.tools, vec!["http_request", "shell"]);
        assert_eq!(manifest.tags, vec!["testing", "api"]);
        assert!(manifest.prompt_template.contains("API testing specialist"));
    }

    #[test]
    fn parse_minimal_skill() {
        let content = "Just a prompt with no frontmatter.";
        let manifest = SkillManifest::parse(content, "/tmp/skills/quick").unwrap();
        assert_eq!(manifest.name, "quick");
        assert_eq!(manifest.prompt_template, "Just a prompt with no frontmatter.");
    }

    #[test]
    fn skill_registry_search() {
        let mut reg = SkillRegistry::new();
        reg.register(SkillManifest {
            name: "a".into(),
            description: "a".into(),
            version: "0.1.0".into(),
            author: "".into(),
            tools: vec![],
            tags: vec!["testing".into()],
            prompt_template: "".into(),
            source_dir: PathBuf::new(),
        });
        reg.register(SkillManifest {
            name: "b".into(),
            description: "b".into(),
            version: "0.1.0".into(),
            author: "".into(),
            tools: vec![],
            tags: vec!["deploy".into()],
            prompt_template: "".into(),
            source_dir: PathBuf::new(),
        });

        let found = reg.search_by_tag("testing");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "a");
    }
}
