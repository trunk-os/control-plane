use crate::{
	Client, Config, Input, InputType, PackageTitle, Prompt,
	PromptCollection, PromptResponse, PromptResponses, RegistryConfig,
	Server,
};
use std::path::PathBuf;
use tempfile::{NamedTempFile, tempdir};

async fn start_server(
	debug: bool, pool: Option<String>,
) -> (PathBuf, Option<PathBuf>, Option<(PathBuf, String, String)>) {
	let tf = NamedTempFile::new().unwrap();
	let (_, path) = tf.keep().unwrap();
	let pb = path.to_path_buf();
	let pb2 = pb.clone();

	let buckle_info = if let Some(pool) = pool {
		let (zpool, file) =
			buckle::testutil::create_zpool(&pool).unwrap();

		let buckle_socket = buckle::testutil::make_server(Some(
			buckle::config::Config {
				socket: "".into(), // ovewrites socket on create, not sure why
				zfs: buckle::config::ZFSConfig {
					pool: zpool.clone(),
				},
				log_level: buckle::config::LogLevel::Debug,
			},
		))
		.await
		.unwrap();
		Some((buckle_socket, zpool, file))
	} else {
		None
	};

	tokio::time::sleep(std::time::Duration::from_millis(500)).await;

	let systemd_root = if debug {
		Some(crate::SYSTEMD_SERVICE_ROOT.into())
	} else {
		let tf = tempdir().unwrap();
		Some(tf.keep())
	};

	let bi = buckle_info.clone();

	let inner = systemd_root.clone();
	tokio::spawn(async move {
		Server::new(Config {
			socket: pb2,
			log_level: None,
			debug: Some(debug),
			registry: RegistryConfig {
				path: "testdata/registry".into(),
				url: None,
			},
			systemd_root: inner,
			charon_path: Some(crate::DEFAULT_CHARON_BIN_PATH.into()),
			buckle_socket: bi
				.map(|x| x.0)
				.unwrap_or("/tmp/buckled.sock".into()),
		})
		.start()
		.unwrap()
		.await
		.unwrap()
	});

	tokio::time::sleep(std::time::Duration::from_millis(100)).await;

	(path, systemd_root, buckle_info)
}

#[tokio::test]
async fn test_ping() {
	let client =
		Client::new(start_server(true, None).await.0.to_path_buf())
			.unwrap();
	client.status().await.unwrap().ping().await.unwrap();
}

#[tokio::test]
async fn test_write_unit_real() {
	// real mode. validate written. this test also reloads systemd (which doesn't pick up anything
	// new because of the temporary path to write to) so it needs to be run as root.
	let (socket, systemd_path, buckle_info) =
		start_server(false, Some("write-unit-real".into())).await;
	let client = Client::new(socket).unwrap();

	assert!(
		client
			.control()
			.await
			.unwrap()
			.remove_unit("podman-test", "0.0.2")
			.await
			.is_err()
	);

	client
		.control()
		.await
		.unwrap()
		.write_unit("podman-test", "0.0.2", "/tmp/volroot".into())
		.await
		.unwrap();

	let systemd_path = systemd_path.unwrap();

	let content = std::fs::read_to_string(
		systemd_path.join("podman-test-0.0.2.service"),
	)
	.unwrap();
	assert_eq!(
		content,
		format!(
			r#"
[Unit]
Description=Charon launcher for podman-test, version 0.0.2

[Service]
ExecStart=/usr/bin/charon -r testdata/registry launch podman-test 0.0.2 /tmp/volroot
ExecStop=/usr/bin/charon -r testdata/registry stop podman-test 0.0.2 /tmp/volroot
Restart=always
TimeoutSec=300

[Install]
Alias=podman-test-0.0.2.service
"#
		)
	);

	assert!(
		client
			.control()
			.await
			.unwrap()
			.remove_unit("podman-test", "0.0.2")
			.await
			.is_ok()
	);

	assert!(
		!std::fs::exists(
			systemd_path.join("podman-test-0.0.2.service")
		)
		.unwrap()
	);
	if let Some(buckle_info) = buckle_info {
		let _ = buckle::testutil::destroy_zpool(
			"write-unit-real",
			Some(&buckle_info.2),
		);
	}
}

#[tokio::test]
async fn test_write_unit() {
	// debug mode
	let (socket, _, buckle_info) =
		start_server(true, Some("write-unit".into())).await;

	let client = Client::new(socket).unwrap();

	client
		.control()
		.await
		.unwrap()
		.write_unit("podman-test", "0.0.2", "/tmp/volroot".into())
		.await
		.unwrap();

	if let Some(buckle_info) = buckle_info {
		let _ = buckle::testutil::destroy_zpool(
			"write-unit",
			Some(&buckle_info.2),
		);
	}
}

