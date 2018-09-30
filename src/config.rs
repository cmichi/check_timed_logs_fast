extern crate fancy_regex;

use self::fancy_regex::Regex;

pub struct Config {
  pub interval_to_check: u64,
  pub search_pattern: String,
  pub logfile: String,

  pub max_critical_matches: u64,
  pub max_warning_matches: u64,
  pub date_pattern: String,
  pub timeposition: usize,
  pub debug: bool,
  pub verbose: bool,
  pub re: Regex,
}

impl Config {
  pub fn new(
    interval_to_check: u64,
    search_pattern: String,
    logfile: String,

    max_critical_matches: u64,
    max_warning_matches: u64,
    mut date_pattern: String,
    timeposition: usize,
    debug: bool,
    verbose: bool,
  ) -> Result<Config, ConfigError> {
    if logfile.is_empty() {
      return Err(ConfigError::LogfileRequired);
    }
    if search_pattern.is_empty() {
      return Err(ConfigError::PatternRequired);
    }
    if interval_to_check < 1 {
      return Err(ConfigError::IntervalInvalid);
    }
    if logfile == "-" {
      return Err(ConfigError::StdinUnsupported);
    }
    if date_pattern.len() == 0 {
      date_pattern = String::from("%Y-%m-%d %H:%M:%S");
    }

    Ok(Config {
      interval_to_check,
      search_pattern: search_pattern.to_owned(),
      logfile,

      max_critical_matches,
      max_warning_matches,
      date_pattern,
      timeposition,
      debug,
      verbose,
      re: Regex::new(&search_pattern.to_owned()).expect("regex cannot be created"),
    })
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum ConfigError {
   LogfileRequired,
   PatternRequired,
   IntervalInvalid,
   StdinUnsupported,
}

impl From<ConfigError> for String {
  fn from(error: ConfigError) -> Self {
    match error {
      ConfigError::LogfileRequired => "no -logfile".to_owned(),
      ConfigError::PatternRequired => "no -pattern".to_owned(),
      ConfigError::IntervalInvalid => "interval needs to be set and be >= 1".to_owned(),
      ConfigError::StdinUnsupported => "stdin as path is not supported".to_owned(),
    }
  }
}
