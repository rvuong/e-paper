//! Regression test: the render of each mockup state must match its golden image.
//!
//! The golden images in `tests/golden/` were approved visually against the
//! mockups. After an intended change of the render, review the new PNGs, then
//! regenerate them with:
//!
//! ```text
//! cargo run --example mockups -- png tests/golden
//! ```

use std::fs;
use std::path::Path;

use e_paper::frame::Frame;
use e_paper::layout::layout;
use e_paper::mockups::phase1_states;
use e_paper::render::{atkinson_bold, render};

#[test]
fn renders_match_the_golden_images() {
    let font = atkinson_bold();
    for (name, state) in phase1_states() {
        let path = Path::new("tests/golden").join(format!("{name}.png"));
        let golden = Frame::from_png(&fs::read(&path).unwrap()).unwrap();

        let mut frame = Frame::new();
        render(&mut frame, &layout(&state), &font).unwrap();

        assert!(
            frame == golden,
            "{name}: the render differs from {}",
            path.display()
        );
    }
}
