pub(crate) fn collation_language() -> String {
    ["LC_ALL", "LC_COLLATE", "LANG"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find_map(|value| language_tag(&value))
        .unwrap_or_else(|| "und".into())
}

pub(crate) fn language_tag(posix: &str) -> Option<String> {
    let name = posix.split(['.', '@']).next()?.trim();
    match name {
        "" | "C" | "POSIX" => None,
        name => Some(name.replace('_', "-")),
    }
}
