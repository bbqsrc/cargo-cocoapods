use cargo_metadata::Package;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
struct Metadata {
    pod: Option<Config>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub name: Option<String>,
    #[serde(default = "Vec::new")]
    pub features: Vec<String>,
}

pub fn config(package: &Package) -> Config {
    let meta: Metadata = match serde_json::from_value(package.metadata.clone()) {
        Ok(v) => v,
        Err(_e) => {
            return Default::default();
        }
    };
    meta.pod.unwrap_or_default()
}
