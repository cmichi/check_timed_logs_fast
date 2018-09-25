extern crate memmap;
extern crate chrono;
extern crate time;
extern crate glob;
extern crate fancy_regex;

use std::process::exit;
use std::fs::File;
use std::str;
use memmap::Mmap;
use fancy_regex::Regex;

use chrono::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};
use std::fs;
use glob::glob;

pub struct Config {
  interval_to_check: u64,
  search_pattern: String, // TODO probably no longer necessary? or still used in prints?
  filename: String,

  // TODO is usize sufficient?
  max_critical_matches: u64,
  max_warning_matches: u64,
  date_pattern: String,
  timeposition: usize,
  debug: bool,
  verbose: bool,
  re: Regex,
}

impl Config {
  pub fn new(
    interval_to_check: u64,
    search_pattern: String,
    filename: String,

    max_critical_matches: u64,
    max_warning_matches: u64,
    date_pattern: String,
    timeposition: usize,
    debug: bool,
    verbose: bool,
  ) -> Config {
    Config {
      interval_to_check,
      search_pattern: search_pattern.to_owned(),
      filename,

      max_critical_matches,
      max_warning_matches,
      date_pattern,
      timeposition,
      debug,
      verbose,
      re: Regex::new(&search_pattern.to_owned()).unwrap(),
    }
  }
}

fn get_oldest_allowed_utc_ts(conf: &Config, now: std::time::SystemTime) -> u64 {
  let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
  let now_unix_ts = since_the_epoch.as_secs();
  let go_back_secs = 60 * conf.interval_to_check;

  if go_back_secs > now_unix_ts {
    0
  } else {
    now_unix_ts - go_back_secs
  }
}

fn get_oldest_allowed_local_ts(conf: &Config, now: std::time::SystemTime) -> u64 {
  let oldest_ts_utc = get_oldest_allowed_utc_ts(conf, now);
  let oldest_date_no_tz_offset = NaiveDateTime::from_timestamp(oldest_ts_utc as i64, 0); // TODO i64?!
  let adjusted_date = adjust_to_local_tz(oldest_date_no_tz_offset); 
  get_timestamp_from_local(adjusted_date)
}

pub fn run(conf: &Config) {
  let mut files_matched = 0;
  let mut exp = conf.filename.to_owned();
  let star = String::from("*");
  exp.push_str(&star);

  let mut matches = 0;

  let pattern_spaces: Vec<&str> = conf.date_pattern.split_whitespace().collect();
  let whitespaces_in_date = pattern_spaces.len(); // = count of whitespaces
  
  if conf.debug {
    println!("looking for files matching {}", exp);
  }

  // the ts is adjusted to local time
  let now = SystemTime::now();
  let oldest_ts = get_oldest_allowed_local_ts(conf, now);

  if conf.debug {
    let oldest_date_no_tz_offset = NaiveDateTime::from_timestamp(get_oldest_allowed_utc_ts(conf, now) as i64, 0);
    let adjusted_date = NaiveDateTime::from_timestamp(get_oldest_allowed_local_ts(conf, now) as i64, 0);
    println!("oldest allowed date in utc: {} and with tz offset: {}", oldest_date_no_tz_offset, adjusted_date);
  }
  
  // for all files that match pattern
  for entry in glob(&exp).expect("failed to read glob pattern") {
    match entry {
      Ok(path) => {
        files_matched += 1;

        let p = path.to_str().unwrap();
        /*
        // TODO this is dangerous, since we don't check if it actually is a suffix
        if p.contains(".gz") {
          if conf.debug {
            println!("skipping {:?} because zipped", conf.filename);
          }
          continue;
        }
        */

        if !check_file_age(&conf, p) {
          if conf.debug {
            println!("skipping {:?} because too old", conf.filename);
          }
          continue; 
        }

        if p == "-" {
          panic!("ERROR: stdin as path is not supported");
        }

        let local_matches = process_file(p, &conf, whitespaces_in_date, oldest_ts);
        matches += local_matches;
      },
      Err(e) => eprintln!("ERROR: {:?}", e),
    }
  }

  if matches >= conf.max_critical_matches {
    eprintln!("CRITICAL - There are {} instances of \"{}\" in the last {} minutes",
              matches, conf.search_pattern, conf.interval_to_check);
    exit(1);
  }

  if matches >= conf.max_warning_matches {
    eprintln!("WARNING - There are {} instances of \"{}\" in the last {} minutes",
              matches, conf.search_pattern, conf.interval_to_check);
    exit(1);
  }

  if files_matched == 0 {
    eprintln!("UNKNOWN - There were no files matching the passed filename: \"{}\"",
              conf.filename);
    exit(3);
  }

  println!("OK - There are only {} instances of \"{}\" in the last {} minutes - Warning threshold is {:?}",
           matches, conf.search_pattern, conf.interval_to_check, conf.max_warning_matches);
  exit(0);
}


