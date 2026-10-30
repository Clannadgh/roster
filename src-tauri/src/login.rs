use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStart {
    pub login_id: String,
    pub verification_uri: String,
    pub expires_in: i64,
}
