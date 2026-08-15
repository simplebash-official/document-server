/// Creates a standard success response for a feature module's status endpoint.
pub fn module_status_response(
    module_name: &str,
) -> axum::Json<crate::core::response::ApiResponse<crate::domain::ModuleStatusResponse>> {
    axum::Json(crate::core::response::ApiResponse::success(
        crate::domain::ModuleStatusResponse {
            module: module_name.to_string(),
            status: "ok".to_string(),
        },
        format!("{module_name} module status retrieved successfully"),
    ))
}
