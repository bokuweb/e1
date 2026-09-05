//! Where this app keeps what little it keeps.
//!
//! `~/.e1/` holds the window's settings and its logs, and nothing else: the
//! token is discovered, not stored (`AGENTS.md` rule 8), and GitHub's data is
//! cached in memory only. `E1_HOME` moves the directory, which is how a test
//! or a second profile keeps out of the real one.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The root and the files under it.
#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    /// `E1_HOME`, or `~/.e1`.
    pub fn from_env() -> Result<Self> {
        if let Some(root) = std::env::var_os("E1_HOME") {
            return Ok(Self::with_root(root));
        }
        let home = dirs::home_dir().context("no home directory")?;
        Ok(Self::with_root(home.join(".e1")))
    }

    /// Rooted somewhere specific.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The window's settings: `app.json`.
    pub fn app_settings(&self) -> PathBuf {
        self.root.join("app.json")
    }

    /// Daily-rotated logs.
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Create the directories, which is safe to repeat.
    pub fn ensure(&self) -> Result<()> {
        std::fs::create_dir_all(self.logs())
            .with_context(|| format!("creating {}", self.logs().display()))
    }
}
