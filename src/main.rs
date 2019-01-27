extern crate check_timed_logs_fast;

use std::process::exit;

mod args;

fn main() {
  let parse = args::parse();
  let conf = match parse {
    Err(err) => {
      println!("ERROR while parsing the arguments: {}.\nUse `-help` to show usage information.", err);
      exit(3);
    },
    Ok(conf) => {
      conf
    }
  };

  let res = check_timed_logs_fast::run(&conf);
  match res {
    Err(err) => {
      println!("ERROR: {}", err);
      exit(3);
    },
    Ok((matches, files_matched)) => {
      if matches >= conf.max_critical_matches {
        println!("CRITICAL - There are {} instances of \"{}\" in the last {} minutes",
                  matches, conf.search_pattern, conf.interval_to_check);
        exit(2);
      }

      if matches >= conf.max_warning_matches {
        println!("WARNING - There are {} instances of \"{}\" in the last {} minutes",
                  matches, conf.search_pattern, conf.interval_to_check);
        exit(1);
      }

      if files_matched == 0 {
        println!("UNKNOWN - There were no files matching the passed filename: \"{}\"",
                  conf.logfile);
        exit(3);
      }

      println!("OK - There are only {} instances of \"{}\" in the last {} minutes - Warning threshold is {:?}",
               matches, conf.search_pattern, conf.interval_to_check, conf.max_warning_matches);
      exit(0);
    }
  }
}
