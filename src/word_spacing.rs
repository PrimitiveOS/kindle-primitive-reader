//! Font-relative space width and minimum remaining width during line fitting.
//!
//! Missing percent policies deliberately retain the older additive pixel setting.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordSpacePercent {
    pub scaling: u16,
    /// Minimum width as a percentage of the already scaled space. 100 disables shrink.
    pub minimum: u8,
}

impl WordSpacePercent {
    pub const SMALL: Self = Self {
        scaling: 75,
        minimum: 50,
    };
    pub const MEDIUM: Self = Self {
        scaling: 95,
        minimum: 75,
    };
    pub const LARGE: Self = Self {
        scaling: 100,
        minimum: 90,
    };

    pub const fn valid(self) -> bool {
        self.scaling >= 10 && self.scaling <= 500 && self.minimum >= 25 && self.minimum <= 100
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_pair_bounds_and_schema_are_strict() {
        for scaling in 10..=500 {
            for minimum in 25..=100 {
                assert!(WordSpacePercent { scaling, minimum }.valid());
            }
        }
        for pair in [
            WordSpacePercent {
                scaling: 9,
                minimum: 75,
            },
            WordSpacePercent {
                scaling: 501,
                minimum: 75,
            },
            WordSpacePercent {
                scaling: 95,
                minimum: 24,
            },
            WordSpacePercent {
                scaling: 95,
                minimum: 101,
            },
        ] {
            assert!(!pair.valid());
        }
        assert!(
            serde_json::from_str::<WordSpacePercent>(r#"{"scaling":95,"minimum":75,"unknown":1}"#)
                .is_err()
        );
    }
}
