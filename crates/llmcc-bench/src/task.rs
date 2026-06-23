//! Task definition and TOML parsing.

use serde::Deserialize;
use std::fs;
use std::path::Path;

/// A single benchmark task.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub repo: String,
    pub description: String,
}

/// A task file containing multiple tasks.
#[derive(Debug, Deserialize)]
struct TaskFile {
    repo: String,
    tasks: Vec<TaskSpec>,
}

#[derive(Debug, Deserialize)]
struct TaskSpec {
    id: String,
    description: String,
}

/// Load tasks from a TOML file.
pub fn load(path: &Path) -> Vec<Task> {
    let content = fs::read_to_string(path).unwrap();
    let file: TaskFile = toml::from_str(&content).unwrap();
    file.tasks
        .into_iter()
        .map(|task| Task {
            id: task.id,
            repo: file.repo.clone(),
            description: task.description,
        })
        .collect()
}
