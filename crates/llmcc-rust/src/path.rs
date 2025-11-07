use llmcc_descriptor::PathQualifier;

/// Parse a Rust `::`-separated path into a qualifier that carries its segments.
pub fn parse_rust_path(raw: &str) -> PathQualifier {
    if raw.is_empty() {
        return PathQualifier::relative(Vec::new());
    }

    let mut segments: Vec<String> = raw.split("::").map(|s| s.to_string()).collect();

    if raw.starts_with("::") {
        while matches!(segments.first(), Some(segment) if segment.is_empty()) {
            segments.remove(0);
        }
        return PathQualifier::absolute(segments);
    }

    if matches!(segments.first().map(String::as_str), Some("crate")) {
        segments.remove(0);
        return PathQualifier::crate_root(segments);
    }

    if matches!(segments.first().map(String::as_str), Some("self" | "Self")) {
        segments.remove(0);
        return PathQualifier::self_type(segments);
    }

    let mut super_levels = 0u32;
    while matches!(segments.first().map(String::as_str), Some("super")) {
        segments.remove(0);
        super_levels = super_levels.saturating_add(1);
    }
    if super_levels > 0 {
        return PathQualifier::super_with_segments(super_levels, segments);
    }

    PathQualifier::relative(segments)
}
