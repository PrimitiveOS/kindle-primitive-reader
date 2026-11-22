use std::env;
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::Instant;

use reader::{Anchor, Engine, Probe, Result, Settings};

const USAGE: &str = "+Usage: reader-probe <blitz|parley> <book.epub> --font <font.ttf> [options]
  --font <path>          Add a reading font or fallback. Repeat as needed.
  --chapter <number>     Choose a spine item, starting at 1.
  --page <number>        Choose a page, starting at 1.
  --fragment <id>        Open at a chapter fragment.
  --anchor-in <path>     Restore a saved source position.
  --anchor-out <path>    Save the selected source position.
  --width <pixels>       Set the viewport width.
  --height <pixels>      Set the viewport height.
  --size <pixels>        Set the body font size.
  --margin <pixels>      Set the page margin.
  --line-height <ratio>  Set the line-height multiplier.
  --hyphenation <mode>   Choose automatic, authored, or off.
  --output <path>        Write a grayscale PNG.
  --turns <count>        Time additional rasterizations within the chapter.
";

fn main() {
    if let Err(error) = run() {
        eprintln!("Reader probe: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let first = args.next().ok_or(USAGE)?;
    if first == "--help" || first == "-h" {
        print!("{USAGE}");
        return Ok(());
    }
    #[cfg(feature = "kindle")]
    if first == "--opds-worker" && args.len() == 0 { return reader::opds::worker(); }
    let engine_name = first
        .into_string()
        .map_err(|_| "The engine name is invalid")?;
    let engine: Engine = engine_name.parse()?;
    let book = PathBuf::from(args.next().ok_or("Provide an EPUB file")?);
    let mut fonts = Vec::new();
    let mut settings = Settings::default();
    let mut chapter = 0;
    let mut page = 0;
    let mut fragment = None;
    let mut input = None;
    let mut output = None;
    let mut png = None;
    let mut turns: usize = 0;
    while let Some(arg) = args.next() {
        let name = arg.to_str().ok_or("The option name is invalid")?;
        let value = args.next().ok_or_else(|| format!("{name} needs a value"))?;
        let string = || value.to_str().ok_or("The option value is invalid");
        match name {
            "--font" => fonts.push(PathBuf::from(value)),
            "--chapter" => chapter = index(string()?)?,
            "--page" => page = index(string()?)?,
            "--fragment" => fragment = Some(string()?.to_owned()),
            "--anchor-in" => input = Some(PathBuf::from(value)),
            "--anchor-out" => output = Some(PathBuf::from(value)),
            "--width" => settings.width = string()?.parse()?,
            "--height" => settings.height = string()?.parse()?,
            "--size" => settings.font_size = string()?.parse()?,
            "--margin" => settings.margin = string()?.parse()?,
            "--line-height" => settings.line_height = string()?.parse()?,
            "--hyphenation" => {
                let mode = string()?;
                if !matches!(mode, "automatic" | "authored" | "off") {
                    return Err("Choose automatic, authored, or off for hyphenation".into());
                }
                settings.automatic_hyphenation = mode == "automatic";
                settings.disable_soft_hyphens = mode == "off";
            }
            "--output" => png = Some(PathBuf::from(value)),
            "--turns" => turns = string()?.parse()?,
            _ => return Err(format!("Unknown option {name}").into()),
        }
    }
    if input.is_some() && fragment.is_some() {
        return Err("Choose a fragment or a saved position".into());
    }
    if turns > 1000 {
        return Err("Choose at most 1000 rasterizations".into());
    }

    let start = Instant::now();
    let mut probe = Probe::open(&book, engine, &fonts, settings, chapter)?;
    let setup = start.elapsed();
    let anchor: Option<Anchor> = if let Some(input) = input {
        Some(serde_json::from_reader(fs::File::open(input)?)?)
    } else if let Some(id) = fragment {
        Some(
            probe
                .fragment(&id)
                .ok_or_else(|| format!("Fragment {id} is unavailable"))?,
        )
    } else {
        None
    };
    if let Some(anchor) = &anchor {
        page = probe
            .page_for(anchor)
            .ok_or("The saved position is unavailable in this layout")?;
    }
    let paint_start = Instant::now();
    let gray = probe.capture(page)?;
    let first_paint = paint_start.elapsed();
    let first_frame = start.elapsed();
    if !gray.iter().any(|&pixel| pixel < 240) {
        return Err("The selected page produced a blank frame".into());
    }
    if let Some(png) = png {
        save_frame(&png, settings, &gray)?;
    }
    if let Some(output) = output {
        let position = anchor
            .or_else(|| probe.page_anchor(page))
            .ok_or("The page has no text position")?;
        fs::write(output, serde_json::to_vec_pretty(&position)?)?;
    }
    let mut samples = Vec::with_capacity(turns);
    for i in 1..=turns {
        let next = (page + i) % probe.page_count();
        let time = Instant::now();
        std::hint::black_box(probe.capture(next)?);
        samples.push(time.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    let (covered, total) = probe.coverage();
    println!(
        "{}",
        serde_json::json!({
            "engine": engine_name,
            "chapter": probe.chapter,
            "chapter_count": probe.chapter_count(),
            "page": page + 1,
            "pages": probe.page_count(),
            "covered_characters": covered,
            "source_characters": total,
            "missing_glyphs": probe.missing_glyphs(),
            "layout_ms": setup.as_secs_f64() * 1000.0,
            "first_paint_ms": first_paint.as_secs_f64() * 1000.0,
            "first_frame_ms": first_frame.as_secs_f64() * 1000.0,
            "warm_rasterization_count": turns,
            "warm_rasterization_median_ms": percentile(&samples, 0.5),
            "warm_rasterization_p95_ms": percentile(&samples, 0.95),
            "peak_rss_kib": linux_peak_rss()
        })
    );
    Ok(())
}

fn index(value: &str) -> Result<usize> {
    Ok(value
        .parse::<usize>()?
        .checked_sub(1)
        .ok_or("Numbering starts at 1")?)
}

fn percentile(samples: &[f64], fraction: f64) -> Option<f64> {
    let index = (samples.len() as f64 * fraction).ceil() as usize;
    index.checked_sub(1).map(|index| samples[index])
}

fn linux_peak_rss() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn save_frame(path: &Path, settings: Settings, gray: &[u8]) -> Result<()> {
    let mut png = png::Encoder::new(
        BufWriter::new(fs::File::create(path)?),
        settings.width,
        settings.height,
    );
    png.set_color(png::ColorType::Grayscale);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()?.write_image_data(gray)?;
    Ok(())
}
