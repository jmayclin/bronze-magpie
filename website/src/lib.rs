const COMBINED_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/");

const ASSETS: &[(&'static str, &'static [u8])] = &[
    (
        "index.html",
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/index.html")),
    ),
    (
        "styles.css",
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/styles.css")),
    ),
    (
        "logo.svg",
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/logo.svg")),
    ),
    (
        "favicon.ico",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../assets/favicon.ico"
        )),
    ),
    (
        "projects/index.html",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../assets/projects/index.html"
        )),
    ),
    (
        "fonts/bodoni_moda.ttf",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../assets/fonts/bodoni_moda.ttf"
        )),
    ),
];

/// Given a path (e.g. `"index.html"`) to something in the asset folder, return the raw bytes
///
/// This should eventually return a static string, representing the baked in asset
pub fn asset(path: &str) -> Option<&'static [u8]> {
    tracing::debug!("{path}");
    ASSETS
        .iter()
        .find(|(content_path, _content)| *content_path == path)
        .map(|(_content_path, content)| *content)
    // let mut asset_path = COMBINED_PATH.to_string();
    // asset_path.push_str(path);
    // tracing::debug!("{asset_path}");
    // std::fs::read(asset_path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert!(asset("index.html").is_some());
        assert!(asset("not_real.html").is_none());
    }
}
