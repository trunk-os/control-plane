#![allow(unused)]
use std::{
	io::{Read, Write},
	path::PathBuf,
	process::ExitStatus,
};

use anyhow::{Result, anyhow};
use thiserror::Error;

const MIGRATION_FILENAME: &str = "vayama.state";
const DEFAULT_ROOT: &str = "/etc";

#[derive(Debug, Clone, Default, Error)]
pub enum MigrationError {
	#[default]
	#[error("Unknown error")]
	Unknown,
	#[error("Unknown error: {0}")]
	UnknownWithMessage(String),
	#[error("Validation failed: {0}")]
	ValidationFailed(String),
	#[error("Command failed [status: {0}]: {1:?}: output: {2:?}")]
	CommandFailed(ExitStatus, Vec<String>, Option<String>),
}

pub type MigrationFunc<'a> = Box<&'a dyn Future<Output = std::result::Result<(), MigrationError>>>;

#[derive(Debug, Clone, Default)]
pub struct Migrator<'a> {
	pub state_file: PathBuf,
	pub current_state: usize,
	pub migrations: Vec<Migration<'a>>,
}

impl<'a> Migrator<'a> {
	pub fn new(migrations: Vec<Migration<'a>>) -> Result<Migrator<'a>> {
		Self::new_with_root(migrations, PathBuf::from(DEFAULT_ROOT))
	}

	pub fn new_with_root(migrations: Vec<Migration<'a>>, root: PathBuf) -> Result<Migrator<'a>> {
		let state_file = root.join(MIGRATION_FILENAME);

		let current_state: usize = match std::fs::OpenOptions::new().read(true).open(&state_file) {
			Ok(mut f) => {
				let mut s = String::new();
				match f.read_to_string(&mut s) {
					Ok(_) => s.parse().unwrap_or_default(),
					Err(_) => 0,
				}
			}
			Err(_) => 0,
		};

		Ok(Self {
			state_file,
			current_state,
			migrations: migrations.clone(),
		})
	}

	pub fn more_migrations(&self) -> bool {
		self.current_state < self.migrations.len()
	}

	pub fn persist_state(&self) -> Result<()> {
		let mut f = std::fs::OpenOptions::new()
			.write(true)
			.create(true)
			.truncate(true)
			.open(&self.state_file)?;

		Ok(f.write_all(self.current_state.to_string().as_bytes())?)
	}

	pub async fn execute(&mut self) -> Result<Option<usize>> {
		if self.more_migrations() {
			return Err(anyhow!(
				"current state ({}) is greater than migrations length ({})",
				self.current_state,
				self.migrations.len()
			));
		}

		self.migrations[self.current_state]
			.execute()
			.await
			.map_err(|e| Into::<anyhow::Error>::into(e))?;

		self.persist_state()?;

		let orig = self.current_state;
		self.current_state += 1;

		Ok(Some(orig))
	}
}

#[derive(Clone)]
pub struct Migration<'a> {
	pub check: Option<MigrationFunc<'a>>,
	pub run: MigrationFunc<'a>,
	pub post_check: Option<MigrationFunc<'a>>,
}

impl<'a> std::fmt::Debug for Migration<'a> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("[vayama migration]")
	}
}

impl<'a> Migration<'a> {
	pub async fn execute(&self) -> std::result::Result<(), MigrationError> {
		Ok(())
	}
}
