//! Canonical read addresses: formatting and one-pass decoding.
//!
//! A document is its path; a heading is `path#h/<segment>/<segment>` with the
//! full original heading vector and `;occurrence=N` when that vector repeats.
//! Path components and segments escape every byte outside the URI unreserved
//! set as uppercase `%XX`, so a literal `/` inside one heading is `%2F`.
//! Bare native IDs, `trace:N09` and `logic/claims.md#C04` stay entry selectors.
use crate::output::AgentError;
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    Document {
        path: String,
    },
    Heading {
        path: String,
        heading: Vec<String>,
        occurrence: Option<usize>,
    },
}

fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}
fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if unreserved(byte) {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "%{byte:02X}").expect("string write");
        }
    }
    encoded
}
pub fn document(path: &str) -> String {
    path.split('/').map(encode).collect::<Vec<_>>().join("/")
}
pub fn heading(path: &str, vector: &[&str], occurrence: Option<usize>) -> String {
    let mut address = format!("{}#h", document(path));
    for segment in vector {
        address.push('/');
        address.push_str(&encode(segment));
    }
    if let Some(occurrence) = occurrence {
        write!(address, ";occurrence={occurrence}").expect("string write");
    }
    address
}

fn invalid(input: &str, reason: &str) -> AgentError {
    AgentError {
        id: Some(input.into()),
        ..AgentError::semantic("invalid_address", format!("Address `{input}` {reason}"))
    }
}
fn decode(text: &str, input: &str) -> Result<String, AgentError> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let byte = bytes
            .get(index + 1..index + 3)
            .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok())
            .ok_or_else(|| invalid(input, "has a malformed percent escape"))?;
        decoded.push(byte);
        index += 3;
    }
    String::from_utf8(decoded).map_err(|_| invalid(input, "decodes to invalid UTF-8"))
}
fn occurrence(parameter: &str, input: &str) -> Result<usize, AgentError> {
    parameter
        .strip_prefix("occurrence=")
        .filter(|digits| {
            !digits.starts_with('0')
                && !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
        })
        .and_then(|digits| digits.parse().ok())
        .ok_or_else(|| invalid(input, "accepts only `;occurrence=N` with N >= 1"))
}

/// Parse a canonical document or heading address. `None` leaves the input to
/// entry and legacy selector resolution. Decoding happens exactly once; the
/// caller applies the normal document boundary checks to the decoded path.
pub fn parse(input: &str) -> Option<Result<Address, AgentError>> {
    let Some((path, fragment)) = input.split_once('#') else {
        if input.contains([':', '@']) {
            return None;
        }
        // A path by its raw or decoded spelling (`%2Fetc%2Fhosts` is one);
        // a name with a stray `%` that is no path stays an entry selector.
        let path_like =
            |text: &str| text.contains('/') || text.ends_with(".md") || text.ends_with(".yaml");
        return match decode(input, input) {
            Ok(path) if path_like(input) || path_like(&path) => {
                Some(Ok(Address::Document { path }))
            }
            Err(error) if path_like(input) => Some(Err(error)),
            _ => None,
        };
    };
    let rest = fragment.strip_prefix("h/")?;
    Some((|| {
        let (segments, parameter) = match rest.split_once(';') {
            Some((segments, parameter)) => (segments, Some(occurrence(parameter, input)?)),
            None => (rest, None),
        };
        Ok(Address::Heading {
            path: decode(path, input)?,
            heading: segments
                .split('/')
                .map(|segment| decode(segment, input))
                .collect::<Result<_, _>>()?,
            occurrence: parameter,
        })
    })())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_addresses_round_trip_reserved_bytes_and_literal_slashes() {
        let vector = ["Arch", "A/B", "Résumé; 50% ~ok_-."];
        let address = heading("logic/my notes.md", &vector, Some(2));
        assert_eq!(
            address,
            "logic/my%20notes.md#h/Arch/A%2FB/R%C3%A9sum%C3%A9%3B%2050%25%20~ok_-.;occurrence=2"
        );
        assert_eq!(
            parse(&address).unwrap().unwrap(),
            Address::Heading {
                path: "logic/my notes.md".into(),
                heading: vector.iter().map(|s| s.to_string()).collect(),
                occurrence: Some(2),
            }
        );
        assert_ne!(
            heading("p.md", &["A/B"], None),
            heading("p.md", &["A", "B"], None)
        );
    }

    #[test]
    fn non_canonical_inputs_stay_entry_selectors() {
        for input in [
            "C04",
            "trace:N09",
            "logic/claims.md#C04",
            "doc.md#A/B",
            "Term",
            "50% rule",
        ] {
            assert!(parse(input).is_none(), "{input}");
        }
        assert_eq!(
            parse("%2Fetc%2Fhosts").unwrap().unwrap(),
            Address::Document {
                path: "/etc/hosts".into()
            }
        );
        assert_eq!(
            parse("PAPER.md").unwrap().unwrap(),
            Address::Document {
                path: "PAPER.md".into()
            }
        );
    }

    #[test]
    fn malformed_escapes_and_parameters_reject() {
        for input in [
            "p.md#h/A%2",
            "p.md#h/A%+F",
            "p.md#h/%C3",
            "p.md#h/A;occurrence=",
            "p.md#h/A;occurrence=1;occurrence=2",
            "a%G1.md",
        ] {
            assert_eq!(parse(input).unwrap().unwrap_err().code, "invalid_address");
        }
    }
}
