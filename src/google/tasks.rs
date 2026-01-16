use crate::error::{MorrowError, Result};
use serde::{Deserialize, Serialize};

const TASKS_API_BASE: &str = "https://tasks.googleapis.com/tasks/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskList {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListsResponse {
    #[serde(default)]
    pub items: Vec<TaskList>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasksResponse {
    #[serde(default)]
    pub items: Vec<Task>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskInput {
    pub title: String,
    pub notes: Option<String>,
    pub due: Option<String>,
}

pub struct GoogleTasksClient {
    client: reqwest::Client,
    access_token: String,
}

impl GoogleTasksClient {
    pub fn new(access_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token,
        }
    }

    pub async fn list_task_lists(&self) -> Result<Vec<TaskList>> {
        let url = format!("{}/users/@me/lists", TASKS_API_BASE);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(MorrowError::Auth(format!(
                "Google Tasks API error {}: {}. Try running 'morrow auth' again.",
                status, text
            )));
        }

        let data: TaskListsResponse = resp.json().await?;
        Ok(data.items)
    }

    pub async fn find_list_by_name(&self, name: &str) -> Result<TaskList> {
        let lists = self.list_task_lists().await?;
        lists
            .into_iter()
            .find(|l| l.title == name)
            .ok_or_else(|| MorrowError::ListNotFound(name.to_string()))
    }

    pub async fn get_tasks(&self, list_id: &str, include_completed: bool) -> Result<Vec<Task>> {
        let url = format!("{}/lists/{}/tasks", TASKS_API_BASE, list_id);
        let show_completed = if include_completed { "true" } else { "false" };
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .query(&[("showCompleted", show_completed), ("maxResults", "100")])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(MorrowError::Auth(format!(
                "Google Tasks API error {}: {}. Try running 'morrow auth' again.",
                status, text
            )));
        }

        let data: TasksResponse = resp.json().await?;
        Ok(data.items)
    }

    /// Get all incomplete tasks from the source list.
    /// All tasks in this list are treated as "tomorrow's tasks" - no date filtering.
    pub async fn get_pending_tasks(&self, list_id: &str) -> Result<Vec<Task>> {
        self.get_tasks(list_id, false).await
    }

    pub async fn has_incomplete_tasks(&self, list_id: &str) -> Result<bool> {
        let tasks = self.get_tasks(list_id, true).await?;
        Ok(tasks.iter().any(|t| {
            t.status.as_deref() != Some("completed")
        }))
    }

    pub async fn create_task(&self, list_id: &str, task: TaskInput) -> Result<Task> {
        let url = format!("{}/lists/{}/tasks", TASKS_API_BASE, list_id);
        let resp: Task = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&task)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    pub async fn create_list(&self, title: &str) -> Result<TaskList> {
        let url = format!("{}/users/@me/lists", TASKS_API_BASE);
        let body = serde_json::json!({ "title": title });
        let resp: TaskList = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    pub async fn ensure_list_exists(&self, name: &str) -> Result<TaskList> {
        match self.find_list_by_name(name).await {
            Ok(list) => Ok(list),
            Err(MorrowError::ListNotFound(_)) => self.create_list(name).await,
            Err(e) => Err(e),
        }
    }
}