#[tokio::test]
async fn test_get_prompts() {
	let client =
		Client::new(start_server(true, None).await.0.to_path_buf())
			.unwrap();
	let prompts = client
		.query()
		.await
		.unwrap()
		.get_prompts("podman-test", "0.0.2")
		.await
		.unwrap();

	assert!(prompts.0.is_empty());

	let prompts = client
		.query()
		.await
		.unwrap()
		.get_prompts("with-prompts", "0.0.1")
		.await
		.unwrap();

	assert!(!prompts.0.is_empty());

	let equal = PromptCollection(vec![
		Prompt {
			template: "private_path".into(),
			question: "Where do you want this mounted?".into(),
			input_type: InputType::String,
		},
		Prompt {
			template: "private_size".into(),
			question: "How big should it be?".into(),
			input_type: InputType::Integer,
		},
		Prompt {
			template: "private_recreate".into(),
			question:
				"Should we recreate this volume if it already exists?"
					.into(),
			input_type: InputType::Boolean,
		},
	]);

	assert_eq!(prompts, equal);
}

#[tokio::test]
async fn set_get_responses() {
	let responses = PromptResponses(vec![
		PromptResponse {
			input: Input::String("/tmp/volroot".into()),
			template: "private_path".into(),
		},
		PromptResponse {
			input: Input::Integer(8675309),
			template: "private_size".into(),
		},
		PromptResponse {
			input: Input::Boolean(false),
			template: "private_recreate".into(),
		},
	]);

	let client =
		Client::new(start_server(true, None).await.0.to_path_buf())
			.unwrap();
	client
		.query()
		.await
		.unwrap()
		.set_responses("with-prompts", responses.clone())
		.await
		.unwrap();

	let responses2 = client
		.query()
		.await
		.unwrap()
		.get_responses("with-prompts")
		.await
		.unwrap();

	assert_eq!(responses, responses2);
}

#[tokio::test]
async fn list() {
	// NOTE: this table must be updated anytime testdata's registry is.
	// Packages are sorted by name first, then in reverse order by version.
	let table = vec![
		("bad-dependencies", vec!["0.0.3", "0.0.2", "0.0.1"]),
		("bad-name-version", vec!["0.0.2", "0.0.1"]),
		("no-variables", vec!["0.0.1"]),
		("plex", vec!["0.0.2", "0.0.1"]),
		("plex-qemu", vec!["0.0.2", "0.0.1"]),
		("podman-test", vec!["0.0.3", "0.0.2", "0.0.1"]),
		("with-dependencies", vec!["0.0.1"]),
		("with-prompts", vec!["0.0.1"]),
	];

	let mut v = Vec::new();

	for (name, versions) in table {
		for version in versions {
			v.push(PackageTitle {
				name: name.into(),
				version: version.into(),
			})
		}
	}

	let client =
		Client::new(start_server(true, None).await.0.to_path_buf())
			.unwrap();

	let list = client.query().await.unwrap().list().await.unwrap();
	assert_eq!(list, v)
}

#[tokio::test]
async fn installer() {
	use crate::{InstallStatus, PackageTitle};

	let _ = buckle::testutil::destroy_zpool("test-installer", None);

	let client = Client::new(
		start_server(true, Some("test-installer".into()))
			.await
			.0
			.to_path_buf(),
	)
	.unwrap();
	client
		.control()
		.await
		.unwrap()
		.install("plex", "0.0.2")
		.await
		.unwrap();

	tokio::time::sleep(std::time::Duration::from_millis(500)).await;

	assert!(matches!(
		client
			.control()
			.await
			.unwrap()
			.installed("plex", "0.0.2")
			.await
			.unwrap()
			.unwrap(),
		InstallStatus::Installed(_),
	));

	assert_eq!(
		client
			.query()
			.await
			.unwrap()
			.list_installed()
			.await
			.unwrap(),
		vec![PackageTitle {
			name: "plex".into(),
			version: "0.0.2".into()
		}]
	);

	client
		.control()
		.await
		.unwrap()
		.uninstall("plex", "0.0.2")
		.await
		.unwrap();

	assert!(matches!(
		client
			.control()
			.await
			.unwrap()
			.installed("plex", "0.0.2")
			.await
			.unwrap()
			.unwrap(),
		InstallStatus::NotInstalled,
	));

	assert_eq!(
		client
			.query()
			.await
			.unwrap()
			.list_installed()
			.await
			.unwrap(),
		vec![]
	);

	let _ = buckle::testutil::destroy_zpool("test-installer", None);
}
