//! `e-paper`: household e-paper dashboard.

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use e_paper::cycle::{CycleInput, cycle};
use e_paper::devices;
use e_paper::display::{Display, PngDisplay, Update};
use e_paper::epd;
use e_paper::frame::Frame;
use e_paper::freebox_client::Client;
use e_paper::layout::{PEOPLE, layout};
use e_paper::mockups::phase1_states;
use e_paper::refresh::HouseholdState;
use e_paper::render::{atkinson_bold, render};
use e_paper::saved;
use e_paper::state::PersonState::{Away, HomeActive, HomePassive};
use e_paper::state::{ACTIVITY_THRESHOLD_BPS, PersonState};
use e_paper::update::{UpdateKind, changed_window};

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
        #[command(flatten)]
        output: Output,
        /// Presence window in seconds: a smartphone active this recently is
        /// home. Independent of the cycle period.
        #[arg(long, default_value_t = 300)]
        interval: i64,
        /// Full refresh even if nothing changed.
        #[arg(long)]
        force: bool,
        /// Never use partial refreshes.
        #[arg(long)]
        full_only: bool,
    },
    /// Shows the phase 1 mockup with an activity ring (legibility test).
    TestPattern {
        #[command(flatten)]
        output: Output,
    },
    /// Clears the e-paper panel to white.
    Clear,
    /// Checks partial refresh on the panel: one full refresh, then a few
    /// partial ones, 5 s apart.
    PartialTest {
        #[command(flatten)]
        output: Output,
    },
}

/// Where to show the display.
#[derive(clap::Args)]
struct Output {
    /// Draw on the e-paper panel (on the Raspberry Pi) instead of a PNG file.
    #[arg(long)]
    epd: bool,
    /// The PNG file, when not drawing on the panel.
    #[arg(long, default_value = "target/e-paper.png")]
    png: PathBuf,
}

impl Output {
    fn open(&self) -> Result<Box<dyn Display>, Box<dyn Error>> {
        if self.epd {
            Ok(Box::new(epd::linux::open()?))
        } else {
            Ok(Box::new(PngDisplay {
                path: self.png.clone(),
            }))
        }
    }

    fn describe(&self) -> String {
        if self.epd {
            "e-paper panel".into()
        } else {
            self.png.display().to_string()
        }
    }
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
            output,
            interval,
            force,
            full_only,
        } => run_cycle(
            &token, &devices, &state, &output, interval, force, !full_only,
        ),
        Command::TestPattern { output } => test_pattern(&output),
        Command::PartialTest { output } => partial_test(&output),
        Command::Clear => {
            epd::linux::open()?.clear()?;
            println!("Panel cleared");
            Ok(())
        }
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
    output_target: &Output,
    interval_secs: i64,
    force: bool,
    partial_enabled: bool,
) -> Result<(), Box<dyn Error>> {
    let mapping = read_mapping(devices_path)?;
    let app_token = read_token(token_path)?;
    let (previous, warning) = saved::load(state_path);
    if let Some(warning) = warning {
        eprintln!("warning: {warning}");
    }
    // The image on the display, kept next to the state: a partial refresh
    // sends it again, as the panel forgets it between runs.
    let frame_path = state_path.with_file_name("frame.png");
    let previous_frame = fs::read(&frame_path)
        .ok()
        .and_then(|bytes| Frame::from_png(&bytes).ok());

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
        previous_frame: previous_frame.is_some(),
        force,
        partial_enabled,
    });
    let states: Vec<String> = PEOPLE
        .iter()
        .map(|person| format!("{} {}", person.0, describe(output.household[person])))
        .collect();
    println!("{}", states.join(", "));

    let update = match (output.update, &previous_frame) {
        (UpdateKind::None, _) => None,
        (UpdateKind::Partial(window), Some(previous)) => Some(Update::Partial { previous, window }),
        _ => Some(Update::Full),
    };
    match update {
        None => println!("No change: display not touched"),
        Some(update) => {
            let mut frame = Frame::new();
            render(&mut frame, &layout(&output.household), &atkinson_bold())?;
            output_target.open()?.show(&frame, update)?;
            fs::write(&frame_path, frame.to_png())
                .map_err(|error| format!("cannot write {}: {error}", frame_path.display()))?;
            let kind = match update {
                Update::Full => "full",
                Update::Partial { .. } => "partial",
            };
            println!("Display redrawn ({kind}): {}", output_target.describe());
        }
    }
    saved::save(state_path, &output.saved)
        .map_err(|error| format!("cannot write {}: {error}", state_path.display()))?;
    Ok(())
}

fn test_pattern(output: &Output) -> Result<(), Box<dyn Error>> {
    let (_, state) = phase1_states()
        .into_iter()
        .find(|(name, _)| *name == "phase1-activity-ring")
        .expect("mockup exists");
    let mut frame = Frame::new();
    render(&mut frame, &layout(&state), &atkinson_bold())?;
    output.open()?.show(&frame, Update::Full)?;
    println!("Test pattern shown: {}", output.describe());
    Ok(())
}

fn partial_test(output: &Output) -> Result<(), Box<dyn Error>> {
    let household =
        |states: [PersonState; 4]| -> HouseholdState { PEOPLE.into_iter().zip(states).collect() };
    let steps = [
        ("R home", household([HomePassive, Away, Away, Away])),
        (
            "H home (active)",
            household([HomePassive, Away, HomeActive, Away]),
        ),
        (
            "R away, L home",
            household([Away, Away, HomeActive, HomePassive]),
        ),
        ("everybody away", household([Away; 4])),
    ];
    let font = atkinson_bold();
    let mut display = output.open()?;
    let mut shown = Frame::new();
    render(&mut shown, &layout(&household([Away; 4])), &font)?;
    display.show(&shown, Update::Full)?;
    println!("Full refresh: everybody away");
    let mut previous_state = household([Away; 4]);
    for (name, state) in steps {
        thread::sleep(Duration::from_secs(5));
        let window = changed_window(&previous_state, &state).expect("each step changes a slot");
        let mut frame = Frame::new();
        render(&mut frame, &layout(&state), &font)?;
        display.show(
            &frame,
            Update::Partial {
                previous: &shown,
                window,
            },
        )?;
        println!("Partial refresh: {name}");
        shown = frame;
        previous_state = state;
    }
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
