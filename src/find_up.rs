use std::path::{Path, PathBuf};

pub(crate) fn find_file(starting_from: &Path, target_filename: &str) -> Option<PathBuf> {
    let starting_directory = {
        let metadata = std::fs::metadata(starting_from).unwrap();
        if metadata.is_dir() {
            starting_from
        } else {
            starting_from.parent().unwrap_or_else(|| Path::new("."))
        }
    };

    let mut path: PathBuf = starting_directory.to_owned();

    loop {
        path.push(target_filename);
        let found_target = path.is_file();

        if found_target {
            // Pop the filename because we want to return the directory
            path.pop();
            break Some(path);
        }

        if !(path.pop() && path.pop()) {
            // remove file && remove parent
            break None;
        }
    }
}
