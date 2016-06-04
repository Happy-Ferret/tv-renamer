use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub mod logging;
pub mod traits;
pub mod tokenizer;

use self::traits::Digits;
use self::tokenizer::TemplateToken;

use tvdb;

#[derive(Clone, Debug)]
pub struct Arguments {
    // Automatically infer the name of a series and season number by the directory structure.
    pub automatic:     bool,

    // Print the changes that would have been made without actually making any changes.
    pub dry_run:       bool,

    // Log the changes that were made to the disk.
    pub log_changes:   bool,

    // Print all changes that are being attempted and performed.
    pub verbose:       bool,

    // Contains the base directory of the series to rename.
    pub directory:     String,

    // Contains the name of the series to be renamed.
    pub series_name:   String,

    // Contains the season number to add to the filename and for use with TVDB lookups.
    pub season_number: usize,

    // The starting episode index count to start renaming from.
    pub episode_count: usize,

    // The number of zeros to use when padding episode numbers.
    pub pad_length:    usize,

    // The template used for setting the naming scheme of episodes.
    pub template:      Vec<TemplateToken>
}

impl Arguments {
    /// Given a source of episodes from a directory, this returns a list of their target paths.
    pub fn get_targets(&self, directory: &str, episodes: &[PathBuf], episode_index: usize) -> Result<Vec<PathBuf>, String> {
        let api = tvdb::Tvdb::new("0629B785CE550C8D");
        let series_info = if self.template.contains(&TemplateToken::TVDB) {
            match api.search(self.series_name.as_str(), "en") {
                Ok(reply) => Some(reply),
                Err(_) => { return Err(String::from("unable to get TVDB series information")); }
            }
        } else {
            None
        };

        let mut output: Vec<PathBuf> = Vec::new();
        let mut current_index = episode_index;
        for file in episodes {
            // TVDB Titles
            let tvdb_title = if self.template.contains(&TemplateToken::TVDB) {
                let reply = series_info.clone().unwrap();
                match api.episode(&reply[0], self.season_number as u32, current_index as u32) {
                    Ok(episode) => episode.episode_name,
                    Err(_) => { return Err(format!("episode '{}' does not exist", file.to_string_lossy())); }
                }
            } else {
                String::new()
            };

            // Get target destination for the current file.
            let new_destination = self.get_destination(Path::new(directory), file, current_index, &tvdb_title);
            output.push(new_destination);
            current_index += 1;
        }
        Ok(output)
    }

    /// Obtain the target path of the file based on the episode count
    pub fn get_destination(&self, directory: &Path, file: &Path, episode: usize, title: &str) -> PathBuf {
        let mut destination = String::from(directory.to_str().unwrap());
        destination.push('/');

        let mut filename = String::new();
        for pattern in self.template.clone() {
            match pattern {
                TemplateToken::Character(value) => filename.push(value),
                TemplateToken::Series  => filename.push_str(self.series_name.clone().as_str()),
                TemplateToken::Season  => filename.push_str(self.season_number.to_string().as_str()),
                TemplateToken::Episode => filename.push_str(episode.to_padded_string('0', self.pad_length).as_str()),
                TemplateToken::TVDB    => filename.push_str(title),
            }
        }
        filename = String::from(filename.trim()); // Remove extra spaces
        filename = filename.replace("/", "-");     // Remove characters that are invalid in pathnames

        // Append the extension
        let extension = file.extension().unwrap_or_else(|| OsStr::new("")).to_str().unwrap_or("");
        if !extension.is_empty() {
            filename.push('.');
            filename.push_str(extension);
        }

        // Return the path as a PathBuf
        destination.push_str(&filename);
        PathBuf::from(destination)
    }
}

/// Takes a pathname and shortens it for readability.
pub fn shorten_path(path: &Path) -> PathBuf {
    if let Ok(value) = path.strip_prefix(&env::current_dir().unwrap()) {
        let mut path = PathBuf::from(".");
        path.push(value);
        path
    } else {
        match path.strip_prefix(&env::home_dir().unwrap()) {
            Ok(value) => {
                let mut path = PathBuf::from("~");
                path.push(value);
                path
            },
            Err(_) => path.to_path_buf()
        }
    }
}

/// Given a directory path, derive the number of the season and assign it.
pub fn derive_season_number(season: &Path) -> Option<usize> {
    let mut directory_name = season.file_name().unwrap().to_str().unwrap().to_lowercase();
    match directory_name.as_str() {
        "season0" | "season 0" | "specials" => Some(0),
        _ => {
            directory_name = directory_name.replace("season", "");
            directory_name = directory_name.replace(" ", "");
            if let Ok(season_number) = directory_name.parse::<usize>() {
                Some(season_number)
            } else {
                None
            }
        }
    }
}

/// Collects a list of all of the seasons in a given directory.
pub fn get_seasons(directory: &str) -> Result<Vec<PathBuf>, &str> {
    if let Ok(files) = fs::read_dir(directory) {
        let mut seasons = Vec::new();
        for entry in files {
            if let Ok(entry) = entry {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_dir() {
                        seasons.push(entry.path());
                    }
                } else {
                    return Err("unable to get metadata");
                }
            } else {
                return Err("unable to get directory entry");
            }
        }
        seasons.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
        Ok(seasons)
    } else {
        Err("unable to read directory")
    }
}

/// Collects a list of all of the episodes in a given directory.
pub fn get_episodes(directory: &str) -> Result<Vec<PathBuf>, &str> {
    if let Ok(files) = fs::read_dir(directory) {
        let mut episodes = Vec::new();
        for entry in files {
            if let Ok(entry) = entry {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() { episodes.push(entry.path()); }
                } else {
                    return Err("unable to get metadata");
                }
            } else {
                return Err("unable to get file entry");
            }
        }
        episodes.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
        Ok(episodes)
    } else {
        Err("unable to read file")
    }
}

#[test]
fn test_derive_season_number() {
    assert_eq!(derive_season_number(&Path::new("Specials")), Some(0));
    assert_eq!(derive_season_number(&Path::new("Season 0")), Some(0));
    assert_eq!(derive_season_number(&Path::new("Season 1")), Some(1));
    assert_eq!(derive_season_number(&Path::new("season9")), Some(9));
    assert_eq!(derive_season_number(&Path::new("Extras")), None);
}