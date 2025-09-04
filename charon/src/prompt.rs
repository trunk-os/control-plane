use std::path::PathBuf;

use crate::{Input, InputType, ProtoPromptResponse, ProtoType};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

pub const RESPONSES_SUBPATH: &str = "responses";
const DELIMITER: char = '?';

pub struct ResponseRegistry {
	pub root: PathBuf,
}

impl ResponseRegistry {
	pub fn new(root: PathBuf) -> Self {
		Self { root }
	}

	pub fn remove(&self, name: &str) -> Result<()> {
		Ok(std::fs::remove_file(
			self.root
				.join(RESPONSES_SUBPATH)
				.join(format!("{}.json", name)),
		)?)
	}

	pub fn get(&self, name: &str) -> Result<PromptResponses> {
		Ok(serde_json::from_reader(
			std::fs::OpenOptions::new().read(true).open(
				self.root
					.join(RESPONSES_SUBPATH)
					.join(format!("{}.json", name)),
			)?,
		)?)
	}

	pub fn set(
		&self, name: &str, responses: &PromptResponses,
	) -> Result<()> {
		let pb = self.root.join(RESPONSES_SUBPATH);

		std::fs::create_dir_all(&pb)?;
		let tmpname = pb.join(format!("{}.json.tmp", name));
		serde_json::to_writer_pretty(
			std::fs::OpenOptions::new()
				.create(true)
				.truncate(true)
				.write(true)
				.open(&tmpname)?,
			responses,
		)?;

		Ok(std::fs::rename(
			&tmpname,
			pb.join(format!("{}.json", name)),
		)?)
	}
}

#[derive(
	Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize,
)]
pub struct PromptResponses(pub Vec<PromptResponse>);

