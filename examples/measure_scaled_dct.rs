//! Compare the same fixed-page viewport with and without reduced JPEG decoding.
use reader::fixed::{Pdf, RenderRequest};
use std::{env, fs, path::Path, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let usage = "measure_scaled_dct <PDF> <one-based page> <zoom factor> <PNG>";
    let mut args = env::args_os().skip(1);
    let input = args.next().ok_or(usage)?;
    let page = args
        .next()
        .and_then(|s| s.to_str()?.parse::<usize>().ok())
        .and_then(|number| number.checked_sub(1))
        .ok_or(usage)?;
    let zoom = args
        .next()
        .and_then(|s| s.to_str()?.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 8.0)
        .ok_or(usage)?;
    let output = args.next().ok_or(usage)?;
    if args.next().is_some() {
        return Err(usage.into());
    }

    let pdf = Pdf::open(fs::read(input)?)?;
    let (page_width, page_height) = pdf.page_size(page)?;
    let (width, height) = (1264, 1606);
    let scale = (width as f32 / page_width).min(height as f32 / page_height) * zoom;
    let request = RenderRequest {
        width,
        height,
        scale,
        x: (width as f32 - page_width * scale) * 0.5,
        y: (height as f32 - page_height * scale) * 0.5,
        turns: 0,
    };
    let start = Instant::now();
    let pixels = pdf.render(page, request)?;
    let raster_ms = start.elapsed().as_millis();
    let peak = fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmHWM:")
                    .map(|number| number.trim().to_owned())
            })
        })
        .unwrap_or_default();
    primitive_app::write_png(Path::new(&output), width, height, &pixels)?;
    println!(
        "raster_ms={raster_ms} peak={peak} size={page_width:.3}x{page_height:.3} transform={scale:.6},{:.6},{:.6}",
        request.x, request.y
    );
    Ok(())
}
