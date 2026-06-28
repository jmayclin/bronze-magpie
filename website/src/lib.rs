
const COMBINED_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/");

/// Given a path (e.g. `"index.html"`) to something in the asset folder, return the raw bytes
/// 
/// This should eventually return a static string, representing the baked in asset
pub fn asset(path: &str) -> Option<Vec<u8>> {
    let mut asset_path = COMBINED_PATH.to_string();
    asset_path.push_str(path);
    println!("{asset_path}");
    std::fs::read(asset_path).ok()
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
