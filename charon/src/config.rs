use crate::{Registry, SYSTEMD_SERVICE_ROOT};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::FmtSubscriber;

const GIT_PATH: &str = "git"; // FIXME: for now. This should be absolute or configurable, at least.
const GIT_DEFAULT_REPOSITORY: &str = "https://github.com/trunk-os/charon-packages";
const REGISTRY_DEFAULT_PATH: &str = "/trunk/charon/registry";

pub const DEFAULT_CHARON_BIN_PATH: &str = "/usr/bin/charon";

#[derive(Debug, Clone, Deserialize, Default)]
pub enum LogLevel {
    #[serde(rename = "warn")]
    Warn,
    #[default]
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "debug")]
    Debug,
    #[serde(rename = "trace")]
    Trace,
}

impl From<LogLevel> for tracing::Level {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Trace => tracing::Level::TRACE,
        }
    }
}

impl From<tracing::Level> for LogLevel {
    fn from(value: tracing::Level) -> Self {
        match value {
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
            tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::TRACE => LogLevel::Trace,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryConfig {
    pub path: PathBuf,
    pub url: Option<String>,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            path: REGISTRY_DEFAULT_PATH.into(),
            url: Some(GIT_DEFAULT_REPOSITORY.into()),
        }
    }
}

fn default_systemd_root() -> Option<PathBuf> {
    Some(SYSTEMD_SERVICE_ROOT.into())
}

fn default_charon_path() -> Option<PathBuf> {
    Some(DEFAULT_CHARON_BIN_PATH.into())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    pub registry: RegistryConfig,
    pub socket: PathBuf,
    #[serde(default = "default_systemd_root")]
    pub systemd_root: Option<PathBuf>,
    pub log_level: Option<LogLevel>,
    pub debug: Option<bool>,
    #[serde(default = "default_charon_path")]
    pub charon_path: Option<PathBuf>,
    pub buckle_socket: PathBuf,
}

impl Config {
    pub fn from_file(filename: PathBuf) -> Result<Self> {
        let f = std::fs::OpenOptions::new().read(true).open(&filename)?;
        let this: Self = serde_yaml_ng::from_reader(&f)?;
        let subscriber = FmtSubscriber::builder()
            .with_max_level(Into::<tracing::Level>::into(
                this.log_level.clone().unwrap_or_default(),
            ))
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
        this.sync_registry()?;
        info!("Configuration parsed successfully.");
        Ok(this)
    }

    pub fn registry(&self) -> Registry {
        Registry::new(self.registry.path.clone())
    }

    pub fn debug(&self) -> bool {
        self.debug.unwrap_or_default()
    }

    pub fn sync_registry(&self) -> Result<()> {
        if let Some(url) = &self.registry.url {
            // exists. here, we want to store any files we have laying around so the rebase doesn't
            // fail. this is admittedly pretty dodgy but I don't have a better solution right now.
            if std::fs::exists(&self.registry.path)? {
                self.run_command(vec![GIT_PATH.into(), "add".into(), ".".into()])?;
                self.run_command(vec![GIT_PATH.into(), "stash".into()])?;
                self.run_command(vec![GIT_PATH.into(), "pull".into(), "--rebase".into()])?;
                // FIXME this sucks
                let _ = self.run_command(vec![GIT_PATH.into(), "stash".into(), "apply".into()]);
            } else {
                std::fs::create_dir_all(&self.registry.path)?;
                // first time, clone it
                self.run_command(vec![
                    GIT_PATH.into(),
                    "clone".into(),
                    url.clone(),
                    self.registry.path.to_string_lossy().to_string(),
                ])?;
            }
        }

        Ok(())
    }

    fn run_command(&self, command: Vec<String>) -> Result<()> {
        let mut iter = command.iter();
        if let Some(cmd) = iter.nth(0) {
            let status = std::process::Command::new(cmd)
                .args(iter.collect::<Vec<&String>>())
                .current_dir(&self.registry.path)
                .status()?;
            if !status.success() {
                return Err(anyhow!(
                    "command {:?} failed: exit status {}",
                    command.clone(),
                    status.code().unwrap_or(1)
                ));
            }
        } else {
            return Err(anyhow!("please specify a command"));
        }

        Ok(())
    }
}
