use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const API_PATH: &str = "/cli/v1/invoke";
pub const TOKEN_FILE: &str = ".cli-token";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CliRequest {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CliResponse {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CliResponseError>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CliResponseError {
    pub code: String,
    pub message: String,
}

impl CliResponse {
    pub fn success(id: impl Into<String>, data: Value) -> Self {
        Self {
            id: id.into(),
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn failure(id: impl Into<String>, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ok: false,
            data: None,
            error: Some(CliResponseError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn response_round_trip_preserves_error() {
        let response = CliResponse::failure("request-1", "invalid_argument", "bad value");
        let encoded = serde_json::to_vec(&response).expect("serialize response");
        let decoded = serde_json::from_slice::<CliResponse>(&encoded).expect("deserialize response");

        assert!(!decoded.ok);
        assert_eq!(decoded.id, "request-1");
        assert_eq!(
            decoded.error.as_ref().map(|error| error.code.as_str()),
            Some("invalid_argument")
        );
    }

    #[test]
    fn request_defaults_missing_params_to_null() {
        let request = serde_json::from_value::<CliRequest>(json!({
            "id": "request-2",
            "method": "status"
        }))
        .expect("deserialize request");

        assert_eq!(request.params, Value::Null);
    }
}