impl From<Vec<PromptResponse>> for PromptResponses {
	fn from(value: Vec<PromptResponse>) -> Self {
		Self(value)
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PromptParser(pub PromptCollection);

impl PromptParser {
	pub fn collection(&self) -> PromptCollection {
		self.0.clone()
	}

	pub fn prompts(&self, s: String) -> Result<Vec<Prompt>> {
		let mut v = Vec::new();
		let mut inside = false;
		let mut tmp = String::new();

		for ch in s.chars() {
			if inside && ch == DELIMITER {
				inside = false;
				if tmp.is_empty() {
					// ??, not a template
					continue;
				}

				for prompt in &self.collection().to_vec() {
					if prompt.template == tmp {
						v.push(prompt.clone())
					}
				}
				tmp = String::new();
			} else if ch == DELIMITER {
				inside = true
			} else if inside {
				tmp.push(ch)
			}
		}

		Ok(v)
	}

	pub fn template(
		&self, s: String, responses: &PromptResponses,
	) -> Result<String> {
		let mut tmp = String::new();
		let mut inside = false;
		let mut out = String::new();

		for ch in s.chars() {
			if inside && ch == DELIMITER {
				inside = false;
				if tmp.is_empty() {
					// ??, not a template
					out.push(DELIMITER);
					continue;
				}

				let mut matched = false;
				for response in &responses.0 {
					if response.template == tmp {
						out += &response.to_string();
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
	pub template: String,
	pub question: String,
	pub input_type: InputType,
}

#[derive(
	Debug, Clone, Eq, Default, PartialEq, Serialize, Deserialize,
)]
pub struct PromptCollection(pub Vec<Prompt>);

impl PromptCollection {
	pub fn to_vec(&self) -> Vec<Prompt> {
		self.0.clone()
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PromptResponse {
	pub template: String,
	pub input: Input,
}

impl std::fmt::Display for PromptResponse {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.input.to_string())
	}
}

impl From<PromptResponse> for ProtoPromptResponse {
	fn from(value: PromptResponse) -> Self {
		Self {
			template: value.template.clone(),
			response: value.input.to_string(),
			// tag the type separate of the input, probably the only way this is going to work
			input_type: match value.input {
				Input::Integer(_) => ProtoType::Integer,
				Input::SignedInteger(_) => ProtoType::SignedInteger,
				Input::Boolean(_) => ProtoType::Boolean,
				Input::String(_) => ProtoType::String,
			}
			.into(),
		}
	}
}

impl From<ProtoPromptResponse> for PromptResponse {
	fn from(value: ProtoPromptResponse) -> Self {
		Self {
			template: value.template.clone(),
			input: match value.input_type() {
				ProtoType::Integer => {
					Input::Integer(value.response.parse().unwrap())
				}
				ProtoType::SignedInteger => Input::SignedInteger(
					value.response.parse().unwrap(),
				),
				ProtoType::Boolean => {
					Input::Boolean(value.response.parse().unwrap())
				}
				ProtoType::String => Input::String(value.response),
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::PromptResponse;

	use super::{
		Input, InputType, Prompt, PromptCollection, PromptParser,
	};
	use lazy_static::lazy_static;

	lazy_static! {
		static ref PROMPTS: Vec<Prompt> = [
			Prompt {
				template: "greeting".into(),
				question: "how do we greet each other in computers?"
					.into(),
				input_type: InputType::String,
			},
			Prompt {
				template: "shoesize".into(),
				question: "what is your shoe size?".into(),
				input_type: InputType::Integer,
			},
			Prompt {
				template: "file".into(),
				question: "Give me the name of your favorite file"
					.into(),
				input_type: InputType::String,
			},
		]
		.to_vec();
	}

	#[test]
	fn prompt_responding() {
		let parser = PromptParser(PromptCollection(PROMPTS.clone()));
		assert!(
			parser
				.template("?greeting?".into(), &Default::default())
				.is_err()
		);
		assert!(
			parser
				.template(
					"?greeting?".into(),
					&(vec![PromptResponse {
						template: "not-greeting".into(),
						input: Input::Integer(20)
					}]
					.into()),
				)
				.is_err()
		);
		assert!(
			parser
				.template(
					"?greeting?".into(),
					&(vec![PromptResponse {
						template: "greeting".into(),
						input: Input::String("hello, world!".into())
					}]
					.into())
				)
				.is_ok()
		);

		assert!(
			parser
				.template(
					"?greeting?".into(),
					&(vec![
						PromptResponse {
							template: "greeting".into(),
							input: Input::String(
								"hello, world!".into()
							)
						},
						PromptResponse {
							template: "not-greeting".into(),
							input: Input::String(
								"hello, world!".into()
							)
						},
					]
					.into()),
				)
				.is_ok()
		);

		assert_eq!(
			parser
				.template(
					"?greeting?".into(),
					&(vec![PromptResponse {
						template: "greeting".into(),
						input: Input::String("hello, world!".into())
					}]
					.into())
				)
				.unwrap(),
			"hello, world!"
		);

		assert_eq!(
			parser
				.template(
					"?greeting? ?shoesize?".into(),
					&(vec![
						PromptResponse {
							template: "greeting".into(),
							input: Input::String(
								"hello, world!".into()
							)
						},
						PromptResponse {
							template: "shoesize".into(),
							input: Input::Integer(20),
						}
					]
					.into())
				)
				.unwrap(),
			"hello, world! 20"
		);

		assert!(
			parser
				.template("?greeting".into(), &Default::default())
				.is_ok()
		);
		assert_eq!(
			parser
				.template("?greeting".into(), &Default::default())
				.unwrap(),
			"?greeting"
		);
		assert!(
			parser.template("?".into(), &Default::default()).is_ok()
		);
		assert_eq!(
			parser.template("?".into(), &Default::default()).unwrap(),
			"?"
		);
		assert!(
			parser.template("??".into(), &Default::default()).is_ok()
		);
		assert_eq!(
			parser.template("??".into(), &Default::default()).unwrap(),
			"?"
		);
		assert_eq!(
			parser
				.template("why so serious?".into(), &Default::default())
				.unwrap(),
			"why so serious?"
		);
		assert_eq!(
			parser
				.template(
					"why so serious??".into(),
					&Default::default()
				)
				.unwrap(),
			"why so serious?"
		);
	}

	#[test]
	fn prompt_gathering() {
		let parser = PromptParser(PromptCollection(PROMPTS.clone()));

		assert_eq!(
			*parser
				.prompts("?greeting?".into())
				.unwrap()
				.iter()
				.next()
				.unwrap(),
			PROMPTS[0]
		);

		assert_eq!(
			*parser
				.prompts("also a ?greeting? woo".into())
				.unwrap()
				.iter()
				.next()
				.unwrap(),
			PROMPTS[0]
		);

		// items should appear in order
		assert_eq!(
            *parser
                .prompts("here are three items: ?file? and ?shoesize? and ?greeting? woo".into())
                .unwrap(),
            vec![PROMPTS[2].clone(), PROMPTS[1].clone(), PROMPTS[0].clone()]
        );

		assert_eq!(*parser.prompts("??".into()).unwrap(), vec![]);
		assert_eq!(*parser.prompts("?".into()).unwrap(), vec![]);
		assert_eq!(*parser.prompts("?test".into()).unwrap(), vec![]);
		assert_eq!(
			*parser.prompts("?file ?shoesize".into()).unwrap(),
			vec![]
		);
		assert_eq!(
			*parser.prompts("why so serious?".into()).unwrap(),
			vec![]
		);
	}

	#[test]
	fn input_conversion() {
		assert_eq!("20", Input::Integer(20).to_string());
		assert_eq!("-20", Input::SignedInteger(-20).to_string());
		assert_eq!(
			"hello, world!",
			Input::String("hello, world!".into()).to_string()
		);
	}
}
