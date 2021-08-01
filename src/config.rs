use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use thiserror::Error;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Credentials {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Host {
    pub hostname: Option<String>,
    #[serde(default = "default_discovery")]
    pub discovery: bool,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub listen_port: Option<u16>,
}

fn default_discovery() -> bool {
    false
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Config {
    pub default: Option<String>,
    pub host: BTreeMap<String, Host>,
    pub listen_port: Option<u16>,
}

impl Config {
    pub fn get_host(&self, key: Option<&str>) -> Result<Host, Error> {
        let localhost = Host {
            hostname: Some("localhost".to_string()),
            ..Default::default()
        };
        match key {
            None => match &self.default {
                None if self.host.len() <= 1 => match self.host.iter().next() {
                    None => Ok(localhost),
                    Some(entry) => Ok({
                        let mut host = entry.1.clone();
                        if host.hostname.is_none() {
                            host.hostname = localhost.hostname.clone();
                        }
                        host
                    }),
                },
                None => Err(Error::DefaultHostError(
                    "Default is not set, but there is more than one host configured, and no Kodi host provided on the command line with the -k switch.".to_string(),
                )),
                Some(default) => match self.host.get(default) {
                    None => Err(Error::DefaultHostError(
                        "Default is set, but no matching host found in config".to_string(),
                    )),
                    Some(host) => Ok({
                        let mut host = host.clone();
                        if host.hostname.is_none() {
                            host.hostname = Some(host.hostname.unwrap_or(default.clone()));
                        }
                        host
                    }),
                },
            },
            Some(key) => match self.host.get(key) {
                None => Ok(Host {
                    hostname: Some(key.to_string()),
                    ..Default::default()
                }),
                Some(host) => {
                    log::debug!("Use the named host config: {}", key);
                    Ok(host.clone())
                }
            },
        }
    }
}

#[derive(Error, Debug)]
pub struct ParseError {
    pub filename: String,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to parse {}: {}", self.filename, self.message)
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    ParseError(ParseError),

    #[error(transparent)]
    TomlDeError(#[from] toml::de::Error),

    #[error(transparent)]
    TomlSerError(#[from] toml::ser::Error),

    #[error(transparent)]
    IOError(#[from] io::Error),

    #[error(transparent)]
    AtomicIOError(#[from] atomicwrites::Error<io::Error>),

    #[error("Cannot determine default host: {}", .0)]
    DefaultHostError(String),
}

pub static FILENAME: &str = "koko.ini";

impl Config {
    pub fn new() -> Config {
        let host = vec![].into_iter().collect();
        Config {
            host,
            ..Default::default()
        }
    }

    // If no file is found, returns default config instead of error
    pub fn load(filename: &str) -> Result<Config, Error> {
        let contents = match fs::read_to_string(filename) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Config::new()),
            Err(error) => return Err(Error::IOError(error)),
        };
        let config = match toml::from_str(&contents) {
            Ok(contents) => contents,
            Err(error) if error.line_col().is_some() => {
                return Err(Error::ParseError(ParseError {
                    filename: String::from(filename),
                    message: format!("{}", error),
                }));
            }
            Err(error) => return Err(Error::TomlDeError(error)),
        };
        log::info!("Loaded config from {}", filename);
        Ok(config)
    }

    pub fn save(self, filename: &str) -> Result<(), Error> {
        let mut config_dir = Path::new(filename).to_path_buf();
        config_dir.pop();
        std::fs::create_dir_all(config_dir)?;
        let contents = toml::to_string(&self)?;
        let writer = atomicwrites::AtomicFile::new(filename, atomicwrites::AllowOverwrite);
        writer.write(|f| f.write_all(contents.as_bytes()))?;
        log::info!("Wrote config to {}", filename);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save() {
        let mut config = Config::new();
        config.default = Some("test".to_string());
        config.host.insert(
            "test".to_string(),
            Host {
                hostname: Some(String::from("localhost")),
                port: Some(42),
                username: Some(String::from("helo")),
                password: Some(String::from("world")),
            },
        );
        config.save("test.ini").unwrap();
    }
}
