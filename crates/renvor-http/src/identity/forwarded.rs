//! Parsing `Forwarded` (RFC 7239) and `X-Forwarded-For`, **fail-closed**.
//!
//! # This module is a security boundary
//!
//! If you are changing this file, you are changing what Renvor believes about who is asking. Every
//! refusal below closes a specific bypass, and loosening one to make a test pass moves the bypass
//! back in.
//!
//! # Why Renvor owns a parser here
//!
//! Constitution principle III forbids writing a parser to own the implementation, and requires an
//! accepted ADR where one is genuinely needed. The evaluation is ADR-0012, Finding 4: the
//! maintained candidates parse these headers **unconditionally**. None of them models *whether the
//! header should be trusted at all*, which is the only question this crate needs answered. Adopting
//! one would make the default configuration derive identity from attacker-controlled input.
//!
//! # Fail-closed means attributing to the peer Renvor observed
//!
//! Failing closed here never means "attribute the request to the value we could not parse". It
//! means falling back to the direct peer, which is always a fact.
//!
//! # Rightmost, not leftmost
//!
//! `X-Forwarded-For` is appended to by each hop, so the **leftmost** entry is the one furthest from
//! the server and the one a client can write freely. The **rightmost** entry is the one the nearest
//! proxy added. Renvor reads the rightmost, which is the only entry the trusted hop vouched for.

use std::net::IpAddr;

/// Extracts a client address from a single `X-Forwarded-For` header value.
///
/// Returns `None` — fail closed — when the value is empty, carries a control character, or its
/// rightmost entry is not a bare address.
#[must_use]
pub fn from_x_forwarded_for(value: &str) -> Option<IpAddr> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }

    // Rightmost: the entry the nearest hop appended. See the module documentation.
    let last = value.rsplit(',').next()?.trim();
    parse_address(last)
}

/// Extracts a client address from a single `Forwarded` header value (RFC 7239).
///
/// Reads the **last** forwarded-element's `for=` parameter, for the same reason
/// [`from_x_forwarded_for`] reads the rightmost entry.
///
/// Returns `None` — fail closed — when the value is empty, carries a control character, has no
/// `for=` parameter, or names an obfuscated or `unknown` identifier. RFC 7239 permits those forms;
/// they are legal syntax that carries no address, and treating them as one would be an invention.
#[must_use]
pub fn from_forwarded(value: &str) -> Option<IpAddr> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }

    // QUOTE-AWARE SPLITTING, both times.
    //
    // RFC 7239 permits a quoted node value, and a quoted value may contain `,` and `;` — the very
    // characters that separate elements and parameters. A naive `split(',')` on
    // `for="1.2.3.4, for=9.9.9.9"` yields a second "element" from inside one quoted string, and the
    // parser then reads an address the sender never offered as its own node.
    //
    // This is only reachable from a peer already inside the trusted set, so it is a correctness
    // defect rather than a spoofing route. It is fixed rather than documented, because "only a
    // trusted proxy can trigger it" is an argument that stops being true the moment somebody adds
    // a proxy.
    let elements = split_outside_quotes(value, ',');
    let last_element = elements.last()?;

    for parameter in split_outside_quotes(last_element, ';') {
        // A parameter with no `=` is not RFC 7239 syntax. Refusing the whole header rather than
        // skipping the parameter is deliberate: a value we cannot fully parse is a value we do not
        // understand, and guessing at the rest of it is how a parser disagrees with its upstream.
        let (name, raw) = parameter.split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("for") {
            continue;
        }

        let raw = raw.trim().trim_matches('"');

        // RFC 7239 §6.3: an obfuscated identifier begins with `_`. `unknown` is also legal. Both
        // are valid syntax carrying no address.
        if raw.is_empty() || raw.starts_with('_') || raw.eq_ignore_ascii_case("unknown") {
            return None;
        }

        return parse_address(raw);
    }

    None
}