fn process_file(path: &str, conf: &Config, whitespaces_in_date: usize, oldest_ts: u64) -> u64 {
  let mmap;
  let mut matches = 0;

  let file_in = File::open(path).unwrap();
  let metadata = file_in.metadata().unwrap();
  if !metadata.is_file() {
      if conf.debug {
        eprintln!("{} is not a file", path);
      }
      return 0;
  } else if metadata.len() > isize::max_value() as u64 {
    eprintln!("the file {} is too large to be safely mapped to memory: 
               https://github.com/danburkert/memmap-rs/issues/69", path);
  } else if metadata.len() == 0 {
    if conf.debug {
      eprintln!("{} is empty", path);
    }
    return 0;
  } 

  let (file, len) = {
    mmap = Mmap::open_path(path, memmap::Protection::Read).unwrap();
    let bytes = unsafe { mmap.as_slice() };
    (bytes, mmap.len())
  };

  let mut last_printed = len as i64;
  let mut index = last_printed - 1;
  while index >= -1 {
    if index == -1 || file[index as usize] == '\n' as u8 {
      let drin = process_line(&file[(index + 1) as usize..last_printed as usize],
                              whitespaces_in_date, oldest_ts, &conf);
      match drin {
        Ok(v) => {
          if v {
            matches += 1;
          }
        },
        Err(_) => {
          // if error because of ascii file, then just skip without outputting error
          break;
        }
      }

      last_printed = index + 1;
    }

    index -= 1;
  }

  matches
}

// checks if the file age is >= now - interval_to_check
fn check_file_age(conf: &Config, path: &str) -> bool {
  let secs_allowed = conf.interval_to_check * 60;

  let attr = fs::metadata(&path).unwrap();
  let last_modified = attr.modified().unwrap();
  let elapsed_secs = last_modified.elapsed().unwrap().as_secs();

  if conf.debug {
    println!("found file {} is {} seconds old", path, elapsed_secs);
  }

  if elapsed_secs <= secs_allowed {
    return true;
  }

  false
}

fn adjust_to_local_tz(date: NaiveDateTime) -> DateTime<chrono::Local> {
  let dt = chrono::Local::now();
  let local_offset = dt.offset();

  // convert from utc to local time
  let off = TimeZone::from_offset(local_offset);
  DateTime::<chrono::Local>::from_utc(date, off)
}

fn parse_date(datefields: &str, pattern: &str) -> Option<DateTime<Utc>> {
  // if does not start with 4 digits then return None
  let p = match Utc.datetime_from_str(&datefields, pattern) {
    Ok(v) => v,
    Err(e) => {
      // there are a few things we can try to fix the error
      let err_desc = e.to_string();
      if err_desc == "trailing input" {
        // the original check_timed_logs.pl would just ignore the trailing input,
        // but unfortunately chrono does not support ignoring trailing input.
        // hence this hack.
        let comma_pos = datefields.find(',').unwrap_or(datefields.len());
        let (before_comma, _) = datefields.split_at(comma_pos);
        return parse_date(&before_comma, pattern);
      }

      // try prepending the year, for many logs the year is missing
      let mut new_pattern = String::from("%Y ");
      new_pattern.push_str(&pattern);

      let mut datestring = String::from("2018 ");
      datestring.push_str(&datefields);

      match Utc.datetime_from_str(&datestring, &new_pattern) {
        Ok(v) => v,
        Err(_) => {
          // a typical reason for not being able to parse the date is that
          // there is a stacktrace which spans multiple lines in the log
          //eprintln!("This error appeared when parsing the date in the log file with the provided pattern: {:?}. The date fields: {:?}, the pattern: {:?}.", e, datefields, pattern);
          return None;
        },
      }
    },
  };

  Some(p)
}

fn get_timestamp_from_local(date: DateTime<chrono::Local>) -> u64 {
  date.naive_local().timestamp() as u64
}

fn get_timestamp(date: DateTime<chrono::Utc>) -> u64 {
  date.naive_local().timestamp() as u64
}

fn process_line(bytes: &[u8], whitespaces_in_datefields: usize, oldest_ts: u64, conf: &Config) -> Result<bool, ()> {
  if bytes.len() == 0 {
    return Ok(false);
  }

  let l = str::from_utf8(bytes);
  if l.is_err() {
      if conf.debug {
        //eprintln!("skipping file because not ascii!");
        // TODO return string with err
      }
      return Err(());
  }
  let line = l.unwrap().trim();
  if line.len() == 0 {
    return Ok(false);
  }

  if conf.debug {
    println!("processing line: {}", line);
  }

  let split: Vec<&str> = line.split_whitespace().collect();
  let datefields = split.get(conf.timeposition..whitespaces_in_datefields).unwrap().join(" ");

    let comma_pos = datefields.find(',').unwrap_or(datefields.len());
    let (before_comma, _) = datefields.split_at(comma_pos);

  //let date = parse_date(&datefields, &conf.date_pattern);
  let date = parse_date(&before_comma, &conf.date_pattern);
  match date {
    None => Ok(false),
    Some(date) => {
      if conf.debug {
        println!("parsed {} to date {}", datefields, date);
      }

      let ts = get_timestamp(date);
      if ts < oldest_ts {
        // TODO return string with err
        return Err(());
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
  use super::*;
  use time as t;

  #[test]
  fn should_prepend_current_year() {
    // given
    let pattern = "%b %d %H:%M:%S";
    let datefields = "Aug 8 11:28:21";

    // when
    let date = parse_date(datefields, pattern);

    // then
    let ts = date.unwrap().timestamp() as u64;
    assert_eq!(ts, 1533727701);
  }

  #[test]
  fn should_not_prepend_year_if_already_present() {
    // given
    let pattern = "%Y %b %d %H:%M:%S";
    let datefields = "2018 Aug 8 11:28:21";

    // when
    let date = parse_date(datefields, pattern);

    // then
    let ts = date.unwrap().timestamp() as u64;
    assert_eq!(ts, 1533727701);
  }

  #[test]
  fn should_correctly_calculate_oldest_allowed_ts_utc() {
    // given
    let now = std::time::SystemTime::now(); // TODO mock a fixed time
    let mins: u64 = 1;
    let conf = Config {  // TODO use ::new
      interval_to_check: mins,

      search_pattern: "".to_owned(),
      filename: "".to_owned(),
      max_critical_matches: 1,
      max_warning_matches: 1,
      date_pattern: "".to_owned(),
      timeposition: 1,
      debug: false,
      verbose: false,
      re: Regex::new("").unwrap(),
    };

    // when
    let oldest_ts = get_oldest_allowed_utc_ts(&conf, now);

    // then
    let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
    assert_eq!(oldest_ts, since_the_epoch.as_secs() - (mins * 60));
  }

  #[test]
  fn should_correctly_calculate_oldest_allowed_ts_adjusted_to_local_tz() {
    // given
    std::env::set_var("TZ", "America/Los_Angeles");
    t::tzset();

    let now = std::time::SystemTime::now(); // TODO mock a fixed time
    let interval_to_check: u64 = 13; // minutes
    let conf = Config {
      interval_to_check: interval_to_check,

      search_pattern: "".to_owned(),
      filename: "".to_owned(),
      max_critical_matches: 1,
      max_warning_matches: 1,
      date_pattern: "".to_owned(),
      timeposition: 1,
      debug: false,
      verbose: false,
      re: Regex::new("").unwrap(),
    };

    // when
    let oldest_ts = get_oldest_allowed_local_ts(&conf, now);

    // then
    // the oldest allowed timestamp in this case should not be current utc - interval_to_check
    // but rather current time adjusted to local tz - interval_to_check
    let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
    let offset = 7 * 60 * 60; // 7 hours
    assert_eq!(oldest_ts, since_the_epoch.as_secs() - (interval_to_check * 60) - offset);
  }

  #[test]
  fn should_parse_everything_even_when_lines_do_not_contain_dates() {
    // given

    // when
    
    // then
  }


  #[test]
  fn should_ignore_trailing_comma() {
    // given

    // when
    
    // then
  }

  #[test]
  fn should_handle_file_age_correctly() {
    // given

    // when
    
    // then
  }
}
