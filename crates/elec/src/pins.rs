//! Wire and pin conventions of the electrical layer.
//!
//! A net is only as good as the points it connects, and two conventions decide those
//! points. AutoCAD Electrical stores a symbol's wire connection points as invisible
//! attributes named `X<n>TERM<nn>` — the attribute's insertion point is the pin, and
//! the digits after `TERM` are the pin number. Wires themselves live on a `WIRES`
//! layer; a print that was flattened onto layer `0` has to fall back to a length test,
//! which the command layer does.
//!
//! Keeping both rules here means the geometry pass, the reports and the commands agree
//! about what a pin is, and lets the rules be tested without a drawing host.

/// True when a layer carries electrical wires.
///
/// `WIRENO` (the label layer) and `WIRES` both start with `WIRE`, so the comparison is
/// on the whole name; otherwise a wire number would be read as a wire segment.
pub fn is_wire_layer(layer: &str) -> bool {
    let name = layer.trim();
    name.eq_ignore_ascii_case("WIRES") || name.eq_ignore_ascii_case("WIRE")
}

/// The pin number an ACADE wire-connection attribute names.
///
/// `X1TERM02` means "connection point 2, approached from the right"; `X4TERM01` is the
/// left-hand point 1. The direction digit is not part of the identity, so both return
/// the bare terminal number (`"2"` and `"1"`). Attributes that are not wire connection
/// points — `XREF`, `X8LINK`, `TAG1` — return `None`, which keeps metadata out of the
/// pin list.
pub fn pin_number(attribute: &str) -> Option<String> {
    let name = attribute.trim().to_ascii_uppercase();
    let rest = name.strip_prefix('X')?;
    let mut characters = rest.chars();
    if !characters.next()?.is_ascii_digit() {
        return None;
    }
    let suffix = rest[1..].strip_prefix("TERM")?;
    if suffix.is_empty() || !suffix.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    let trimmed = suffix.trim_start_matches('0');
    Some(if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    })
}

/// Parse the `x,y` coordinate some symbols store as a pin attribute's value.
///
/// `ELECPLC` writes `X1TERM01` as the pin's own coordinates so a later pass can follow
/// the wire from the point; ACADE itself leaves the value empty and keeps the pin at the
/// attribute's insertion point. Both forms are accepted.
pub fn parse_point(value: &str) -> Option<(f64, f64)> {
    let mut parts = value.trim().split(',');
    let x = parts.next()?.trim().parse::<f64>().ok()?;
    let y = parts.next()?.trim().parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some((x, y))
}

/// True for blocks that are drawing furniture rather than a placeable device.
///
/// These blocks carry geometry but no device identity: junction dots, terminals, rails
/// and arrows. Treating one as a component would put a "device" on every crossing or
/// every terminal block, and treating their geometry as pins would anchor nets to the
/// wrong thing. The list is the one measured on the project's own drawings plus the
/// names the local library uses.
pub fn is_non_device_block(name: &str) -> bool {
    let name = name.trim().to_ascii_uppercase();
    if name.starts_with('*') {
        // Anonymous blocks (`*X26`, `*D12`, `A$C...`): exploded geometry, not a symbol.
        return true;
    }
    matches!(
        name.as_str(),
        "DOT"
            | "WDDOT"
            | "BOX"
            | "TY"
            | "TERMINAL"
            | "GROUND"
            | "ELGND"
            | "EARTH"
            | "JUMPER"
            | "ARROW-L"
            | "ARROW-S"
            | "PL"
            | "PT"
            | "MUFFLER"
            | "CYL-RET"
            | "SI"
            | "JUNCTION"
            | "WIRE_LABEL"
    ) || name.starts_with("ARROW-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_layers_are_exact() {
        assert!(is_wire_layer("WIRES"));
        assert!(is_wire_layer(" wires "));
        assert!(is_wire_layer("Wire"));
        assert!(!is_wire_layer("WIRENO"));
        assert!(!is_wire_layer("0"));
        assert!(!is_wire_layer("SYMS"));
    }

    #[test]
    fn pin_numbers_come_from_the_term_suffix() {
        assert_eq!(pin_number("X1TERM02").as_deref(), Some("2"));
        assert_eq!(pin_number("X4TERM01").as_deref(), Some("1"));
        assert_eq!(pin_number("x0term10").as_deref(), Some("10"));
        assert_eq!(pin_number("X0TERM00").as_deref(), Some("0"));
        assert_eq!(pin_number("X8LINK"), None);
        assert_eq!(pin_number("XREF"), None);
        assert_eq!(pin_number("TAG1"), None);
        assert_eq!(pin_number("X1TERM"), None);
        assert_eq!(pin_number("XTERM01"), None);
        assert_eq!(pin_number("X1TERM0A"), None);
    }

    #[test]
    fn pin_points_parse_xy() {
        assert_eq!(parse_point("1.25,2.5"), Some((1.25, 2.5)));
        assert_eq!(parse_point(" -3 , 4 "), Some((-3.0, 4.0)));
        assert_eq!(parse_point(""), None);
        assert_eq!(parse_point("1.0"), None);
        assert_eq!(parse_point("1,2,3"), None);
        assert_eq!(parse_point("a,b"), None);
        assert_eq!(parse_point("NaN,1"), None);
    }

    #[test]
    fn furniture_blocks_are_not_devices() {
        for name in ["DOT", "WDDOT", "BOX", "TY", "TERMINAL", "Arrow-l", "ARROW-S", "*X26", "JUNCTION"] {
            assert!(is_non_device_block(name), "{name}");
        }
        for name in ["CONT-NO", "CONTACT_NO", "MOTOR", "PB-NO", "24VDCPRX", "HMO13"] {
            assert!(!is_non_device_block(name), "{name}");
        }
    }
}
