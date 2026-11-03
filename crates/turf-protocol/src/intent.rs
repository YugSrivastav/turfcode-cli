use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Task status conforming to [MD-037] and TurfCode specification:
/// queued, in_progress, done, blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskStatus {
    Queued,
    InProgress,
    Done,
    Blocked,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::InProgress => "in_progress",
            Self::Done => "done",
            Self::Blocked => "blocked",
        }
    }

    pub fn display_badge(&self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::InProgress => "IN PROGRESS",
            Self::Done => "DONE",
            Self::Blocked => "BLOCKED",
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_lowercase().replace(['-', ' '], "_");
        match normalized.as_str() {
            "queued" | "queue" => Ok(Self::Queued),
            "in_progress" | "inprogress" | "progress" => Ok(Self::InProgress),
            "done" | "completed" => Ok(Self::Done),
            "blocked" | "block" => Ok(Self::Blocked),
            other => Err(format!("Unknown task status: '{other}'")),
        }
    }
}

impl Serialize for TaskStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<TaskStatus>().map_err(serde::de::Error::custom)
    }
}

/// Task card conforming to [MD-037] and Firstmate specification:
/// `id`, `title`, `assignee`, `status` (queued, in_progress, done, blocked), `target_files`, `created_at`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCard {
    pub id: String,
    pub title: String,
    pub assignee: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub target_files: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl TaskCard {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        assignee: impl Into<String>,
        status: TaskStatus,
        target_files: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            assignee: assignee.into(),
            status,
            target_files,
            created_at: Utc::now(),
        }
    }

    pub fn with_created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = created_at;
        self
    }
}

/// Persistent JSON format for .turf/intent-board.json conforming to [MD-014].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentBoardData {
    pub version: u32,
    pub updated_at: DateTime<Utc>,
    pub tasks: Vec<TaskCard>,
}

impl Default for IntentBoardData {
    fn default() -> Self {
        Self {
            version: 1,
            updated_at: Utc::now(),
            tasks: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_parsing_and_serialization() {
        assert_eq!("queued".parse::<TaskStatus>().unwrap(), TaskStatus::Queued);
        assert_eq!("in_progress".parse::<TaskStatus>().unwrap(), TaskStatus::InProgress);
        assert_eq!("inprogress".parse::<TaskStatus>().unwrap(), TaskStatus::InProgress);
        assert_eq!("done".parse::<TaskStatus>().unwrap(), TaskStatus::Done);
        assert_eq!("completed".parse::<TaskStatus>().unwrap(), TaskStatus::Done);
        assert_eq!("blocked".parse::<TaskStatus>().unwrap(), TaskStatus::Blocked);
        assert!("invalid".parse::<TaskStatus>().is_err());

        let json = serde_json::to_string(&TaskStatus::InProgress).unwrap();
        assert_eq!(json, "\"in_progress\"");
        let deserialized: TaskStatus = serde_json::from_str("\"in_progress\"").unwrap();
        assert_eq!(deserialized, TaskStatus::InProgress);
    }

    #[test]
    fn test_task_card_serialization() {
        let task = TaskCard::new("1", "Title", "Ayush", TaskStatus::Blocked, vec!["file.rs".into()]);
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"status\":\"blocked\""));

        let deserialized: TaskCard = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "1");
        assert_eq!(deserialized.status, TaskStatus::Blocked);
    }
}

