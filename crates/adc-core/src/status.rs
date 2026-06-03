use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusResponse {
    pub service: String,
    pub version: String,
    pub status: String,
    pub message: String,
}

pub fn status_for(service: impl Into<String>, version: impl Into<String>) -> StatusResponse {
    StatusResponse {
        service: service.into(),
        version: version.into(),
        status: "ready".to_string(),
        message: "adc-targetd MVP foundation is initialized".to_string(),
    }
}
