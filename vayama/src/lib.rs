#![allow(unused)]
use std::process::ExitStatus;

use thiserror::Error;
const MIGRATION_FILENAME: &str = "vayama.state";

#[derive(Debug, Clone, Default)]
pub struct Migrator<'a> {
	pub current_state: usize,
	pub migrations: Vec<Migration<'a>>,
}

#[derive(Debug, Clone, Default, Error)]
pub enum MigrationError {
	#[default]
	#[error("Unknown error")]
	Unknown,
	#[error("Validation failed: {0}")]
	ValidationFailed(String),
	#[error("Command failed [status: {0}]: {1:?}: output: {2:?}")]
	CommandFailed(ExitStatus, Vec<String>, Option<String>),
}

pub type MigrationFunc<'a> = Box<&'a dyn Future<Output = Result<(), MigrationError>>>;

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
