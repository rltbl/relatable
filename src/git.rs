//! # rltbl/relatable
//!
//! This is relatable (rltbl::core).

use crate as rltbl;
use rltbl::core::RelatableError;

use anyhow::Result;
use chrono::NaiveDate;
use regex::Regex;
use std::process::Command;

#[derive(Clone, Debug, Default)]
pub struct GitStatus {
    pub raw_text: String,
    pub local: String,
    pub remote: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    /// Answers the question: "Are there any uncommitted changes to tracked files?"
    pub uncommitted: bool,
}

pub fn commit(message: &str, user: &str, is_amendment: bool) -> Result<()> {
    let user = format!("{user} <rltbl@localhost>");
    let mut args = vec!["commit", "--message", message, "--author", &user];
    if is_amendment {
        args.push("--amend");
    }

    let output = match Command::new("git").args(args).output() {
        Err(error) => {
            return Err(
                RelatableError::GitError(format!("Error running git commit: {error}")).into(),
            )
        }
        Ok(output) if !output.status.success() => {
            let error = std::str::from_utf8(&output.stderr)?;
            return Err(
                RelatableError::GitError(format!("Error running git commit: {error}")).into(),
            );
        }
        Ok(output) => output,
    };
    std::str::from_utf8(&output.stdout)?;
    Ok(())
}

pub fn add(paths: &Vec<String>) -> Result<()> {
    let mut args = vec!["add"];
    args.append(&mut paths.iter().map(|p| p.as_str()).collect::<Vec<_>>());
    let output = match Command::new("git").args(args).output() {
        Err(error) => {
            return Err(RelatableError::GitError(format!("Error running git add: {error}")).into())
        }
        Ok(output) if !output.status.success() => {
            let error = std::str::from_utf8(&output.stderr)?;
            return Err(RelatableError::GitError(format!("Error running git add: {error}")).into());
        }
        Ok(output) => output,
    };
    std::str::from_utf8(&output.stdout)?;
    Ok(())
}

pub fn get_last_commit_info() -> Result<(String, usize)> {
    let output = match Command::new("git")
        .args(["log", "-1", "--pretty=format:%as|%an"])
        .output()
    {
        Err(error) => {
            return Err(RelatableError::GitError(format!("Error getting git log: {error}")).into())
        }
        Ok(output) => output,
    };

    let status = output.status;
    if !status.success() {
        let error = std::str::from_utf8(&output.stderr)?;
        return Err(RelatableError::GitError(format!("Error getting git log: {error}")).into());
    }

    let commit_info = std::str::from_utf8(&output.stdout)?;
    let (commit_date, commit_author) = {
        let components = commit_info.splitn(2, '|').collect::<Vec<_>>();
        if components.len() != 2 {
            return Err(RelatableError::GitError("Error reading commit info".to_string()).into());
        }
        let commit_date = components[0];
        let commit_author = components[1];
        (commit_date, commit_author)
    };
    let commit_date = NaiveDate::parse_from_str(commit_date, "%Y-%m-%d").unwrap();
    let today = chrono::Local::now().date_naive();
    let days_ago = {
        let days_ago = today - commit_date;
        let days_ago = days_ago.num_days();
        if days_ago < 0 {
            return Err(RelatableError::GitError(format!("Last commit is in the future!")).into());
        }
        days_ago as usize
    };

    Ok((commit_author.to_string(), days_ago))
}

pub fn get_status() -> Result<GitStatus> {
    let output = match Command::new("git")
        .args(["status", "--short", "--branch", "--porcelain"])
        .output()
    {
        Err(error) => {
            return Err(
                RelatableError::GitError(format!("Error getting git status: {error}")).into(),
            )
        }
        Ok(output) => output,
    };

    let status = output.status;
    if !status.success() {
        let error = std::str::from_utf8(&output.stderr)?;
        return Err(RelatableError::GitError(format!("Error getting git status: {error}")).into());
    }

    let status_text = std::str::from_utf8(&output.stdout)?;
    let status_lines = status_text.lines().collect::<Vec<_>>();
    if status_lines.len() < 1 {
        return Err(
            RelatableError::GitError("Expected at least one line of output".to_string()).into(),
        );
    }

    let branch_status = status_lines[0];
    let file_statuses = {
        if status_lines.len() > 1 {
            status_lines[1..].iter().collect::<Vec<_>>()
        } else {
            vec![]
        }
    };

    let local_remote_re = r"(((\S+)\.{3}(\S+)|(\S+)))";
    let ahead_behind_re = r"( \[(ahead (\d+))?(, )?(behind (\d+))?\])?";
    let tracking_pattern = Regex::new(&format!(r"## {local_remote_re}{ahead_behind_re}")).unwrap();

    let captures = tracking_pattern
        .captures(&branch_status)
        .ok_or(RelatableError::GitError(format!(
            "Invalid status string: {branch_status}"
        )))?;

    Ok(GitStatus {
        raw_text: status_text.to_string(),
        local: {
            let local = captures.get(3).and_then(|c| Some(c.as_str()));
            let local_alt = captures.get(5).and_then(|c| Some(c.as_str()));
            match local {
                Some(local) => local.to_string(),
                None => match local_alt {
                    Some(local_alt) => local_alt.to_string(),
                    None => {
                        return Err(RelatableError::GitError(
                            "Could not determine LOCAL from git status output".to_string(),
                        )
                        .into())
                    }
                },
            }
        },
        remote: captures.get(4).and_then(|c| Some(c.as_str().to_string())),
        ahead: {
            let ahead = captures.get(8).and_then(|c| Some(c.as_str()));
            match ahead {
                None => 0,
                Some(n) => n.parse::<usize>()?,
            }
        },
        behind: {
            let behind = captures.get(11).and_then(|c| Some(c.as_str()));
            match behind {
                None => 0,
                Some(n) => n.parse::<usize>()?,
            }
        },
        uncommitted: file_statuses.iter().any(|s| !s.starts_with("??")),
    })
}
