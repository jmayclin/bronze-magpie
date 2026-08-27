use std::{
    env,
    fs::{File, read_dir},
    path::PathBuf,
};

fn recurse_directory(path: PathBuf) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let mut folders = Vec::new();
    folders.push(path);

    while let Some(folder) = folders.pop() {
        for entry in read_dir(folder).unwrap() {
            let entry = entry.unwrap();
            if entry.metadata().unwrap().is_dir() {
                folders.push(entry.path());
            } else {
                entries.push(entry.path());
            }
        }
    }
    entries
}

fn main() {
    // println!("cargo::rerun-if-changed=src/hello.c");
    let workspace_dir = env::var("CARGO_MANIFEST_DIR").map(PathBuf::from).unwrap();
    let assets_dir = workspace_dir.join("../assets");
    println!("cargo:warning={}", assets_dir.display());
    let entries = recurse_directory(assets_dir);
    println!("cargo:warning={:#?}", entries);

    // a change
}
