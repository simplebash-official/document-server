// Pure business types for the templates feature — no I/O, no SQLite/Axum
// types beyond serde/utoipa derives. SQLite row shapes live in
// `modules::templates::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Category of template: standard printable `document` (receipt, invoice, report)
/// or a `label` (thermal sticker, product tag, barcode label).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TemplateType {
    Document,
    Label,
}

impl std::fmt::Display for TemplateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Document => write!(f, "document"),
            Self::Label => write!(f, "label"),
        }
    }
}

impl std::str::FromStr for TemplateType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "document" => Ok(Self::Document),
            "label" => Ok(Self::Label),
            other => Err(format!("unknown template type: {other}")),
        }
    }
}

/// Who may see a template: every tenant (`global`, shipped with the app) or
/// only the tenants its family was granted to (`private`, a published
/// package).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TemplateScope {
    Global,
    Private,
}

impl std::fmt::Display for TemplateScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Global => write!(f, "global"),
            Self::Private => write!(f, "private"),
        }
    }
}

impl std::str::FromStr for TemplateScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "global" => Ok(Self::Global),
            "private" => Ok(Self::Private),
            other => Err(format!("unknown template scope: {other}")),
        }
    }
}

/// `published` templates can be rendered; `retired` ones only reprint the
/// documents they already produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TemplateStatus {
    Published,
    Retired,
}

impl std::fmt::Display for TemplateStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Published => write!(f, "published"),
            Self::Retired => write!(f, "retired"),
        }
    }
}

impl std::str::FromStr for TemplateStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "published" => Ok(Self::Published),
            "retired" => Ok(Self::Retired),
            other => Err(format!("unknown template status: {other}")),
        }
    }
}

/// Input control a template declares in its schema's `x-options`; the POS
/// renders it as a form field and sends the chosen values in the payload's
/// `options` object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum OptionType {
    Toggle,
    Select,
    Color,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptionChoice {
    pub value: String,
    pub label: String,
}

/// One option declared by a template (`x-options` entry in its schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateOption {
    /// Key inside the payload's `options` object.
    pub key: String,
    pub r#type: OptionType,
    /// Label shown next to the control.
    pub label: String,
    /// Initial value: a bool for `toggle`, one of the choice values for
    /// `select`, a `#RRGGBB` string for `color`, text for `text`.
    pub default: serde_json::Value,
    /// Required (and non-empty) for `select`, absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<OptionChoice>>,
    /// Required for `text` (1-200), absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
}

/// A template as returned to API clients. `key` (see `core::id::generate_id`)
/// is the row's external identifier; `name` is the generated on-disk identity
/// (`doc_temp_<nanoid>`). Templates of one design share a `family`; every
/// change is a new `version` (a new row and name — a published template is
/// never edited).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub key: String,
    pub name: String,
    pub family: String,
    pub version: i64,
    pub r#type: TemplateType,
    /// What the template is for, from its schema's `x-document-kind`
    /// (e.g. `a4-invoice`).
    pub kind: String,
    /// Human-readable title, from the schema's `title`.
    pub description: String,
    pub status: TemplateStatus,
    pub scope: TemplateScope,
    /// Whether this is the built-in default of its kind (schema `x-default`).
    pub is_default: bool,
    /// Options the template declares (schema `x-options`).
    pub options: Vec<TemplateOption>,
    /// The template's input contract (JSON Schema).
    pub data_schema: serde_json::Value,
    /// A worked example of valid input.
    pub data: serde_json::Value,
    /// SHA-256 (hex) of the stored package archive; of the `.typ` source for
    /// a bundled template.
    pub sha256: String,
    /// Oldest desktop app version that can use the package; `null` = any.
    pub min_app_version: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response body for `GET /api/templates`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplatesResponse {
    pub templates: Vec<Template>,
}

/// What a template's schema declares about the template itself, validated.
#[derive(Debug, Clone)]
pub struct SchemaInfo {
    pub title: String,
    pub kind: String,
    pub is_default: bool,
    pub options: Vec<TemplateOption>,
}

/// The schema's `x-options` array (absent = no options).
pub fn parse_options(schema: &serde_json::Value) -> Result<Vec<TemplateOption>, String> {
    match schema.get("x-options") {
        None => Ok(Vec::new()),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|err| format!("x-options is invalid: {err}")),
    }
}

