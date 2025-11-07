use llmcc_descriptor::PathQualifier;

/// Parse a Rust `::`-separated path into a qualifier that carries its parts.
pub fn parse_rust_path(raw: &str) -> PathQualifier {
    if raw.is_empty() {
        return PathQualifier::relative(Vec::new());
    }

    let mut parts: Vec<String> = raw.split("::").map(|s| s.to_string()).collect();

    if raw.starts_with("::") {
        while matches!(parts.first(), Some(segment) if segment.is_empty()) {
            parts.remove(0);
        }
        return PathQualifier::absolute(parts);
    }

    if matches!(parts.first().map(String::as_str), Some("crate")) {
        parts.remove(0);
        return PathQualifier::crate_root(parts);
    }

    if matches!(parts.first().map(String::as_str), Some("self" | "Self")) {
        parts.remove(0);
        return PathQualifier::self_type(parts);
    }

    let mut super_levels = 0u32;
    while matches!(parts.first().map(String::as_str), Some("super")) {
        parts.remove(0);
        super_levels = super_levels.saturating_add(1);
    }
    if super_levels > 0 {
        return PathQualifier::super_with_segments(super_levels, parts);
    }

    PathQualifier::relative(parts)
}
