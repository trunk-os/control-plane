// please see the `vayama` library for tooling.
#![allow(dead_code)]
use anyhow::Result;
use std::{
	io::{Read, Write},
	path::{Path, PathBuf},
};
use vayama::Migrator;

pub(crate) mod plan;

#[derive(Debug, Default)]
pub struct MigrationPlan {
	migrator: Migrator,
	root: PathBuf,
	zpool: String,
}

impl MigrationPlan {
	pub fn new(migrator: Migrator, root: Option<PathBuf>, zpool: Option<String>) -> Self {
		let root = root.unwrap_or(PathBuf::from("/"));
		// FIXME: defaults like this should be a constant somewhere
		let zpool = zpool.unwrap_or("trunk".into());

		Self {
			migrator,
			root,
			zpool,
		}
	}

	pub async fn execute(&mut self) -> Result<usize> {
		self.migrator.execute_failed().await?;

		let mut latest_migration = 0;

		while let Ok(res) = self.migrator.execute().await {
			if let Some(latest) = res {
				latest_migration = latest;
			}
		}

		Ok(latest_migration)
	}

	pub fn join_root<'a>(&self, target: impl Into<&'a Path> + AsRef<Path>) -> PathBuf {
		self.root.join(target)
	}

	pub fn write_file<'a>(
		&self, target: impl Into<&'a Path> + AsRef<Path>, out: &[u8],
	) -> Result<()> {
		let p = self.join_root(target);

		// semi-atomic write
		let mut f = tempfile::NamedTempFile::new()?;
		f.write_all(out)?;

		Ok(std::fs::rename(f.path(), p)?)
	}

	pub fn read_file<'a>(&self, target: impl Into<&'a Path> + AsRef<Path>) -> Result<Vec<u8>> {
		let p = self.join_root(target);

		let mut f = std::fs::OpenOptions::new().read(true).open(p)?;
		let mut v = Vec::new();
		f.read_to_end(&mut v)?;

		Ok(v)
	}

	pub fn exists_file<'a>(&self, target: impl Into<&'a Path> + AsRef<Path>) -> Result<bool> {
		Ok(std::fs::exists(self.join_root(target))?)
	}
}
