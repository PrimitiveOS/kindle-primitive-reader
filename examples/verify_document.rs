//! Developer entry point for comparing the same reading pipeline across release builds.
use reader::Result;
use std::{env, path::PathBuf};

fn verify() -> Result<()> {
    let mut args = env::args_os().skip(1).map(PathBuf::from);
    let usage =
        "verify_document <documents root> <book path> <new scratch directory> <font> [font...]";
    let root = args.next().ok_or(usage)?;
    let path = args.next().ok_or(usage)?;
    let scratch = args.next().ok_or(usage)?;
    let fonts: Vec<_> = args.collect();
    if fonts.is_empty() {
        return Err(usage.into());
    }
    reader::document_check::verify(
        &root,
        &path,
        &scratch,
        fonts,
        env::var_os("READER_VERIFY_PICKER").is_some(),
    )
}

fn main() {
    if let Err(error) = verify() {
        println!(
            "{}",
            serde_json::json!({"opened":false,"error":error.to_string()})
        );
        std::process::exit(1);
    }
}
