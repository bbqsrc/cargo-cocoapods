use cargo_metadata::{Package, Target};
use heck::CamelCase;
use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;
use std::fmt::Display;

pub struct Source {
    pub http: String,
}

#[non_exhaustive]
pub struct OsSubspec {
    pub deployment_target: String,
}

#[non_exhaustive]
pub struct Podspec {
    pub name: String,
    pub summary: String,
    pub version: String,
    pub authors: IndexMap<String, String>,
    pub license: String,
    pub homepage: String,
    pub source: Source,
    pub source_files: Vec<String>,
    pub macos: OsSubspec,
    pub ios: OsSubspec,
    pub vendored_frameworks: Vec<String>,
    pub pod_target_xcconfig: IndexMap<String, String>,
    pub prepare_command: Option<String>,
    pub preserve_paths: Vec<String>,
}

impl Podspec {
    pub(crate) fn add_target(&mut self, target: &Target) {
        match self.pod_target_xcconfig.get_mut("OTHER_LDFLAGS") {
            Some(v) => {
                v.push_str(&format!(" -l{}", target.name.replace("-", "_")));
            }
            None => {
                self.pod_target_xcconfig.insert(
                    "OTHER_LDFLAGS".into(),
                    format!("-l{}", target.name.replace("-", "_")),
                );
            }
        }
    }

    pub(crate) fn disable_bitcode(&mut self) {
        self.pod_target_xcconfig
            .insert("ENABLE_BITCODE".into(), "NO".into());
    }
}

static AUTHOR_RE: Lazy<Regex> = regex_static::lazy_regex!(r"^\s*(.+?)(?: <(.+?)>)?\s*$");
static SOURCE_RE: Lazy<Regex> =
    regex_static::lazy_regex!(r"^https://github\.com/(.*?)/(.*?)(?:\.git)?/?$");

impl From<Package> for Podspec {
    fn from(p: Package) -> Self {
        let mut authors = IndexMap::new();

        for line in p.authors {
            match AUTHOR_RE.captures(&line) {
                Some(cap) => {
                    let name = cap.get(1).map(|x| x.as_str()).unwrap_or("");
                    let email = cap.get(2).map(|x| x.as_str()).unwrap_or("");
                    authors.insert(name.to_string(), email.to_string());
                }
                None => {
                    log::warn!("Could not parse author line: '{}', skipping.", line);
                }
            }
        }

        if authors.is_empty() {
            authors.insert("Unknown".to_string(), "<EMAIL>".to_string());
        }

        let source = if let Some(repo) = &p.repository {
            let captures = SOURCE_RE.captures(&repo);
            match captures {
                Some(c) if c.get(1).is_some() && c.get(2).is_some() => {
                    format!("https://github.com/{}/{}/releases/download/v#{{spec.version}}/cargo-pod.tgz",
                        c.get(1).unwrap().as_str(),
                        c.get(2).unwrap().as_str())
                }
                _ => "UNKNOWN".into(),
            }
        } else {
            "UNKNOWN".into()
        };

        Podspec {
            name: p.name.to_camel_case(),
            summary: p.description.unwrap_or_else(|| "UNKNOWN".into()),
            version: p.version.to_string(),
            authors,
            license: p.license.unwrap_or_else(|| "UNKNOWN".into()),
            homepage: p.repository.clone().unwrap_or_else(|| "UNKNOWN".into()),
            source: Source { http: source },
            macos: OsSubspec {
                deployment_target: "10.10".into(),
            },
            ios: OsSubspec {
                deployment_target: "8.0".into(),
            },
            source_files: vec!["src/**/*".into()],
            vendored_frameworks: vec![format!("dist/{}.xcframework", p.name.to_camel_case())],
            pod_target_xcconfig: Default::default(),
            prepare_command: None,
            preserve_paths: vec![],
        }
    }
}

fn escape_apos(input: &str) -> String {
    input.replace("'", "\\'")
}

impl Display for Podspec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pod::Spec.new { |spec|\n")?;
        f.write_fmt(format_args!(
            "  spec.name = '{}'\n",
            escape_apos(&self.name)
        ))?;
        f.write_fmt(format_args!(
            "  spec.version = '{}'\n",
            escape_apos(&self.version)
        ))?;
        f.write_fmt(format_args!(
            "  spec.summary = '{}'\n",
            escape_apos(&self.summary)
        ))?;
        f.write_str("  spec.authors = {\n")?;
        for (name, email) in self.authors.iter() {
            f.write_fmt(format_args!(
                "    '{}' => '{}',\n",
                escape_apos(&name),
                escape_apos(&email)
            ))?;
        }
        f.write_str("  }\n")?;
        f.write_fmt(format_args!(
            "  spec.license = {{ :type => '{}' }}\n",
            escape_apos(&self.license)
        ))?;
        f.write_fmt(format_args!(
            "  spec.homepage = '{}'\n",
            escape_apos(&self.homepage)
        ))?;

        f.write_fmt(format_args!(
            "  spec.macos.deployment_target = '{}'\n",
            self.macos.deployment_target
        ))?;

        f.write_fmt(format_args!(
            "  spec.ios.deployment_target = '{}'\n",
            self.ios.deployment_target
        ))?;

        if !self.pod_target_xcconfig.is_empty() {
            f.write_str("  spec.pod_target_xcconfig = {\n")?;
            for (key, value) in self.pod_target_xcconfig.iter() {
                f.write_fmt(format_args!(
                    "    '{}' => '{}',\n",
                    escape_apos(&key),
                    escape_apos(&value)
                ))?;
            }
            f.write_str("  }\n")?;
        }
        if !self.preserve_paths.is_empty() {
            f.write_fmt(format_args!(
                "  spec.preserve_paths = ['{}']\n",
                self.preserve_paths.join("', '")
            ))?;
        }

        if !self.vendored_frameworks.is_empty() {
            f.write_fmt(format_args!(
                "  spec.macos.vendored_libraries = ['{}']\n",
                self.vendored_frameworks.join("', '")
            ))?;
        }

        if !self.source_files.is_empty() {
            f.write_fmt(format_args!(
                "  spec.source_files = ['{}']\n",
                self.source_files.join("', '")
            ))?;
        }

        f.write_str("  spec.source = {\n")?;
        f.write_fmt(format_args!("    :http => '{}',\n", self.source.http))?;
        f.write_str("  }\n")?;
        f.write_str("}\n")
    }
}
