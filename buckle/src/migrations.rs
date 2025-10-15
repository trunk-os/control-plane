// please see the `vayama` library for tooling.

#![allow(unused_imports, dead_code)]
use anyhow::Result;
use std::{
	io::{Read, Write},
	path::{Path, PathBuf},
};
use vayama::{utils::*, *};

#[derive(Debug, Default)]
pub struct MigrationPlan {
	migrator: Migrator,
	root: PathBuf,
}

impl MigrationPlan {
	pub fn new(migrator: Migrator) -> Self {
		Self::new_with_root(migrator, None)
	}

	pub fn new_with_root(migrator: Migrator, root: Option<PathBuf>) -> Self {
		let root = root.unwrap_or(PathBuf::from("/"));
		Self { migrator, root }
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

		std::fs::rename(f.path(), p)?;
		Ok(())
	}

	pub async fn read_file<'a>(
		&self, target: impl Into<&'a Path> + AsRef<Path>,
	) -> Result<Vec<u8>> {
		let p = self.join_root(target);

		let mut f = std::fs::OpenOptions::new().read(true).open(p)?;
		let mut v = Vec::new();
		f.read_to_end(&mut v)?;

		Ok(v)
	}
}