fn is_option_key(key: &str) -> bool {
    let mut chars = key.chars();
    key.len() <= 40
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric())
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn validate_option(option: &TemplateOption) -> Result<(), String> {
    let key = &option.key;
    if !is_option_key(key) {
        return Err(format!(
            "option key '{key}' must be 1-40 letters/digits and start with a letter"
        ));
    }
    if option.label.trim().is_empty() {
        return Err(format!("option '{key}' needs a label"));
    }
    match option.r#type {
        OptionType::Toggle => {
            if !option.default.is_boolean() {
                return Err(format!(
                    "option '{key}': a toggle's default must be true or false"
                ));
            }
        }
        OptionType::Select => {
            let choices = option
                .choices
                .as_ref()
                .filter(|c| !c.is_empty())
                .ok_or_else(|| format!("option '{key}': a select needs at least one choice"))?;
            let default = option.default.as_str().ok_or_else(|| {
                format!("option '{key}': a select's default must be a choice value")
            })?;
            if !choices.iter().any(|c| c.value == default) {
                return Err(format!(
                    "option '{key}': default '{default}' is not one of the choices"
                ));
            }
            if choices.iter().any(|c| c.label.trim().is_empty()) {
                return Err(format!("option '{key}': every choice needs a label"));
            }
        }
        OptionType::Color => {
            if !option.default.as_str().is_some_and(is_hex_color) {
                return Err(format!(
                    "option '{key}': a color's default must look like #RRGGBB"
                ));
            }
        }
        OptionType::Text => {
            let max = option
                .max_length
                .filter(|m| (1..=200).contains(m))
                .ok_or_else(|| format!("option '{key}': a text option needs maxLength 1-200"))?;
            let default = option
                .default
                .as_str()
                .ok_or_else(|| format!("option '{key}': a text option's default must be text"))?;
            if default.chars().count() > max as usize {
                return Err(format!("option '{key}': default is longer than maxLength"));
            }
        }
    }
    if option.r#type != OptionType::Select && option.choices.is_some() {
        return Err(format!("option '{key}': only a select has choices"));
    }
    if option.r#type != OptionType::Text && option.max_length.is_some() {
        return Err(format!("option '{key}': only a text option has maxLength"));
    }
    Ok(())
}

/// Checks everything the server relies on in a template's schema: a valid
/// JSON Schema, a title, an `x-document-kind`, well-formed `x-options`, and —
/// when options are declared — that the payload contract requires an
/// `options` object defining each of them.
pub fn validate_template_schema(schema: &serde_json::Value) -> Result<SchemaInfo, String> {
    if !schema.is_object() {
        return Err("schema must be a JSON object".to_string());
    }
    jsonschema::validator_for(schema)
        .map_err(|err| format!("schema is not a valid JSON Schema: {err}"))?;

    let title = schema
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or("schema needs a non-empty title")?
        .to_string();

    let kind = schema
        .get("x-document-kind")
        .and_then(|v| v.as_str())
        .filter(|k| {
            !k.is_empty()
                && k.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        .ok_or("schema needs an x-document-kind of lowercase letters, digits and dashes")?
        .to_string();

    let is_default = match schema.get("x-default") {
        None => false,
        Some(serde_json::Value::Bool(flag)) => *flag,
        Some(_) => return Err("x-default must be true or false".to_string()),
    };

    let options = parse_options(schema)?;
    let mut seen = std::collections::HashSet::new();
    for option in &options {
        validate_option(option)?;
        if !seen.insert(option.key.as_str()) {
            return Err(format!("option key '{}' is declared twice", option.key));
        }
    }
    if !options.is_empty() {
        let defined = schema
            .pointer("/properties/options/properties")
            .and_then(|v| v.as_object())
            .ok_or("schema declares x-options but has no properties.options.properties")?;
        for option in &options {
            if !defined.contains_key(&option.key) {
                return Err(format!(
                    "option '{}' is missing from properties.options.properties",
                    option.key
                ));
            }
        }
        let required = schema
            .get("required")
            .and_then(|v| v.as_array())
            .is_some_and(|r| r.iter().any(|v| v.as_str() == Some("options")));
        if !required {
            return Err("schema declares x-options so \"options\" must be in required".to_string());
        }
    }

    Ok(SchemaInfo {
        title,
        kind,
        is_default,
        options,
    })
}
