extern crate chrono;
extern crate std;

use Config;
use chrono::prelude::*;
use std::fs;
use std::str;
use std::time::UNIX_EPOCH;

pub fn get_oldest_allowed_utc_ts(conf: &Config, now: std::time::SystemTime) -> u64 {
  let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
  let now_unix_ts = since_the_epoch.as_secs();
  let go_back_secs = 60 * conf.interval_to_check;

  if go_back_secs > now_unix_ts {
    0
  } else {
    now_unix_ts - go_back_secs
  }
}

pub fn get_oldest_allowed_local_ts(conf: &Config, now: std::time::SystemTime) -> u64 {
  let oldest_ts_utc = get_oldest_allowed_utc_ts(conf, now);
  let oldest_date_no_tz_offset = NaiveDateTime::from_timestamp(oldest_ts_utc as i64, 0); // TODO i64?!
  let adjusted_date = adjust_to_local_tz(oldest_date_no_tz_offset);
  get_timestamp_from_local(adjusted_date)
}

/// check if the file age is >= now - interval_to_check
pub fn check_file_age(conf: &Config, path: &str) -> bool {
  let secs_allowed = conf.interval_to_check * 60;

  let attr = fs::metadata(&path).expect("cannot get metadata");
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

pub fn adjust_to_local_tz(date: NaiveDateTime) -> DateTime<chrono::Local> {
  let dt = chrono::Local::now();
  let local_offset = dt.offset();

  // convert from utc to local time
  let off = TimeZone::from_offset(local_offset);
  DateTime::<chrono::Local>::from_utc(date, off)
}

pub fn parse_date(datefields: &str, pattern: &str) -> Option<DateTime<Utc>> {
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
          // if it's still not possible to parse a date from the line we just
          // ignore the line.
          // eprintln!("This error appeared when parsing the date in the log
          //            file with the provided pattern: {:?}. The date fields:
          //            {:?}, the pattern: {:?}.", e, datefields, pattern);
          return None;
        },
      }
    },
  };

  Some(p)
}

pub fn get_timestamp_from_local(date: DateTime<chrono::Local>) -> u64 {
  date.naive_local().timestamp() as u64
}

pub fn get_timestamp(date: DateTime<chrono::Utc>) -> u64 {
  date.naive_local().timestamp() as u64
}

#[cfg(test)]
mod tests {
  use super::*;

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

}
