use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Timba {
    pub fields: Vec<String>,
    pub ty: TimbaType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct TimbaCreatedEvent {
    pub id: i64,
    pub name: String,
    pub time_of_creation: String,
}

#[derive(Serialize)]
pub struct CreateTimbaResponse {
    pub id: i64,
}

#[derive(Deserialize)]
pub struct VoteRequest {
    pub choices: Vec<String>,
}

#[derive(Serialize)]
pub struct VoteResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct VoteCount {
    pub choice: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct TimbaInfo {
    pub id: i64,
    pub name: String,
    pub time_of_creation: String,
    pub deadline: Option<String>,
    pub fields: Vec<String>,
    #[serde(rename = "type")]
    pub ty: String,
    pub votes: Vec<VoteCount>,
    pub total_votes: i64,
}

#[derive(Serialize)]
pub struct Stats {
    pub total_timbas: i64,
    pub total_votes: i64,
    pub oldest_timba: Option<String>,
    pub newest_timba: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub enum TimbaType {
    Simple,
    Multiple,
    YN,
    Scale(u64, u64),
}

impl TimbaType {
    pub fn to_string(&self) -> String {
        match self {
            TimbaType::Simple => "Simple".to_string(),
            TimbaType::Multiple => "Multiple".to_string(),
            TimbaType::YN => "YN".to_string(),
            TimbaType::Scale(lower, upper) => format!("Scale({},{})", lower, upper),
        }
    }
}
