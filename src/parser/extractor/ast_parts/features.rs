fn generate_features(
    name: &str,
    kind: NodeKind,
    parent: Option<&str>,
    file_path: &str,
) -> Vec<String> {
    let mut features = split_identifier(name);
    features.push(format!("{:?}", kind).to_lowercase());

    if let Some(parent) = parent {
        features.extend(split_identifier(parent));
    }

    for segment in file_path.split(&['/', '\\'][..]) {
        let stem = segment
            .strip_suffix(".rs")
            .or_else(|| segment.strip_suffix(".py"))
            .or_else(|| segment.strip_suffix(".ts"))
            .or_else(|| segment.strip_suffix(".tsx"))
            .or_else(|| segment.strip_suffix(".js"))
            .or_else(|| segment.strip_suffix(".jsx"))
            .or_else(|| segment.strip_suffix(".go"))
            .or_else(|| segment.strip_suffix(".java"))
            .or_else(|| segment.strip_suffix(".cs"))
            .or_else(|| segment.strip_suffix(".cpp"))
            .or_else(|| segment.strip_suffix(".hpp"))
            .or_else(|| segment.strip_suffix(".h"))
            .or_else(|| segment.strip_suffix(".swift"))
            .or_else(|| segment.strip_suffix(".rb"))
            .unwrap_or(segment);
        if stem.len() > 2 && stem != "src" && stem != "lib" && stem != "mod" {
            features.extend(split_identifier(stem));
        }
    }

    features.sort();
    features.dedup();
    features
}
