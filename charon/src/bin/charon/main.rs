use anyhow::Result;
use charon::{
	Client, Global, GlobalRegistry, PackageTitle, Registry,
	SourcePackage, SystemdUnit, generate_command, stop_package,
};
use clap::{Parser, Subcommand};
use fancy_duration::AsFancyDuration;
use std::path::PathBuf;

const DEFAULT_SOCKET_PATH: &str = "/tmp/charond.sock";

#[derive(Parser, Debug, Clone)]
#[command(version, about="CLI to the Charon Packaging System", long_about=None)]
struct MainArgs {
	#[arg(
		short = 'r',
		long = "registry",
		help = "Root path to package registry"
	)]
	registry_path: Option<PathBuf>,

	#[arg(
		short = 'b',
		long = "buckle",
		help = "Path to buckle socket"
	)]
	buckle_socket: Option<PathBuf>,

	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
	NewPackage(NewPackageArgs),
	RemovePackage(RemovePackageArgs),
	Launch(LaunchArgs),
	Stop(StopArgs),
	CreateUnit(CreateUnitArgs),
	Remote(RemoteArgs),
}

#[derive(Parser, Debug, Clone)]
#[command(about="Remote Control charond through GRPC socket", long_about=None)]
struct RemoteArgs {
	#[arg(
		short = 's',
		long = "socket",
		help = "Path to control socket"
	)]
	socket: Option<PathBuf>,
	#[command(subcommand)]
	command: RemoteCommands,
}

#[derive(Subcommand, Debug, Clone)]
enum RemoteCommands {
	Ping,
	WriteUnit(CreateUnitArgs),
}

#[derive(Parser, Debug, Clone)]
#[command(about="Create a systemd unit from a package", long_about=None)]
struct CreateUnitArgs {
	package_name: String,
	package_version: String,
	volume_root: PathBuf,
	#[arg(
        short = 's',
        long = "systemd-root",
        default_value = charon::SYSTEMD_SERVICE_ROOT,
        help = "path to systemd unit directory"
    )]
	systemd_root: Option<PathBuf>,
}

#[derive(Parser, Debug, Clone)]
#[command(about="Launch a package", long_about=None)]
struct LaunchArgs {
	package_name: String,
	package_version: String,
	volume_root: PathBuf,
}

#[derive(Parser, Debug, Clone)]
#[command(about="Stop a running package", long_about=None)]
struct StopArgs {
	package_name: String,
	package_version: String,
	volume_root: PathBuf,
}

#[derive(Parser, Debug, Clone)]
#[command(about="Create a new Package, creating the registry if necessary", long_about=None)]
struct NewPackageArgs {
	name: String,
	initial_version: String,
}

#[derive(Parser, Debug, Clone)]
#[command(about="Remove a package completely from the registry", long_about=None)]
struct RemovePackageArgs {
	name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
	let args = MainArgs::parse();
	let cwd = std::env::current_dir()?;
	match args.command {
		Commands::NewPackage(new_args) => {
			let r = Registry::new(
				args.registry_path.clone().unwrap_or(cwd.clone()),
			);
			let sp = SourcePackage {
				title: PackageTitle {
					name: new_args.name.clone(),
					version: new_args.initial_version,
				},
				description: "Please modify this description".into(),
				..Default::default()
			};
			r.write(&sp)?;
			let gr =
				GlobalRegistry::new(args.registry_path.unwrap_or(cwd));
			let g = Global {
				name: new_args.name,
				..Default::default()
			};
			gr.set(&g)?;
		}
		Commands::RemovePackage(rp_args) => {
			let r = Registry::new(
				args.registry_path.clone().unwrap_or(cwd.clone()),
			);
			let gr =
				GlobalRegistry::new(args.registry_path.unwrap_or(cwd));
			r.remove(&rp_args.name)?;
			gr.remove(&rp_args.name)?;
		}
		Commands::Launch(l_args) => {
			let r = Registry::new(
				args.registry_path.clone().unwrap_or(cwd.clone()),
			);
			let command = generate_command(
				r.load(&l_args.package_name, &l_args.package_version)?
					.compile()
					.await?,
				l_args.volume_root,
			)?;

			let status = std::process::Command::new(&command[0])
				.args(command.iter().skip(1))
				.status()?;
			std::process::exit(status.code().unwrap_or(1));
		}
		Commands::Stop(s_args) => {
			let r = Registry::new(
				args.registry_path.clone().unwrap_or(cwd.clone()),
			);
			stop_package(
				r.load(&s_args.package_name, &s_args.package_version)?
					.compile()
					.await?,
				s_args.volume_root,
			)?;
		}
		Commands::CreateUnit(cu_args) => {
			let r = Registry::new(
				args.registry_path.clone().unwrap_or(cwd.clone()),
			);
			let systemd = SystemdUnit::new(
				r.load(
					&cu_args.package_name,
					&cu_args.package_version,
				)?
				.compile()
				.await?,
				cu_args.systemd_root,
				std::env::current_exe().ok(),
			);

			systemd
				.create_unit(
					args.registry_path.unwrap_or(cwd.clone()),
					cu_args.volume_root,
				)
				.await?;

			println!(
				"Wrote unit to '{}'. Please reload systemd to take effect.",
				systemd.filename().display()
			);
		}
		Commands::Remote(r_args) => {
			let socket = r_args
				.socket
				.unwrap_or_else(|| DEFAULT_SOCKET_PATH.into());

			let client = Client::new(socket)?;
			match r_args.command {
				RemoteCommands::Ping => {
					let start = std::time::Instant::now();
					client.status().await?.ping().await?;
					eprintln!(
						"Ping successful! Took {}",
						(std::time::Instant::now() - start)
							.fancy_duration(),
					);
				}
				RemoteCommands::WriteUnit(wu_args) => {
					client
						.control()
						.await?
						.write_unit(
							&wu_args.package_name,
							&wu_args.package_version,
							wu_args.volume_root,
						)
						.await?;
					eprintln!(
						"Wrote unit for {}-{}",
						wu_args.package_name, wu_args.package_version,
					);
				}
			}
		}
	}

	Ok(())
}
