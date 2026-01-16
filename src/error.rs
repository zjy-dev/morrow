use thiserror::Error;

#[derive(Error, Debug)]
pub enum MorrowError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("LLM API error: {0}")]
    Llm(String),

    #[error("Output list has incomplete tasks. Please complete or clear them before planning.")]
    OutputListNotEmpty,

    #[error("Task list not found: {0}")]
    ListNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

pub type Result<T> = std::result::Result<T, MorrowError>;
