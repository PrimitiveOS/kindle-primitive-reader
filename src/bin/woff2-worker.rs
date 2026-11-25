//! Isolated WOFF 2 decoder. The parent sends bounded bytes over stdin.
//!
//! Linux resource limits are installed before input is read. A failed limit
//! setup is a hard failure; the Reader then uses the next CSS font source.
use std::io::{self, Read, Write};

const MAX_FONT: usize = 2 * 1024 * 1024;
const ADDRESS_SPACE: u64 = 64 * 1024 * 1024;
const FIXTURE_SHA256: [u8; 32] = [
    0xbf, 0x12, 0x95, 0x81, 0x6d, 0x80, 0x30, 0xcc, 0xce, 0x91, 0x0c, 0xa2, 0xf2, 0xce, 0x34, 0x67,
    0x0e, 0xd2, 0x1b, 0x01, 0x8b, 0x25, 0x87, 0x37, 0xd9, 0xfd, 0x6e, 0xea, 0xb3, 0x93, 0x3f, 0x86,
];

#[cfg(target_os = "linux")]
fn limit_resource(resource: libc::c_int, soft: u64, hard: u64) -> io::Result<()> {
    let mut current = std::mem::MaybeUninit::<libc::rlimit>::uninit();
    // SAFETY: current points to valid writable storage for getrlimit.
    if unsafe { libc::getrlimit(resource as _, current.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: getrlimit succeeded and initialized the value.
    let current = unsafe { current.assume_init() };
    let hard = (hard as libc::rlim_t).min(current.rlim_max);
    let next = libc::rlimit {
        rlim_cur: (soft as libc::rlim_t).min(hard).min(current.rlim_cur),
        rlim_max: hard,
    };
    // SAFETY: next is a valid rlimit and only reduces this process's limits.
    if unsafe { libc::setrlimit(resource as _, &next) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn configure_sandbox() -> io::Result<bool> {
    limit_resource(libc::RLIMIT_CORE as libc::c_int, 0, 0)?;
    limit_resource(libc::RLIMIT_AS as libc::c_int, ADDRESS_SPACE, ADDRESS_SPACE)?;
    limit_resource(libc::RLIMIT_CPU as libc::c_int, 2, 3)?;
    Ok(false)
}

#[cfg(not(target_os = "linux"))]
fn configure_sandbox() -> io::Result<bool> {
    // The known, benign fixture can be exercised in a debug-only simulator.
    // Arbitrary fonts never reach an unconfined decoder on a host that cannot
    // establish the Linux memory ceiling.
    if cfg!(debug_assertions)
        && std::env::args_os().nth(1).as_deref()
            == Some(std::ffi::OsStr::new("--host-fixture-test"))
    {
        return Ok(true);
    }
    Err(io::Error::other(
        "WOFF 2 worker requires Linux resource limits",
    ))
}

fn decode_fixture_or_font() -> Result<(), Box<dyn std::error::Error>> {
    let fixture_only = configure_sandbox()?;
    let mut input = io::stdin().lock();
    let mut header = [0u8; 12];
    input.read_exact(&mut header)?;
    if &header[..4] != b"W2H1" {
        return Err("invalid protocol".into());
    }
    let limit = u32::from_be_bytes(header[4..8].try_into()?) as usize;
    let size = u32::from_be_bytes(header[8..12].try_into()?) as usize;
    if limit == 0 || limit > MAX_FONT || size == 0 || size > MAX_FONT {
        return Err("font exceeds limits".into());
    }
    let mut data = vec![0; size];
    input.read_exact(&mut data)?;
    let mut extra = [0u8; 1];
    if input.read(&mut extra)? != 0 || !data.starts_with(b"wOF2") {
        return Err("invalid input".into());
    }
    if fixture_only && ring::digest::digest(&ring::digest::SHA256, &data).as_ref() != FIXTURE_SHA256
    {
        return Err("unapproved host fixture".into());
    }
    let mut brotli = |compressed: &[u8], expected: usize| {
        if expected > MAX_FONT {
            return Err::<Vec<u8>, Box<dyn std::error::Error>>("font stream exceeds limit".into());
        }
        use brotli_decompressor::{
            BrotliDecompressStream, BrotliResult, BrotliState, StandardAlloc,
        };
        let mut result = vec![0u8; expected + 1];
        let mut available_in = compressed.len();
        let mut input_offset = 0;
        let mut available_out = result.len();
        let mut output_offset = 0;
        let mut total_out = 0;
        let mut state = BrotliState::new_strict(
            StandardAlloc::default(),
            StandardAlloc::default(),
            StandardAlloc::default(),
        );
        let status = BrotliDecompressStream(
            &mut available_in,
            &mut input_offset,
            compressed,
            &mut available_out,
            &mut output_offset,
            &mut result,
            &mut total_out,
            &mut state,
        );
        let trailing = &compressed[input_offset..];
        if !matches!(status, BrotliResult::ResultSuccess)
            || output_offset != expected
            || trailing.len() > 3
            || trailing.iter().any(|byte| *byte != 0)
        {
            return Err("invalid font stream".into());
        }
        result.truncate(expected);
        Ok(result)
    };
    let result = wuff::decompress_woff2_with_custom_brotli(&data, &mut brotli)?;
    if result.len() > limit || !(result.starts_with(b"\0\x01\0\0") || result.starts_with(b"OTTO")) {
        return Err("font output exceeds limit".into());
    }
    io::stdout().lock().write_all(&result)?;
    Ok(())
}

fn main() {
    if decode_fixture_or_font().is_err() {
        std::process::exit(1);
    }
}
