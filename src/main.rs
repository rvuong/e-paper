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
use e_paper::board::{Block, Time, board_layout};
use e_paper::bus::{self, BusState};
use e_paper::cycle::{CycleInput, cycle};
use e_paper::departures::Direction;
use e_paper::devices;
use e_paper::diagnose;
use e_paper::display::{Display, PngDisplay, Update};
use e_paper::epd;
use e_paper::frame::Frame;
use e_paper::freebox;
use e_paper::freebox_client::Client;
use e_paper::layout::{PEOPLE, layout};
use e_paper::mockups::phase1_states;
use e_paper::prim::{self, Stop};
use e_paper::prim_client;
use e_paper::refresh::HouseholdState;
use e_paper::render::{atkinson_bold, render, render_board};
use e_paper::saved;
use e_paper::state::PersonState::{Away, HomeActive, HomePassive};
use e_paper::state::{ACTIVITY_THRESHOLD_BPS, PersonState};
use e_paper::update::{UpdateKind, changed_window, hourly_full_due, plan_display};
use e_paper::usage;
use jiff::Timestamp;

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
    /// Shows where each assigned device's download counter comes from (Wi-Fi
    /// access points, LAN browser). Never prints a MAC address.
    CheckCounters {
        /// The app token written by `authorize`.
        #[arg(long, default_value = "secrets/freebox_app_token")]
        token: PathBuf,
        /// The mapping file.
        #[arg(long, default_value = "devices.toml")]
        devices: PathBuf,
    },
    /// Runs one cycle: polls the Freebox (and PRIM on its own cadence),
    /// computes the state, redraws if needed.
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
        #[command(flatten)]
        prim: Prim,
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
        /// Appends each assigned device's download to this CSV file (dev
        /// feature, to check the activity threshold).
        #[arg(long)]
        usage_log: Option<PathBuf>,
    },
    /// Shows the phase 1 mockup with an activity ring (legibility test).
    TestPattern {
        #[command(flatten)]
        output: Output,
    },
    /// Logs the raw answers of the PRIM next departures API, one JSON line
    /// per stop and poll (observation before phase 2).
    BusLog {
        /// The PRIM API key (a secret).
        #[arg(long, default_value = "secrets/prim_api_key")]
        key: PathBuf,
        /// A stop to poll, as label=reference; repeat for each stop.
        #[arg(long = "stop", value_parser = prim::parse_stop, required = true)]
        stops: Vec<Stop>,
        /// Folder of the logs, one `<label>.jsonl` file per stop.
        #[arg(long, default_value = "bus-log")]
        dir: PathBuf,
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

/// The PRIM API, for the bus part.
#[derive(clap::Args)]
struct Prim {
    /// The PRIM API key (a secret).
    #[arg(long = "prim-key", default_value = "secrets/prim_api_key")]
    key: PathBuf,
    /// SIRI reference of the home pole towards Rungis, e.g.
    /// `STIF:StopPoint:Q:12345:`. Kept out of git: set it in the environment.
    #[arg(long, env = "PRIM_STOP_RUNGIS")]
    stop_rungis: String,
    /// SIRI reference of the home pole towards Antony RER.
    #[arg(long, env = "PRIM_STOP_ANTONY")]
    stop_antony: String,
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
        Command::CheckCounters { token, devices } => check_counters(&token, &devices),
        Command::Run {
            token,
            devices,
            state,
            output,
            prim,
            interval,
            force,
            full_only,
            usage_log,
        } => run_cycle(
            &token,
            &devices,
            &state,
            &output,
            &prim,
            interval,
            force,
            !full_only,
            usage_log.as_deref(),
        ),
        Command::TestPattern { output } => test_pattern(&output),
        Command::PartialTest { output } => partial_test(&output),
        Command::BusLog { key, stops, dir } => bus_log(&key, &stops, &dir),
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

fn check_counters(token_path: &Path, devices_path: &Path) -> Result<(), Box<dyn Error>> {
    let mapping = read_mapping(devices_path)?;
    let app_token = read_token(token_path)?;
    let mut client = Client::connect()?;
    client.login(&app_token)?;
    let hosts = client.lan_hosts()?;
    let counters = client.wifi_counters()?;
    for line in diagnose::counters_report(&mapping, &hosts, &counters) {
        println!("{line}");
    }
    let fields = diagnose::access_point_fields(&hosts);
    println!("access_point fields: {}", fields.join(", "));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_cycle(
    token_path: &Path,
    devices_path: &Path,
    state_path: &Path,
    output_target: &Output,
    prim: &Prim,
    interval_secs: i64,
    force: bool,
    partial_enabled: bool,
    usage_log: Option<&Path>,
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
    // Devices on the repeater only have the LAN browser's counter.
    let counters = freebox::download_counters(client.wifi_counters()?, &hosts);
    let now_at = Timestamp::now();
    let now = now_at.as_second();

    let input = CycleInput {
        now,
        interval_secs,
        threshold_bps: ACTIVITY_THRESHOLD_BPS,
        mapping: &mapping,
        hosts: &hosts,
        counters: &counters,
        previous: previous.as_ref(),
    };
    let output = cycle(&input);
    // The display matters more than the log: a write failure is only a warning.
    if let Some(path) = usage_log
        && let Err(error) = append_usage(path, &usage::usage_lines(&input, &output.household))
    {
        eprintln!("warning: cannot write {}: {error}", path.display());
    }
    let states: Vec<String> = PEOPLE
        .iter()
        .map(|person| format!("{} {}", person.0, describe(output.household[person])))
        .collect();
    println!("{}", states.join(", "));

    // PRIM is polled on its own cadence; a failed poll only shows as the
    // wrench, the presence part is still drawn.
    let previous_bus = previous.as_ref().and_then(|state| state.bus.as_ref());
    let bus = match previous_bus.filter(|bus| !bus::poll_due(now_at, Some(bus.polled_at))) {
        Some(bus) => bus.clone(),
        None => poll_prim(prim, previous_bus, now_at)?,
    };
    let board = bus::board(now_at, &bus);
    println!(
        "Rungis: {}; Antony: {}",
        describe_block(&board.rungis),
        describe_block(&board.antony)
    );

    let font = atkinson_bold();
    let mut frame = Frame::new();
    render_board(
        &mut frame,
        &board_layout(&board, &output.household, &font),
        &font,
    )?;
    let mut saved = output.saved;
    let full_required = force || hourly_full_due(now, saved.last_full_refresh);
    let plan = plan_display(
        previous_frame.as_ref(),
        &frame,
        full_required,
        partial_enabled,
    );
    let update = match (plan, &previous_frame) {
        (UpdateKind::None, _) => None,
        (UpdateKind::Partial(window), Some(previous)) => Some(Update::Partial { previous, window }),
        _ => Some(Update::Full),
    };
    match update {
        None => println!("No change: display not touched"),
        Some(update) => {
            output_target.open()?.show(&frame, update)?;
            fs::write(&frame_path, frame.to_png())
                .map_err(|error| format!("cannot write {}: {error}", frame_path.display()))?;
            let kind = match update {
                Update::Full => {
                    saved.last_full_refresh = Some(now);
                    "full"
                }
                Update::Partial { .. } => "partial",
            };
            println!("Display redrawn ({kind}): {}", output_target.describe());
        }
    }
    saved.bus = Some(bus);
    saved::save(state_path, &saved)
        .map_err(|error| format!("cannot write {}: {error}", state_path.display()))?;
    Ok(())
}

/// Polls both home poles. A failed poll is a warning, counted in the state.
fn poll_prim(
    prim: &Prim,
    previous: Option<&BusState>,
    now: Timestamp,
) -> Result<BusState, Box<dyn Error>> {
    let client = prim_client::Client::new(read_prim_key(&prim.key)?);
    let poll = |label: &str, stop: &str, direction| {
        let result = match client.stop_monitoring(stop) {
            Ok(answer) => bus::read_answer(answer.status, &answer.body, direction),
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = &result {
            eprintln!("warning: PRIM {label}: {error}");
        }
        result
    };
    let rungis = poll("rungis", &prim.stop_rungis, Direction::Rungis);
    let antony = poll("antony", &prim.stop_antony, Direction::Antony);
    Ok(bus::after_poll(previous, now, rungis, antony))
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

fn describe_block(block: &Block) -> String {
    match block {
        Block::Times(times) if times.is_empty() => "no departure".into(),
        Block::Times(times) => times
            .iter()
            .map(Time::to_string)
            .collect::<Vec<_>>()
            .join(" "),
        Block::Moon => "service over (moon)".into(),
        Block::Wrench => "bus data unavailable (wrench)".into(),
    }
}

fn describe(state: PersonState) -> &'static str {
    match state {
        PersonState::Away => "away",
        PersonState::HomePassive => "home",
        PersonState::HomeActive => "home (active)",
    }
}

/// Appends lines to the usage log, with the header first if the file is new
/// or empty.
fn append_usage(path: &Path, lines: &[String]) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut text = String::new();
    if file.metadata()?.len() == 0 {
        text.push_str(usage::HEADER);
        text.push('\n');
    }
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    file.write_all(text.as_bytes())
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

fn bus_log(key_path: &Path, stops: &[Stop], dir: &Path) -> Result<(), Box<dyn Error>> {
    let client = prim_client::Client::new(read_prim_key(key_path)?);
    fs::create_dir_all(dir)?;
    for stop in stops {
        let polled_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let line = match client.stop_monitoring(&stop.monitoring_ref) {
            Ok(answer) => {
                println!(
                    "{}: HTTP {}, {} bytes",
                    stop.label,
                    answer.status,
                    answer.body.len()
                );
                prim::log_line(polled_at, answer.status, &answer.body)
            }
            Err(error) => {
                eprintln!("{}: {error}", stop.label);
                prim::log_line(polled_at, 0, &error.to_string())
            }
        };
        let path = dir.join(format!("{}.jsonl", stop.label));
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{line}")?;
    }
    Ok(())
}

fn read_prim_key(path: &Path) -> Result<String, Box<dyn Error>> {
    let key = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(prim::api_key(&key)?)
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
