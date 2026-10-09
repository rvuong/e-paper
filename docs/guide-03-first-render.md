# Guide 03: first render (phase 1)

Exceptionally implemented by Claude at Rémy's request (2026-10-09). This page
records what was built and how to check it.

## What it does

The household state becomes an 800×480 black and white image, in three layers:

| Module | Role | Tests |
|---|---|---|
| `layout` | Pure: the household state gives one `Slot` per person (centre, core circle, rings, letter). No pixels. | 6 |
| `render` | Draws the slots on any `embedded-graphics` target: circles with `embedded-graphics`, the letter rasterised by `fontdue` and thresholded at 50 % coverage. | 7 |
| `frame` | `Frame`: an in-memory 800×480 1-bit image (`DrawTarget`), with PNG export and import. | 1 |

The font (`assets/fonts/AtkinsonHyperlegible-Bold.ttf`, OFL licence in
`assets/fonts/OFL.txt`) is embedded in the binary with `include_bytes!`.

## Approved tests, as implemented

1. Slots in the order R, M, H, L, centred at x = 109, 303, 497, 691, y = 240.
2. `Away`: 130 px circle, 12 px stroke, not filled; black 88 px letter; no ring.
3. `HomePassive`: filled circle, white letter, no ring.
4. `HomeActive`: plus two rings of 154 and 178 px, 6 px stroke.
5. A person missing from the state is drawn `Away`.
6. Every shape fits on the display (outer rings from x = 20 to x = 780).
7. Letter origin with the mockups' rule (centred on the advance width, line box
   as high as the font size), checked against values computed from the font
   tables: x = 80.224, baseline = 269.04 for R.
8. Outline black mid-stroke (59 px from the centre), inside white (45 px).
9. Filled inside for a person at home.
10. Rings black at 74 and 86 px, white gaps at 68 and 80 px.
11. **Added:** no ring pixels without activity (complements test 10).
12. Letter ink present inside the circle, its bounding box centred within 10 px.
13. White background (corners and margins).
14. PNG round trip: same size, same pixels.

## Finding: circle centres

`embedded-graphics`' `Circle::with_center` puts the centre **on** a pixel; the
mockups put it **between** pixels (x = 109 is the boundary between columns 108
and 109). With even diameters, the circles were shifted by one pixel right and
down. `render` now places circles from their top-left corner
(`center − diameter / 2`).

## Comparison with the mockups

The SVG mockups were rasterised with `resvg` (throwaway tool, outside the
project) and compared pixel by pixel with the PNG renders: about 1 % of the
black pixels differ, all on shape edges (isolated pixels, plus one pixel column
on a stem of the M). These come from the two rasterisers' rounding; nothing is
off by more than one pixel.

## Check it yourself

Render the three mockup states as PNG (in `target/mockups/`):

```bash
cargo run --example mockups -- png
```

Open one state in the simulator window (close the window to quit):

```bash
cargo run --example mockups -- show phase1-activity-ring
```

The other names are `phase1-r-home` and `phase1-all-away`. Compare with
`docs/mockups/phase1-*.svg`.

The simulator is a dev-dependency (`embedded-graphics-simulator`, needs SDL2):
it is never built for the Pi.

## Golden images

Rémy approved the render in the simulator (2026-10-09). The three PNGs are
stored in `tests/golden/`, and `tests/golden.rs` compares each render with its
golden image pixel by pixel (checked: swapping two golden images makes it
fail). The mockup states live in `src/mockups.rs`, shared by the example and
the test.

After an intended change of the render, review the new PNGs, then regenerate:

```bash
cargo run --example mockups -- png tests/golden
```
