use serde::Serialize;
use crate::SharedState;

#[derive(Debug, Serialize)]
pub struct TemplateResponse {
    pub success: bool,
    pub data: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn template_command(
    state: tauri::State<'_, SharedState>,
    input: String,
) -> Result<TemplateResponse, String> {
    // let state = state.lock().await;
    
    Ok(TemplateResponse {
        success: true,
        data: Some(format!("Processed: {}", input)),
        error: None,
    })
}
