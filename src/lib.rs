//! check_timed_logs_fast.
//!
//! ```
//! extern crate check_timed_logs_fast;
//!
//! use check_timed_logs_fast::*;
//!
//! fn main() {
//!   let conf = Config::new(
//!     5,                               // interval in minutes to check
//!     "timeout".to_owned(),            // regex to match in the file
//!     "./fixtures/logfile".to_owned(), // path to the log file
//!     5,                               // max_critical_matches
//!     1,                               // max_warning_matches
//!     "%Y-%m-%d %H:%M:%S".to_owned(),  // datepattern
//!     0,                               // timeposition = position of datepattern in logfile
//!     false,                           // flag to enable debug output
//!     false,                           // flag to enable verbose output
//!   ).unwrap();
//!
//!   let res = check_timed_logs_fast::run(&conf);
//!   match res {
//!     Err(err) => {
//!       eprintln!("ERROR: {}", err);
//!       // ...
//!     },
//!     Ok((matches, files_matched)) => {
//!       // ...
//!     }
//!   }
//! }
//! ```

extern crate chrono;
extern crate fancy_regex;
extern crate glob;
extern crate memmap;
extern crate time;

use chrono::prelude::*;
use fancy_regex::Regex;
use glob::glob;
use memmap::Mmap;
use std::fs::File;
use std::str;
use std::time::SystemTime;

mod utils;

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

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
enum SearchError {
  NotFile,
  EmptyFile,
  NotUtf8,
  TimestampTooOld,
}

impl From<SearchError> for String {
  fn from(error: SearchError) -> Self {
    match error {
      SearchError::NotFile => "not a file".to_owned(),
      SearchError::EmptyFile => "file empty".to_owned(),
      SearchError::NotUtf8 => "file not utf8".to_owned(),
      SearchError::TimestampTooOld => "timestamp in line too old".to_owned(),
    }
  }
}

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

