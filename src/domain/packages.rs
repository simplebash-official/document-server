// API types for template packages — no I/O. A package is the zip a template
// is published as; see `modules::packages` for the archive layout.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::templates::{Template, TemplateType};

/// The only manifest format this server reads and writes.
pub const MANIFEST_FORMAT: u32 = 1;

/// `manifest.json` inside a package zip. An upload (`POST /api/packages`)
/// carries only `format`, `type` and optionally `minAppVersion`; the archive
/// the server stores and serves additionally names the package's identity, so
/// it can be replicated to another document-server unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub r#type: TemplateType,
    pub min_app_version: Option<String>,
    pub name: Option<String>,
    pub family: Option<String>,
    pub version: Option<i64>,
    pub created_at: Option<DateTime<Utc>>,
}

/// A template plus the tenants its family is granted to (admin view).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    #[serde(flatten)]
    pub template: Template,
    /// Tenant keys the family is granted to. Always empty for a global
    /// (bundled) template, which every tenant can use.
    pub grants: Vec<String>,
}

/// Response body for `GET /api/packages`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackagesResponse {
    pub packages: Vec<PackageInfo>,
}

/// Request body for `PUT /api/packages/families/{family}/grants`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantsRequest {
    /// The complete set of tenant keys the family is granted to.
    pub tenant_keys: Vec<String>,
}

/// Response body for `POST /api/packages/families/{family}/retire`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RetireResponse {
    /// How many versions were retired.
    pub retired_versions: u64,
}
