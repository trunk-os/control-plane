use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

const GLOBAL_SUBPATH: &str = "variables";
const DELIMITER: char = '@';

pub type Variables = HashMap<String, String>;

#[derive(
	Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize,
)]
pub struct Global {
	pub name: String,
	pub variables: Variables,
}

impl Global {
	pub fn var(&self, name: &str) -> Option<String> {
		self.variables.get(name).cloned()
	}

	pub fn template(&self, s: &str) -> Result<String> {
		let mut tmp = String::new();
		let mut inside = false;
		let mut out = String::new();

		for ch in s.chars() {
			if inside && ch == DELIMITER {
				inside = false;
				if tmp.is_empty() {
					// @@, not a template
					out.push(DELIMITER);
					continue;
				}

				let mut matched = false;
				for (key, value) in &self.variables {
					if key == &tmp {
						out += value.as_str();
						matched = true;
						break;
					}
				}

				if !matched {
					return Err(anyhow!(
						"No response matches prompt '{}'",
						tmp
					));
				}

				tmp = String::new();
			} else if ch == DELIMITER {
				inside = true
			} else if inside {
				tmp.push(ch)
			} else {
				out.push(ch)
			}
		}

		// if we were inside at the end of the string, don't swallow the ?
		if inside {
			out += &(DELIMITER.to_string() + &tmp);
		}

		Ok(out)
	}
}

impl Ord for Global {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.name.cmp(&other.name)
	}
}

impl PartialOrd for Global {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

pub struct GlobalRegistry {
	pub root: PathBuf,
}

impl GlobalRegistry {
	pub fn new(root: PathBuf) -> Self {
		Self { root }
	}

	pub fn remove(&self, name: &str) -> Result<()> {
		Ok(std::fs::remove_file(
			self.root
				.join(GLOBAL_SUBPATH)
				.join(format!("{}.json", name)),
		)?)
	}

	pub fn get(&self, name: &str) -> Result<Global> {
		Ok(serde_json::from_reader(
			std::fs::OpenOptions::new().read(true).open(
				self.root
					.join(GLOBAL_SUBPATH)
					.join(format!("{}.json", name)),
			)?,
		)?)
	}

	pub fn set(&self, global: &Global) -> Result<()> {
		let pb = self.root.join(GLOBAL_SUBPATH);

		std::fs::create_dir_all(&pb)?;
		let name = pb.join(format!("{}.json.tmp", global.name));
		serde_json::to_writer_pretty(
			std::fs::OpenOptions::new()
				.create(true)
				.truncate(true)
				.write(true)
				.open(&name)?,
			global,
		)?;

		Ok(std::fs::rename(
			name,
			pb.join(format!("{}.json", &global.name)),
		)?)
	}
}

#[cfg(test)]
mod tests {
	use super::{Global, GlobalRegistry, Variables};

	#[test]
	fn sort() {
		let mut table = vec![
			Global {
				name: "test".into(),
				variables: Default::default(),
			},
			Global {
				name: "first".into(),
				variables: Default::default(),
			},
		];

		table.sort();

		let mut i = table.iter();
		assert_eq!(i.next().unwrap().name, "first");
		assert_eq!(i.next().unwrap().name, "test");
	}

	#[test]
	fn io() {
		let dir = tempfile::tempdir().unwrap();
		let mut variables = Variables::default();
		variables.insert("foo".into(), "bar".into());
		variables.insert("baz".into(), "quux".into());

		let table = &[Global {
			name: "test".into(),
			variables: variables.clone(),
		}];

		let registry = GlobalRegistry {
			root: dir.path().into(),
		};

		for item in table {
			for (key, value) in &variables {
				assert_eq!(item.var(key).unwrap(), *value);
			}
			assert_eq!(item.var("unset"), None);
			assert!(registry.set(item).is_ok());
			assert_eq!(registry.get(&item.name).unwrap(), item.clone());
		}
	}

	#[test]
	fn template() {
		let mut variables = Variables::default();
		variables.insert("foo".into(), "bar".into());
		variables.insert("baz".into(), "quux".into());

		let global = Global {
			name: "test".into(),
			variables: variables.clone(),
		};

		assert!(global.template("@nonexistent@".into()).is_err());
		assert_eq!(global.template("@foo@".into(),).unwrap(), "bar");
		assert_eq!(
			global.template("@foo@ @baz@".into(),).unwrap(),
			"bar quux"
		);

		assert!(global.template("@foo".into()).is_ok());
		assert_eq!(global.template("@foo".into()).unwrap(), "@foo");
		assert!(global.template("@".into()).is_ok());
		assert_eq!(global.template("@".into()).unwrap(), "@");
		assert!(global.template("@@".into()).is_ok());
		assert_eq!(global.template("@@".into()).unwrap(), "@");
		assert_eq!(
			global.template("bgates@microsoft.com".into()).unwrap(),
			"bgates@microsoft.com"
		);
		assert_eq!(
			global.template("bgates@@microsoft.com".into()).unwrap(),
			"bgates@microsoft.com"
		);
	}
}
