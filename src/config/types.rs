use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub upstreams: HashMap<String, UpstreamConfig>,
    pub tools: HashMap<String, ToolConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamConfig {
    pub url: Option<String>,
    pub cmd: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AuthType {
    #[default]
    None,
    Bearer,
    Basic,
    Header,
}

impl<'de> Deserialize<'de> for AuthType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "none" => Ok(Self::None),
            "bearer" => Ok(Self::Bearer),
            "basic" => Ok(Self::Basic),
            "header" => Ok(Self::Header),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["none", "bearer", "basic", "header"],
            )),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthConfig {
    #[serde(rename = "type", default)]
    pub auth_type: AuthType,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolConfig {
    pub description: String,
    pub detailed_help: Option<String>,
    #[serde(default)]
    pub parameters: Vec<ParamConfig>,
    pub steps: Vec<StepConfig>,
    pub output_fields: Vec<OutputFieldConfig>,
    pub aggregates: Vec<AggregateConfig>,
    pub next_steps: Vec<NextStepConfig>,
    pub empty_message: String,
    #[serde(default = "default_max_items")]
    pub max_items: u32,
}

const fn default_max_items() -> u32 {
    10
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    String,
    Number,
    Boolean,
}

impl std::fmt::Display for ParamType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String => f.write_str("string"),
            Self::Number => f.write_str("number"),
            Self::Boolean => f.write_str("boolean"),
        }
    }
}

impl<'de> Deserialize<'de> for ParamType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <String as Deserialize>::deserialize(deserializer)?;
        match s.as_str() {
            "string" => Ok(Self::String),
            "number" => Ok(Self::Number),
            "boolean" => Ok(Self::Boolean),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["string", "number", "boolean"],
            )),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParamConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: ParamType,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepConfig {
    pub name: String,
    pub upstream: String,
    pub tool: String,
    #[serde(default)]
    pub args: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub transform: Option<TransformConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransformConfig {
    pub pick: Option<Vec<String>>,
    pub rename: Option<HashMap<String, String>>,
    pub filter: Option<String>,
    pub truncate: Option<HashMap<String, usize>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputFieldConfig {
    pub name: String,
    pub description: String,
    pub max_len: Option<u32>,
    pub prefix: Option<String>,
    #[serde(default = "default_true")]
    pub default_visible: bool,
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct AggregateConfig {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NextStepConfig {
    pub command: String,
    pub description: String,
    pub when: Option<String>,
}
