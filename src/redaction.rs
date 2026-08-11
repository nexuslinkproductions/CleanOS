//! Redaction for report output and report JSON (AC-5).
//! Implemented without regex crates (deps limited by SPEC).

/// Apply all SPEC section 7 redaction patterns.
pub fn redact_text(input: &str) -> String {
    let mut s = redact_users_paths(input);
    s = redact_uuids(&s);
    s = redact_tokens(&s);
    s = redact_serials(&s);
    s
}

fn redact_users_paths(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    let needle = b"/Users/";
    while i < bytes.len() {
        if bytes[i..].starts_with(needle) {
            out.push_str("/Users/");
            i += needle.len();
            let start = i;
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c == '/' || c.is_whitespace() || c == '"' || c == '\'' {
                    break;
                }
                i += 1;
            }
            if i > start {
                out.push_str("<user>");
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn is_hex_digit(c: u8) -> bool {
    c.is_ascii_hexdigit()
}

fn redact_uuids(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 36 <= bytes.len() && looks_like_uuid(&bytes[i..i + 36]) {
            out.push_str("<uuid>");
            i += 36;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn looks_like_uuid(slice: &[u8]) -> bool {
    if slice.len() < 36 {
        return false;
    }
    let groups = [(0, 8), (9, 4), (14, 4), (19, 4), (24, 12)];
    let dashes = [8usize, 13, 18, 23];
    for d in dashes {
        if slice[d] != b'-' {
            return false;
        }
    }
    for (start, len) in groups {
        for b in &slice[start..start + len] {
            if !is_hex_digit(*b) {
                return false;
            }
        }
    }
    true
}

fn redact_tokens(input: &str) -> String {
    let mut s = input.to_string();
    s = redact_prefix_token(&s, "sk-");
    s = redact_prefix_token(&s, "pk-");
    s = redact_prefix_token(&s, "AKIA");
    s = redact_key_equals(&s, "api_key=");
    s = redact_key_equals(&s, "token=");
    s = redact_bearer(&s);
    s
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || c == '_'
        || c == '-'
        || c == '.'
        || c == '+'
        || c == '/'
        || c == '='
}

fn redact_prefix_token(input: &str, prefix: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let pref_lower = prefix.to_ascii_lowercase();
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if lower[i..].starts_with(&pref_lower) {
            let after = i + prefix.len();
            let mut j = after;
            while j < bytes.len() && is_token_char(bytes[j] as char) {
                j += 1;
            }
            if j > after {
                out.push_str("<redacted>");
                i = j;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn redact_key_equals(input: &str, key: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let key_lower = key.to_ascii_lowercase();
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if lower[i..].starts_with(&key_lower) {
            out.push_str(&input[i..i + key.len()]);
            i += key.len();
            let start = i;
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c.is_whitespace() || c == '"' || c == '\'' || c == '&' || c == ',' {
                    break;
                }
                i += 1;
            }
            if i > start {
                out.push_str("<redacted>");
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn redact_bearer(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let needle = "bearer ";
    while i < bytes.len() {
        if lower[i..].starts_with(needle) {
            out.push_str(&input[i..i + needle.len()]);
            i += needle.len();
            let start = i;
            while i < bytes.len() && is_token_char(bytes[i] as char) {
                i += 1;
            }
            if i > start {
                out.push_str("<redacted>");
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn redact_serials(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if looks_like_apple_serial(bytes, i) {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j] as char).is_ascii_alphanumeric() {
                j += 1;
            }
            if j - i >= 10 && j - i <= 14 {
                out.push_str("<serial>");
                i = j;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn looks_like_apple_serial(bytes: &[u8], i: usize) -> bool {
    if i + 3 > bytes.len() {
        return false;
    }
    let at_boundary = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
    if !at_boundary {
        return false;
    }
    let starts_c02 = bytes[i] == b'C' && bytes[i + 1] == b'0' && bytes[i + 2] == b'2';
    let starts_f2 = bytes[i] == b'F' && bytes[i + 1] == b'2';
    starts_c02 || starts_f2
}
