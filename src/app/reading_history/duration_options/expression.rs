//! Bounded arithmetic for the page-duration keypad.

/// Evaluate an expression only when it resolves to a representable whole
/// second. Statistics currently persists integer-second limits, so accepting
/// a fractional result here would silently change the user's setting.
pub(super) fn whole_seconds(input: &str) -> Option<u32> {
    if input.is_empty() || input.len() > 40 || !input.is_ascii() {
        return None;
    }
    let mut parser = Parser {
        input: input.as_bytes(),
        at: 0,
        depth: 0,
    };
    let value = parser.sum()?;
    parser.skip_space();
    if parser.at != parser.input.len() || !value.is_finite() || value < 0.0 {
        return None;
    }
    let rounded = value.round();
    if (value - rounded).abs() > 1e-9 || rounded > f64::from(u32::MAX) {
        return None;
    }
    Some(rounded as u32)
}

struct Parser<'a> {
    input: &'a [u8],
    at: usize,
    depth: u8,
}

impl Parser<'_> {
    fn skip_space(&mut self) {
        while self.input.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn take(&mut self, expected: u8) -> bool {
        self.skip_space();
        if self.input.get(self.at) == Some(&expected) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn sum(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        loop {
            if self.take(b'+') {
                value += self.product()?;
            } else if self.take(b'-') {
                value -= self.product()?;
            } else {
                return value.is_finite().then_some(value);
            }
        }
    }

    fn product(&mut self) -> Option<f64> {
        let mut value = self.factor()?;
        loop {
            if self.take(b'*') {
                value *= self.factor()?;
            } else if self.take(b'/') {
                let divisor = self.factor()?;
                if divisor == 0.0 {
                    return None;
                }
                value /= divisor;
            } else {
                return value.is_finite().then_some(value);
            }
        }
    }

    fn factor(&mut self) -> Option<f64> {
        self.skip_space();
        if self.depth >= 12 {
            return None;
        }
        if self.take(b'+') {
            self.depth += 1;
            let value = self.factor();
            self.depth -= 1;
            return value;
        }
        if self.take(b'-') {
            self.depth += 1;
            let value = self.factor().map(|value| -value);
            self.depth -= 1;
            return value;
        }
        if self.take(b'(') {
            self.depth += 1;
            let value = self.sum();
            self.depth -= 1;
            let value = value?;
            return self.take(b')').then_some(value);
        }
        let start = self.at;
        while self
            .input
            .get(self.at)
            .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
        {
            self.at += 1;
        }
        std::str::from_utf8(&self.input[start..self.at])
            .ok()?
            .parse()
            .ok()
    }
}
