//! Measure Reader's existing renderers using the authored render-mode specimen.
use reader::{Anchor, Engine, Probe, Result, Settings};
use std::{env, fs, path::PathBuf};

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 1606;
const MAIN: &[&str] = &[
    "Before joined margins",
    "After joined margins",
    "Before negative margins",
    "After negative margins",
    "Before net negative gap",
    "After net negative gap",
    "Centered box marker",
    "Centered container marker",
    "Pixel box marker",
    "After pixel box marker",
    "Floating card marker",
    "Neighbor starts here",
    "Clearance marker",
    "Before inline cluster",
    "Inside inline cluster",
    "after inline cluster",
    "First table cell",
    "Second table cell",
    "End of specimen",
];
const BREAKS: &[&str] = &[
    "Before empty breaks",
    "After empty breaks",
    "After nonempty break",
];

fn longest_band(pixels: &[u8], tone: u8) -> (usize, usize) {
    let mut longest = (0, 0);
    for row in pixels.chunks_exact(WIDTH as usize) {
        let mut x0 = 0;
        let mut len = 0;
        for (x, &pixel) in row.iter().enumerate() {
            if pixel == tone {
                if len == 0 {
                    x0 = x;
                }
                len += 1;
                if len > longest.1 {
                    longest = (x0, len);
                }
            } else {
                len = 0;
            }
        }
    }
    longest
}

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1).map(PathBuf::from);
    let document = args.next().ok_or("Pass the authored EPUB")?;
    let font = args.next().ok_or("Pass Noto Sans Regular")?;
    let captures = args.next();
    let settings = Settings {
        width: WIDTH,
        height: HEIGHT,
        font_size: 40.0,
        line_height: 1.20,
        ..Settings::default()
    };
    for (name, engine) in [("Parley", Engine::Parley), ("Blitz", Engine::Blitz)] {
        for (section, phrases) in [MAIN, BREAKS].into_iter().enumerate() {
            let mut view = Probe::open(
                &document,
                engine,
                std::slice::from_ref(&font),
                settings,
                section,
            )?;
            println!(
                "ENGINE {name} section={} pages={}",
                section + 1,
                view.page_count()
            );
            for &phrase in phrases {
                let anchors = view.matches(phrase);
                // The shorter "Pixel box marker" also occurs inside the following marker.
                let Some(start) = anchors.first() else {
                    println!("MISSING {name}|{phrase}");
                    continue;
                };
                let end = Anchor {
                    offset: start.offset + phrase.len(),
                    ..start.clone()
                };
                let page = view.page_for(start);
                let rect =
                    page.zip(view.select_range(start, &end))
                        .and_then(|(page, selection)| {
                            view.selection_rects(page, &selection).into_iter().next()
                        });
                if let Some((page, rect)) = page.zip(rect) {
                    println!(
                        "MARK {name}|{phrase}|p={}|x={}|y={}",
                        page + 1,
                        rect.x,
                        rect.y
                    );
                } else {
                    println!("NOBOX {name}|{phrase}");
                }
            }
            if section == 0 {
                let frame = view.capture(0)?;
                for tone in [230, 200, 221] {
                    let (left, width) = longest_band(&frame, tone);
                    println!("BAND {name}|gray={tone}|x={left}|w={width}");
                }
                if let Some(folder) = &captures {
                    let mut pgm = format!("P5\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
                    pgm.extend(frame);
                    fs::write(folder.join(format!("{name}.pgm")), pgm)?;
                }
            }
        }
    }
    Ok(())
}
