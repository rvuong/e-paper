//! Renders the phase 1 mockup states (`docs/mockups/phase1-*.svg`).
//!
//! ```text
//! cargo run --example mockups -- png [DIR]   write the PNGs (default: target/mockups)
//! cargo run --example mockups -- show NAME   open one state in the simulator
//! ```
//!
//! NAME is one of `phase1-r-home`, `phase1-all-away`, `phase1-activity-ring`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::{env, fs};

use e_paper::frame::Frame;
use e_paper::layout::{HEIGHT, WIDTH, layout};
use e_paper::mockups::phase1_states;
use e_paper::render::{atkinson_bold, render};
use embedded_graphics::pixelcolor::{BinaryColor, Rgb888};
use embedded_graphics::prelude::*;
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, Window,
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["png"] => write_pngs(PathBuf::from("target/mockups")),
        ["png", dir] => write_pngs(PathBuf::from(dir)),
        ["show", name] => show(name),
        _ => Err("usage: mockups png [DIR] | mockups show NAME".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn write_pngs(dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&dir)?;
    let font = atkinson_bold();
    for (name, state) in phase1_states() {
        let mut frame = Frame::new();
        render(&mut frame, &layout(&state), &font)?;
        let path = dir.join(format!("{name}.png"));
        fs::write(&path, frame.to_png())?;
        println!("{}", path.display());
    }
    Ok(())
}

fn show(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (_, state) = phase1_states()
        .into_iter()
        .find(|(candidate, _)| *candidate == name)
        .ok_or(format!("unknown mockup: {name}"))?;
    let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(WIDTH, HEIGHT));
    render(&mut display, &layout(&state), &atkinson_bold())?;
    let settings = OutputSettingsBuilder::new()
        .theme(BinaryColorTheme::Custom {
            color_off: Rgb888::WHITE,
            color_on: Rgb888::BLACK,
        })
        // Setting a theme defaults to ×3 pixels with a 1 px grid: one screen
        // pixel per panel pixel instead.
        .scale(1)
        .pixel_spacing(0)
        .build();
    Window::new(name, &settings).show_static(&display);
    Ok(())
}
