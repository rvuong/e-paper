//! `e-paper`: household e-paper dashboard.

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use e_paper::devices;
use e_paper::freebox_client::Client;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// One-time authorization on the Freebox: confirm on its screen.
    Authorize {
        /// Where to store the app token (a secret).
        #[arg(long, default_value = "secrets/freebox_app_token")]
        token: PathBuf,
    },
    /// Writes the devices known to the Freebox into a draft mapping file.
    Devices {
        /// The app token written by `authorize`.
        #[arg(long, default_value = "secrets/freebox_app_token")]
        token: PathBuf,
        /// The draft mapping file to create (never overwritten).
        #[arg(long, default_value = "devices.toml")]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Command::Authorize { token } => authorize(&token),
        Command::Devices { token, output } => list_devices(&token, &output),
    }
}

fn authorize(token_path: &Path) -> Result<(), Box<dyn Error>> {
    if token_path.exists() {
        return Err(format!(
            "{} already exists: delete it to authorize again",
            token_path.display()
        )
        .into());
    }
    let client = Client::connect()?;
    let app_token = client.authorize(&device_name())?;
    save_secret(token_path, &app_token)?;
    println!("Authorized. App token saved in {}", token_path.display());
    Ok(())
}

fn list_devices(token_path: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let app_token = fs::read_to_string(token_path).map_err(|error| {
        format!(
            "cannot read {}: {error} (run `e-paper authorize` first)",
            token_path.display()
        )
    })?;
    let mut client = Client::connect()?;
    client.login(app_token.trim())?;
    let hosts = client.lan_hosts()?;
    devices::write_new(output, &devices::draft(&hosts)).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            format!("{} already exists: not overwritten", output.display())
        } else {
            format!("cannot write {}: {error}", output.display())
        }
    })?;
    println!("{} devices written to {}", hosts.len(), output.display());
    Ok(())
}

/// Name shown on the Freebox screen during the authorization.
fn device_name() -> String {
    fs::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_string())
        .unwrap_or_else(|_| "e-paper".into())
}

/// Writes a secret readable by its owner only; never overwrites.
fn save_secret(path: &Path, secret: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(secret.as_bytes())
}