/// Splits on `delimiter`, ignoring delimiters inside a double-quoted run.
///
/// Deliberately simple: RFC 7239 quoted strings permit `\"` escaping, and this treats a backslash
/// as an ordinary character. The consequence is that a value using an escaped quote is split
/// differently than a fully conformant parser would split it — which produces a value that fails to
/// parse as an address, and therefore **fails closed**. A more permissive escape handler would be
/// more correct and would have more ways to be wrong.
fn split_outside_quotes(value: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;

    for (index, character) in value.char_indices() {
        match character {
            '"' => quoted = !quoted,
            c if c == delimiter && !quoted => {
                parts.push(&value[start..index]);
                start = index + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&value[start..]);
    parts
}

/// Parses one node identifier into an address, refusing every ambiguous form.
fn parse_address(raw: &str) -> Option<IpAddr> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    // A bracketed IPv6 node, optionally with a port: `[2001:db8::1]:4711`.
    if let Some(rest) = raw.strip_prefix('[') {
        let close = rest.find(']')?;
        let (literal, after) = rest.split_at(close);
        let after = &after[1..];
        if !after.is_empty() && !after.starts_with(':') {
            return None;
        }
        return reject_zone(literal);
    }

    // An unbracketed value. More than one colon means an unbracketed IPv6 literal, which RFC 7239
    // does not permit and which is ambiguous with `address:port`. Refuse rather than guess — a
    // guess here silently changes which address a request is attributed to.
    match raw.split(':').count() {
        1 => reject_zone(raw),
        2 => reject_zone(raw.split(':').next()?),
        _ => None,
    }
}

/// Refuses a zone identifier, then parses.
///
/// A scoped address such as `fe80::1%eth0` names an interface on the *parsing host*, which is
/// meaningless as a remote client identity and whose textual form varies by platform. Refused
/// rather than silently stripped, because stripping it would attribute the request to a different
/// address than the one supplied.
fn reject_zone(raw: &str) -> Option<IpAddr> {
    if raw.contains('%') {
        return None;
    }
    raw.parse().ok()
}

/// Selects the single header value, refusing ambiguity.
///
/// **Two headers of the same name are refused**, rather than joined or chosen between. Different
/// hops resolve a repeated header differently, and a disagreement between hops is exactly how a
/// request passes a check each hop believed it had applied.
#[must_use]
pub fn single_value<'a>(values: &[&'a str]) -> Option<&'a str> {
    match values {
        [only] => Some(only),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{from_forwarded, from_x_forwarded_for, single_value};
    use std::net::{IpAddr, Ipv4Addr};

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn x_forwarded_for_reads_the_rightmost_entry() {
        // The leftmost entry is client-writable; the rightmost is what the nearest hop appended.
        assert_eq!(
            from_x_forwarded_for("1.2.3.4, 198.51.100.7"),
            Some(v4(198, 51, 100, 7))
        );
        assert_eq!(
            from_x_forwarded_for("203.0.113.9"),
            Some(v4(203, 0, 113, 9))
        );
        assert_eq!(
            from_x_forwarded_for("  203.0.113.9  ,  198.51.100.7  "),
            Some(v4(198, 51, 100, 7))
        );
    }

    #[test]
    fn every_hostile_x_forwarded_for_fails_closed() {
        for hostile in [
            "",
            "   ",
            "not-an-address",
            "1.2.3.4\r\nX-Injected: yes",
            "1.2.3.4, ",
            "999.999.999.999",
            "1.2.3.4:80:90",
            "2001:db8::1",    // unbracketed IPv6: ambiguous with host:port
            "fe80::1%eth0",   // zone identifier
            "[fe80::1%eth0]", // zone identifier, bracketed
            "1.2.3.4; DROP TABLE",
        ] {
            assert_eq!(
                from_x_forwarded_for(hostile),
                None,
                "`{hostile}` produced an address"
            );
        }

        // POSITIVE CONTROL: a legitimate value parses, so the refusals are about the inputs.
        assert_eq!(
            from_x_forwarded_for("198.51.100.7"),
            Some(v4(198, 51, 100, 7))
        );
    }

    #[test]
    fn forwarded_reads_the_last_elements_for_parameter() {
        assert_eq!(
            from_forwarded(r#"for=1.2.3.4, for=198.51.100.7"#),
            Some(v4(198, 51, 100, 7))
        );
        assert_eq!(
            from_forwarded(r#"for="198.51.100.7:4711""#),
            Some(v4(198, 51, 100, 7))
        );
        assert_eq!(
            from_forwarded(r#"proto=https;for=198.51.100.7;host=example.com"#),
            Some(v4(198, 51, 100, 7))
        );
        assert_eq!(
            from_forwarded(r#"for="[2001:db8::1]:4711""#),
            Some("2001:db8::1".parse::<IpAddr>().expect("valid"))
        );
    }

    #[test]
    fn rfc_7239_forms_that_carry_no_address_fail_closed() {
        for legal_but_addressless in [
            r#"for=unknown"#,
            r#"for=Unknown"#,
            r#"for=_hidden"#, // obfuscated identifier, RFC 7239 6.3
            r#"for="_secret""#,
            r#"for="#,
            r#"proto=https;host=example.com"#, // no `for` at all
            r#"for=1.2.3.4, for=unknown"#,     // the LAST element is the one that counts
        ] {
            assert_eq!(
                from_forwarded(legal_but_addressless),
                None,
                "`{legal_but_addressless}` produced an address"
            );
        }

        // POSITIVE CONTROL.
        assert_eq!(
            from_forwarded("for=198.51.100.7"),
            Some(v4(198, 51, 100, 7))
        );
    }

    #[test]
    fn a_quoted_value_containing_a_separator_is_not_split_inside_the_quotes() {
        // `for="1.2.3.4, for=9.9.9.9"` is ONE element whose node value happens to contain a comma.
        // A naive split would read `for=9.9.9.9"` as a second element and return 9.9.9.9, which
        // the sender never offered as its own node.
        assert_ne!(
            from_forwarded(r#"for="1.2.3.4, for=9.9.9.9""#),
            Some(v4(9, 9, 9, 9)),
            "a comma inside a quoted value produced a second element"
        );

        // Same for the parameter separator.
        assert_ne!(
            from_forwarded(r#"for="1.2.3.4;host=evil""#),
            Some(v4(9, 9, 9, 9))
        );

        // POSITIVE CONTROL: an UNQUOTED comma still separates elements, so the quoting logic has
        // not disabled element splitting altogether.
        assert_eq!(
            from_forwarded("for=1.2.3.4, for=9.9.9.9"),
            Some(v4(9, 9, 9, 9))
        );
    }

    #[test]
    fn a_parameter_with_no_equals_fails_closed() {
        // `secure` is not RFC 7239 syntax. Refusing the whole header is the fail-closed choice.
        assert_eq!(from_forwarded("proto=https;secure;for=1.2.3.4"), None);
    }

    #[test]
    fn a_control_character_anywhere_fails_closed() {
        assert_eq!(from_forwarded("for=1.2.3.4\r\nX: y"), None);
        assert_eq!(from_x_forwarded_for("1.2.3.4\n"), None);
    }

    #[test]
    fn a_repeated_header_is_refused_rather_than_chosen_between() {
        assert_eq!(single_value(&["for=1.2.3.4"]), Some("for=1.2.3.4"));
        assert_eq!(single_value(&[]), None);
        assert_eq!(single_value(&["for=1.2.3.4", "for=evil"]), None);
    }
}
