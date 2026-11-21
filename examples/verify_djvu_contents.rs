//! Print a DjVu outline for independent comparison with djvused.
use reader::{djvu::Djvu, fixed::RenderRequest};
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args_os().nth(1).ok_or("Supply a DjVu document")?;
    let doc = Djvu::open(fs::read(path)?)?;
    let count = doc.page_count();
    let entries: Vec<_> = doc
        .outline()
        .iter()
        .map(|row| (&row.title, row.depth, row.page))
        .collect();
    let mut sampled = Vec::new();
    for page in [0, count / 2, count - 1] {
        if sampled.last() == Some(&page) {
            continue;
        }
        let (width, height) = doc.page_size(page)?;
        let request = RenderRequest {
            width: 320,
            height: 480,
            scale: (320.0 / width).min(480.0 / height),
            x: 0.0,
            y: 0.0,
            turns: 0,
        };
        let image = doc.render(page, request)?;
        if image.len() != 320 * 480 || !image.iter().any(|pixel| *pixel < 180) {
            return Err("A sampled reference page had no visible ink".into());
        }
        sampled.push(page);
    }
    println!(
        "{}",
        serde_json::json!({"pages":count,"entries":entries,"sampled":sampled})
    );
    Ok(())
}
