//! `e-paper`: household e-paper dashboard.

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use e_paper::cycle::{CycleInput, cycle};
use e_paper::devices;
use e_paper::frame::Frame;
use e_paper::freebox_client::Client;
use e_paper::layout::{PEOPLE, layout};
use e_paper::render::{atkinson_bold, render};
use e_paper::saved;
use e_paper::state::{ACTIVITY_THRESHOLD_BPS, PersonState};

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
    /// Checks the mapping file and prints a summary per person.
    CheckDevices {
        /// The mapping file.
        #[arg(long, default_value = "devices.toml")]
        devices: PathBuf,
    },
    /// Runs one cycle: polls the Freebox, computes the state, redraws if needed.
    Run {
        /// The app token written by `authorize`.
        #[arg(long, default_value = "secrets/freebox_app_token")]
        token: PathBuf,
        /// The mapping file.
        #[arg(long, default_value = "devices.toml")]
        devices: PathBuf,
        /// The state kept between runs.
        #[arg(long, default_value = "state.json")]
        state: PathBuf,
        /// Where to draw the display (until the e-paper driver exists).
        #[arg(long, default_value = "target/e-paper.png")]
        png: PathBuf,
        /// Presence window in seconds: the cycle period.
        #[arg(long, default_value_t = 900)]
        interval: i64,
        /// Redraw even if nothing changed.
        #[arg(long)]
        force: bool,
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
        Command::CheckDevices { devices } => check_devices(&devices),
        Command::Run {
            token,
            devices,
            state,
            png,
            interval,
            force,
        } => run_cycle(&token, &devices, &state, &png, interval, force),
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
    let app_token = read_token(token_path)?;
    let mut client = Client::connect()?;
    client.login(&app_token)?;
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

fn check_devices(path: &Path) -> Result<(), Box<dyn Error>> {
    let mapping = read_mapping(path)?;
    for line in devices::summary(&mapping) {
        println!("{line}");
    }
    Ok(())
}

fn run_cycle(
    token_path: &Path,
    devices_path: &Path,
    state_path: &Path,
    png_path: &Path,
    interval_secs: i64,
    force: bool,
) -> Result<(), Box<dyn Error>> {
    let mapping = read_mapping(devices_path)?;
    let app_token = read_token(token_path)?;
    let (previous, warning) = saved::load(state_path);
    if let Some(warning) = warning {
        eprintln!("warning: {warning}");
    }

    let mut client = Client::connect()?;
    client.login(&app_token)?;
    let hosts = client.lan_hosts()?;
    let counters = client.wifi_counters()?;
    let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;

    let output = cycle(&CycleInput {
        now,
        interval_secs,
        threshold_bps: ACTIVITY_THRESHOLD_BPS,
        mapping: &mapping,
        hosts: &hosts,
        counters: &counters,
        previous: previous.as_ref(),
        force,
    });
    let states: Vec<String> = PEOPLE
        .iter()
        .map(|person| format!("{} {}", person.0, describe(output.household[person])))
        .collect();
    println!("{}", states.join(", "));

    if output.refresh {
        let mut frame = Frame::new();
        render(&mut frame, &layout(&output.household), &atkinson_bold())?;
        fs::write(png_path, frame.to_png())
            .map_err(|error| format!("cannot write {}: {error}", png_path.display()))?;
        println!("Display redrawn: {}", png_path.display());
    } else {
        println!("No change: display not touched");
    }
    saved::save(state_path, &output.saved)
        .map_err(|error| format!("cannot write {}: {error}", state_path.display()))?;
    Ok(())
}

fn describe(state: PersonState) -> &'static str {
    match state {
        PersonState::Away => "away",
        PersonState::HomePassive => "home",
        PersonState::HomeActive => "home (active)",
    }
}

fn read_mapping(path: &Path) -> Result<devices::Mapping, Box<dyn Error>> {
    let text = fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read {}: {error} (run `e-paper devices` first)",
            path.display()
        )
    })?;
    Ok(devices::parse_mapping(&text)?)
}

fn read_token(path: &Path) -> Result<String, Box<dyn Error>> {
    let token = fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read {}: {error} (run `e-paper authorize` first)",
            path.display()
        )
    })?;
    Ok(token.trim().to_string())
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
