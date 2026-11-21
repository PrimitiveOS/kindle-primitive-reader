// Modified by PrimitiveOS to reuse exact printer-color conversions on 32-bit ARM.
use super::ToRgb;
use super::icc::ICCProfile;
use std::sync::LazyLock;

#[cfg(any(feature = "cmyk-cache", test))]
mod repeated_colors;

#[derive(Debug, Clone)]
pub(crate) struct DeviceCmyk;

impl ToRgb for DeviceCmyk {
    fn convert(&self, input: &[u8], output: &mut [u8]) -> Option<()> {
        #[cfg(feature = "cmyk-cache")]
        if repeated_colors::is_candidate(input, output) {
            return repeated_colors::convert(input, output, |samples, rgb| {
                CMYK_TRANSFORM.convert(samples, rgb)
            });
        }
        CMYK_TRANSFORM.convert(input, output)
    }
}

#[cfg(feature = "cmyk-cache")]
pub(super) fn compact_image(data: &mut Vec<u8>) -> Option<()> {
    repeated_colors::compact(data, |samples, rgb| CMYK_TRANSFORM.convert(samples, rgb))
}

static CMYK_TRANSFORM: LazyLock<ICCProfile> = LazyLock::new(|| {
    ICCProfile::new(
        include_bytes!("../../assets/CGATS001Compat-v2-micro.icc"),
        4,
    )
    .unwrap()
});