pub fn run(conf: &Config) -> Result<(u64, u64), String> {
  let mut files_searched = 0;
  let mut exp = conf.logfile.to_owned();
  let star = String::from("*");
  exp.push_str(&star);

  let mut matches = 0;

  let pattern_spaces: Vec<&str> = conf.date_pattern.split_whitespace().collect();
  let whitespaces_in_date = pattern_spaces.len(); // = count of whitespaces
  
  if conf.debug {
    println!("looking for files matching {}", exp);
  }

  // the timestamp is adjusted to local time
  let now = SystemTime::now();
  let oldest_ts = utils::get_oldest_allowed_local_ts(conf, now);

  if conf.debug {
    let oldest_date_no_tz_offset = NaiveDateTime::from_timestamp(utils::get_oldest_allowed_utc_ts(conf, now) as i64, 0);
    let adjusted_date = NaiveDateTime::from_timestamp(utils::get_oldest_allowed_local_ts(conf, now) as i64, 0);
    println!("oldest allowed date in utc: {} and with tz offset: {}", oldest_date_no_tz_offset, adjusted_date);
  }
  
  // for all files that match pattern
  for entry in glob(&exp).expect("failed to read glob pattern") {
    match entry {
      Ok(path) => {
        let p = path.to_str().expect("path not available");

        if !utils::check_file_age(&conf, p) {
          if conf.debug {
            println!("skipping {:?} because too old", conf.logfile);
          }
          continue; 
        }

        let local_matches = search_file(p, &conf, whitespaces_in_date, oldest_ts);
        match local_matches {
          Ok(matches_in_file) => {
            files_searched += 1;
            matches += matches_in_file;
          },
          Err((err, matches_in_file)) => {
            // an error can occur because e.g. the file is empty, not utf8 or
            // because the timestamp of the line is too old. so we can
            // just stop searching further and add the matches found so far.
            if conf.debug {
              let err: String = err.into();
              eprintln!("ERROR while searching the file {}: {}
                        There were {} matches until the error appeared.", p, err, matches);
            }

            match err {
              SearchError::TimestampTooOld => files_searched += 1,
              _ => {},
            }

            matches += matches_in_file;
            continue;
          }
        }
      },
      Err(e) => eprintln!("ERROR: {:?}", e),
    }
  }
  Ok((matches, files_searched))
}

fn search_file(path: &str, conf: &Config, whitespaces_in_date: usize, oldest_ts: u64) -> Result<u64, (SearchError, u64)> {
  let mmap;
  let mut matches = 0;

  let file_in = File::open(path).expect("cannot open file");
  let metadata = file_in.metadata().expect("cannot get metadata");
  if !metadata.is_file() {
    return Err((SearchError::NotFile, 0));
  } else if metadata.len() > isize::max_value() as u64 {
    panic!("the file {} is too large to be safely mapped to memory:
            https://github.com/danburkert/memmap-rs/issues/69", path);
  } else if metadata.len() == 0 {
    return Err((SearchError::EmptyFile, 0));
  } 

  let (file, len) = {
    mmap = Mmap::open_path(path, memmap::Protection::Read).expect("cannot memmap");
    let bytes = unsafe { mmap.as_slice() };
    (bytes, mmap.len())
  };

  let mut last_printed = len as i64;
  let mut index = last_printed - 1;
  while index >= -1 {
    if index == -1 || file[index as usize] == '\n' as u8 {
      let line = &file[(index + 1) as usize..last_printed as usize];
      let is_match = search_line(line, whitespaces_in_date, oldest_ts, &conf);
      match is_match {
        Ok(v) => {
          if v {
            matches += 1;
          }
        },
        Err(err) => {
          return Err((err, matches));
        }
      }

      last_printed = index + 1;
    }

    index -= 1;
  }

  Ok(matches)
}

fn search_line(bytes: &[u8], whitespaces_in_datefields: usize, oldest_ts: u64, conf: &Config) -> Result<bool, SearchError> {
  if bytes.len() == 0 {
    return Ok(false);
  }

  let l = str::from_utf8(bytes);
  if l.is_err() {
      if conf.debug {
        eprintln!("skipping file because not utf8 parseable!");
      }
      return Err(SearchError::NotUtf8);
  }
  let line = l.unwrap().trim();
  if line.len() == 0 {
    return Ok(false);
  }

  if conf.debug {
    println!("searching line: {}", line);
  }

  let words: Vec<&str> = line.split_whitespace().collect();
  let datefields = words.get(conf.timeposition..(conf.timeposition + whitespaces_in_datefields));
  let extracted_date;
  match datefields {
    None => return Ok(false),
    Some(fields) => {
      extracted_date = fields.join(" ");
    }
  };

  let date = utils::parse_date(&extracted_date, &conf.date_pattern);
  match date {
    None => Ok(false),
    Some(date) => {
      if conf.debug {
        println!("parsed {} to date {}", extracted_date, date);
      }

      let ts_line = utils::get_timestamp(date);
      if oldest_ts > ts_line {
        return Err(SearchError::TimestampTooOld);
      }

      let is_match = conf.re.captures_from_pos(&line, 0).unwrap();
      let is_match = is_match.is_some();
      if is_match && conf.verbose {
        // no println, "\n" is already contained in line
        print!("{}", line);
      }
      Ok(is_match)
    }
  }
}

#[cfg(test)]
mod tests {
  extern crate filetime;
  extern crate tempfile;

  use super::*;
  use self::tempfile::NamedTempFile;
  use self::filetime::FileTime;
  use std::io::Write;
  use std::time::UNIX_EPOCH;
  use time as t;
  use utils::*;

  const DUMMY_SEARCH_PATTERN: &str = ".*";
  const SOME_LOG_FILE: &str = "/tmp/some-file.log";
  const CHECK_LAST_MINUTE: u64 = 1;

  // Adjusts for local timezone
  fn str_to_filetime(format: &str, s: &str) -> FileTime {
    let mut tm = time::strptime(s, format).unwrap();
    tm.tm_utcoff = time::now().tm_utcoff;
    let ts = tm.to_timespec();
    FileTime::from_unix_time(ts.sec as i64, ts.nsec as u32)
  }

  fn get_dummy_conf(interval_to_check: u64, search_pattern: String, logfile: String) -> Config {
    get_dummy_conf_format(interval_to_check, search_pattern, logfile, "".to_owned(), 0)
  }

  fn get_dummy_conf_format(interval_to_check: u64, search_pattern: String, logfile: String, date_pattern: String, timeposition: usize) -> Config {
    Config::new(
      interval_to_check,
      search_pattern,
      logfile,
      1,              // max_critical_matches
      1,              // max_warning_matches
      date_pattern,
      timeposition,
      true ,          // debug is set to true to also test these branches
      true,           // verbose is set to true to also test these branches
    ).unwrap()
  }

  fn create_temp_file(content: &str) -> (NamedTempFile, String) {
    let mut file = NamedTempFile::new().expect("not able to create tempfile");
    if content.len() > 0 {
      writeln!(file, "{}", content).expect("tempfile cannot be written");
    }
    let path = file.path().to_str().expect("oh no").to_string();
    (file, path)
  }

  /// returns approximately the minutes since unix epoch
  fn forever() -> u64 {
    // we subtract the tz offset for los angeles (-7h) because some
    // of the tests use that tz and it is sufficient to return a
    // very old timestamp from this function.
    (get_now_secs() / 60) - (7 * 60)
  }

  fn get_now_secs() -> u64 {
    let now = std::time::SystemTime::now();
    let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
    since_the_epoch.as_secs()
  }

  fn reset_tz() {
    std::env::set_var("TZ", "Africa/Abidjan"); // Africa/Abidjan = +00:00 UTC Offset
    t::tzset();
  }

  #[test]
  fn should_correctly_calculate_oldest_allowed_ts_utc() {
    // given
    let now = std::time::SystemTime::now(); // TODO use a fixed time. this test
                                            // could be flaky in corner cases.
    let interval_to_check: u64 = 1;
    let conf = get_dummy_conf(interval_to_check,
                              DUMMY_SEARCH_PATTERN.to_owned(),
                              SOME_LOG_FILE.to_owned());

    // when
    let oldest_allowed_ts = get_oldest_allowed_utc_ts(&conf, now);

    // then
    let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
    assert_eq!(oldest_allowed_ts, since_the_epoch.as_secs() - (interval_to_check * 60));
  }

  #[test]
  fn should_correctly_calculate_oldest_allowed_ts_adjusted_to_local_tz() {
    // given
    std::env::set_var("TZ", "America/Los_Angeles");
    t::tzset();

    let now = std::time::SystemTime::now();
    let interval_to_check: u64 = 13; // minutes
    let conf = get_dummy_conf(interval_to_check,
                              DUMMY_SEARCH_PATTERN.to_owned(),
                              SOME_LOG_FILE.to_owned());

    // when
    let oldest_ts = get_oldest_allowed_local_ts(&conf, now);

    // then
    // the oldest allowed timestamp in this case should not be
    // `current utc - interval_to_check`, but rather the current
    // time adjusted to `local tz - interval_to_check`.
    let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
    let offset = 7 * 60 * 60; // 7 hours is the timezone offset from utc to los angeles
    assert_eq!(oldest_ts, since_the_epoch.as_secs() - (interval_to_check * 60) - offset);
  }

  #[test]
  fn should_search_in_file() {
    // given
    reset_tz();
    let now_unix_ts = get_now_secs();
    let format = "%b %d %H:%M:%S";

    let dt = NaiveDateTime::from_timestamp(now_unix_ts as i64, 0);
    let now_formatted = dt.format(format).to_string();

    let five_minutes = now_unix_ts - (5 * 60);
    let dt_five_minutes_ago = NaiveDateTime::from_timestamp(five_minutes as i64, 0);
    let five_minutes_ago = dt_five_minutes_ago.format(format).to_string();

    let content = format!("{} foo_bar\n{} bar\n{} foo-bar\n{} foo_bar",
                           five_minutes_ago, now_formatted, now_formatted, now_formatted);
    let (_file, path) = create_temp_file(&content);

    let interval_to_check: u64 = 2;
    let conf = get_dummy_conf_format(interval_to_check, "foo[-_]+bar".to_owned(), path, format.to_owned(), 0);

    // when
    let res = run(&conf);

    // then
    let matches = 2;
    let files_searched = 1;
    assert_eq!(res, Ok((matches, files_searched)));

  }
  #[test]
  fn should_handle_timeposition() {
    // given
    reset_tz();
    let now_unix_ts = get_now_secs();
    let format = "%b %d %H:%M:%S";

    let dt = NaiveDateTime::from_timestamp(now_unix_ts as i64, 0);
    let now_formatted = dt.format(format).to_string();

    let five_minutes = now_unix_ts - (5 * 60);
    let dt_five_minutes_ago = NaiveDateTime::from_timestamp(five_minutes as i64, 0);
    let five_minutes_ago = dt_five_minutes_ago.format(format).to_string();

    let content = format!("foo_bar {}\nbar {}\nfoo-bar {}\nfoo_bar {}",
                          five_minutes_ago, now_formatted, now_formatted, now_formatted);
    let (_file, path) = create_temp_file(&content);

    let interval_to_check: u64 = 2;
    let conf = get_dummy_conf_format(interval_to_check, "foo[-_]+bar".to_owned(), path, format.to_owned(), 1);

    // when
    let res = run(&conf);

    // then
    let matches = 2;
    let files_searched = 1;
    assert_eq!(res, Ok((matches, files_searched)));
  }

  #[test]
  fn should_handle_empty_file_correctly() {
    // given
    let (_file, path) = create_temp_file("");
    let conf = get_dummy_conf(CHECK_LAST_MINUTE, DUMMY_SEARCH_PATTERN.to_owned(), path);

    // when
    let res = run(&conf);

    // then
    let matches = 0;
    let files_searched = 0;
    assert_eq!(res, Ok((matches, files_searched)));
  }

  #[test]
  fn should_skip_binary_files() {
    // given
    let path = "./fixtures/1x1.png";
    let conf = get_dummy_conf(forever(), DUMMY_SEARCH_PATTERN.to_owned(), path.to_owned());
    let whitespaces_in_date = 3; // doesn't matter, should not be considered anyway
    let oldest_ts = forever();

    // when
    let res = search_file(path, &conf, whitespaces_in_date, oldest_ts);

    // then
    let files_searched = 0;
    assert_eq!(res, Err((SearchError::NotUtf8, files_searched)));
  }

  #[test]
  fn should_handle_utf8_file_content_correctly() {
    // given
    let (_file, path) = create_temp_file("2018-09-13 00:03:01 🐱");
    let conf = get_dummy_conf(forever(), "🐱".to_owned(), path);

    // when
    let res = run(&conf);

    // then
    let matches = 1;
    let files_searched = 1;
    assert_eq!(res, Ok((matches, files_searched)));
  }

  #[test]
  fn should_handle_files_with_lines_without_dates() {
    // given
    // one of the lines intentionally contains as many whitespaces as the
    // valid lines which include a date.
    let (_file, path) = create_temp_file("2018-09-13 00:03:01 foo\nsome\nsome some\nsome foo bar\n2018-09-13 00:03:01 bar\n");
    let conf = get_dummy_conf(forever(), "bar".to_owned(), path);

    // when
    let res = run(&conf);

    // then
    let matches = 1;
    let files_searched = 1;
    assert_eq!(res, Ok((matches, files_searched)));
  }

  #[test]
  fn should_ignore_trailing_comma() {
    // given
    let format = "%Y-%m-%d %H:%M:%S";
    let (_file, path) = create_temp_file("2018-09-13 00:01:51,079 foo\n2018-09-13 00:01:51,079 foobar\n");
    let conf = get_dummy_conf_format(forever(), "foo".to_owned(), path, format.to_owned(), 0);

    // when
    let res = run(&conf);

    // then
    let matches = 2;
    let files_searched = 1;
    assert_eq!(res, Ok((matches, files_searched)));
  }

  #[test]
  fn should_handle_non_default_date_format() {
    // given
    reset_tz();
    let now_unix_ts = get_now_secs();
    let format = "%b %d %H:%M:%S";

    let dt = NaiveDateTime::from_timestamp(now_unix_ts as i64, 0);
    let now_formatted = dt.format(format).to_string();

    let five_minutes = now_unix_ts - (5 * 60);
    let dt_five_minutes_ago = NaiveDateTime::from_timestamp(five_minutes as i64, 0);
    let five_minutes_ago = dt_five_minutes_ago.format(format).to_string();

    let content = format!("{} foo\n{} bar\n{} foobar",
                           five_minutes_ago, now_formatted, now_formatted);
    let (_file, path) = create_temp_file(&content);

    let interval_to_check: u64 = 2;
    let conf = get_dummy_conf_format(interval_to_check, "foo".to_owned(), path, format.to_owned(), 0);

    // when
    let res = run(&conf);

    // then
    // the entry which was five minutes ago should not be matched
    let matches = 1;
    let files_searched = 1;
    assert_eq!(res, Ok((matches, files_searched)));
  }

  #[test]
  fn should_handle_non_default_date_format_and_trailing_comma_and_different_timeposition() {
    // given
    reset_tz();
    let now_unix_ts = get_now_secs();
    let format = "%b %d %H:%M:%S";

    let dt = NaiveDateTime::from_timestamp(now_unix_ts as i64, 0);
    let now_formatted = dt.format(format).to_string();

    let five_minutes = now_unix_ts - (5 * 60);
    let dt_five_minutes_ago = NaiveDateTime::from_timestamp(five_minutes as i64, 0);
    let five_minutes_ago = dt_five_minutes_ago.format(format).to_string();

    let content = format!("foo {},123\nbar{},345\nfoo {},567",
                           five_minutes_ago, now_formatted, now_formatted);
    let (_file, path) = create_temp_file(&content);

    let interval_to_check: u64 = 2;
    let conf = get_dummy_conf_format(interval_to_check, "foo".to_owned(), path, format.to_owned(), 1);

    // when
    let res = run(&conf);

    // then
    // the entry which was five minutes ago should not be matched
    let matches = 1;
    let files_searched = 1;
    assert_eq!(res, Ok((matches, files_searched)));
  }

  #[test]
  fn should_skip_old_files() {
    // given
    let (file, path) = create_temp_file("2018-09-13 00:03:01 foo");
    let five_minutes = 5;
    let conf = get_dummy_conf(five_minutes, "foo".to_owned(), path);

    let start_of_year = str_to_filetime("%Y%m%d%H%M", "201501010000");
    let path = file.path();
    filetime::set_file_times(path, start_of_year, start_of_year).unwrap();

    // when
    let res = run(&conf);

    // then
    let matches = 0;
    let files_searched = 0;
    assert_eq!(res, Ok((matches, files_searched)));
  }

  #[test]
  fn should_handle_file_age_correctly() {
    // given
    let (file, path) = create_temp_file("2018-09-13 00:03:01 foo");
    let conf = get_dummy_conf(forever(), "foo".to_owned(), path);
    let start_of_year = str_to_filetime("%Y%m%d%H%M", "201809130000");

    let path = file.path();
    filetime::set_file_times(path, start_of_year, start_of_year).unwrap();

    // when
    let res = run(&conf);

    // then
    let matches = 1;
    let files_searched = 1;
    assert_eq!(res, Ok((matches, files_searched)));
  }

  #[test]
  fn should_search_matching_files() {
    // given
    // logfile.0 should also be searched
    let conf = get_dummy_conf(forever(), "foobar".to_owned(), "./fixtures/logfile".to_owned());

    // when
    let res = run(&conf);

    // then
    let matches = 2;
    let files_searched = 2;
    assert_eq!(res, Ok((matches, files_searched)));
  }

  #[test]
  fn should_abort_when_stdin_used_as_logfile() {
    // given
    let stdin = "-".to_owned();

    // when
    let conf = Config::new(
      forever(),
      "foobar".to_owned(),
      stdin,
      1,              // max_critical_matches
      1,              // max_warning_matches
      "".to_owned(),  // datepattern
      0,              // timeposition
      true,           // debug is set to true to also test these branches
      true,           // verbose is set to true to also test these branches
    );

    // then
    assert_eq!(conf.is_err(), true);
  }

}
