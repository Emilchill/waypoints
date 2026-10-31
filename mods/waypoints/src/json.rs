//! Small JSON writers. A JSON library costs far more wasm fuel than a callback allows.

pub fn push_u(out: &mut String, n: u64) {
    if n < 10 {
        out.push((b'0' + n as u8) as char);
        return;
    }
    if n <= u64::from(u32::MAX) {
        return push_u32(out, n as u32, false);
    }
    let mut buf = [0u8; 20];
    let mut index = buf.len();
    let mut n = n;
    while n > 0 {
        index -= 1;
        buf[index] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    push_digits(out, &buf[index..]);
}

/// Appends ASCII digits (and a sign) without re-validating them as UTF-8.
fn push_digits(out: &mut String, digits: &[u8]) {
    // SAFETY: callers only pass ASCII digits and '-', which are valid UTF-8.
    out.push_str(unsafe { std::str::from_utf8_unchecked(digits) });
}

/// 32-bit formatting: noticeably cheaper in wasm fuel than 64-bit division.
pub fn push_u32(out: &mut String, mut n: u32, negative: bool) {
    let mut buf = [0u8; 11];
    let mut index = buf.len();
    loop {
        index -= 1;
        buf[index] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if negative {
        index -= 1;
        buf[index] = b'-';
    }
    push_digits(out, &buf[index..]);
}

/// A non-negative number with up to three decimals.
pub fn num(out: &mut String, value: f64) {
    let x = if value.is_finite() && value > 0.0 { value } else { 0.0 };
    let scaled = (x * 1000.0 + 0.5) as u64;
    push_u(out, scaled / 1000);
    let mut fraction = scaled % 1000;
    if fraction != 0 {
        out.push('.');
        let mut divisor = 100;
        while fraction != 0 {
            out.push((b'0' + (fraction / divisor) as u8) as char);
            fraction %= divisor;
            divisor /= 10;
        }
    }
}

/// A quoted string; callers only pass labels and key names, which need no escaping.
pub fn q(out: &mut String, text: &str) {
    out.push('"');
    out.push_str(text);
    out.push('"');
}

pub fn rgba(out: &mut String, color: [f32; 4]) {
    out.push('[');
    for (index, channel) in color.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        num(out, f64::from(*channel));
    }
    out.push(']');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_colours_are_plain_json() {
        let mut out = String::new();
        num(&mut out, 0.5);
        out.push(' ');
        num(&mut out, 12.0);
        out.push(' ');
        rgba(&mut out, [1.0, 0.0, 0.33, 0.5]);
        assert_eq!(out, "0.5 12 [1,0,0.33,0.5]");
        let mut out = String::new();
        push_i(&mut out, -42);
        assert_eq!(out, "-42");
    }
}

/// A signed integer.
pub fn push_i(out: &mut String, n: i64) {
    if let Ok(small) = i32::try_from(n) {
        return push_u32(out, small.unsigned_abs(), small < 0);
    }
    if n < 0 {
        out.push('-');
    }
    push_u(out, n.unsigned_abs());
}



/// A signed number with up to three decimals.
pub fn snum(out: &mut String, value: f64) {
    if value < 0.0 {
        out.push('-');
        num(out, -value);
    } else {
        num(out, value);
    }
}

/// A quoted string with JSON escaping, for text the player or the host gave.
pub fn qs(out: &mut String, text: &str) {
    out.push('"');
    escape_into(out, text);
    out.push('"');
}

/// JSON string escaping without the quotes.
pub fn escape_into(out: &mut String, text: &str) {
    if !text.bytes().any(|b| b == b'"' || b == b'\\' || b < 0x20) {
        out.push_str(text);
        return;
    }
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn signed_and_escaped() {
        let mut out = String::new();
        snum(&mut out, -12.5);
        out.push(' ');
        qs(&mut out, "a\"b\\c\nd");
        assert_eq!(out, "-12.5 \"a\\\"b\\\\c\\nd\"");
    }
}
