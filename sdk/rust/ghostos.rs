use serde::{Deserialize, Serialize};

use super::{GhostTeamClient, GhostTeamError};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhostOsInferRequest {
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhostOsInferResponse {
    pub output: String,
}

pub async fn infer(
    client: &GhostTeamClient,
    request: &GhostOsInferRequest,
) -> Result<GhostOsInferResponse, GhostTeamError> {
    client.post_json("/ghostos/infer", request).await
}
