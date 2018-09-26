extern crate std;

use check_timed_logs_fast::Config;

fn print_usage(program: &str) {
  let brief = format!("Usage: {}
    -pattern <regex-pattern>
    -logfile <path to log file>
    -interval <minutes>
    [-timepattern <POSIX time pattern>]
    [-warning|w <number_of_required_hits>] [-critical|c <number_of_required_hits>]
    [-timeposition <time_string_index_on_line>]

    To allow for rotating logfiles, any file that matches the passed filename and
    was changed within the passed interval is checked. e.g. If you pass /var/log/applog,
    this could match /var/log/applog.0, /var/log/applog.old and so on. However, it does
    not handle compressed (e.g. gzip/bzip) files.

    Default time pattern is: %Y-%m-%d %H:%M:%S  => 2012-12-31 17:20:40
    Example Time patterns (from a RHEL system):
      BSD/Syslog: %b %d %H:%M:%S => Dec 31 17:20:40
      Apache Logs: %d/%b/%Y:%H:%M:%S (with -timeposition 3) => 31/Dec/2012:17:20:40
      Websphere Logs: %d-%b-%Y %I:%M:%S %p => 31-Dec-2012 05:20:40 PM
      Nagios logs: %s => 1361260238 (seconds since 01-01-1970)

    For a posix time format documentation check out:
    http://linux.die.net/man/3/strftime

    Default warning/critical threshold of pattern matches to find is: 1 -> unless you
    change this, you will only get OK or CRITICAL, but never WARNING.

    Default time position is 0
    Time Position: each line is split into an array of strings on the space character,
    this provides the index for the first time string.
    Note: If the line starts with the time, that means we start at index 0.

    The values for interval and warning/critical need to be larger than zero.", program);
  println!("{}", &brief);
}

fn print_version() {
  const VERSION: &'static str = env!("CARGO_PKG_VERSION");
  println!("{}", VERSION);
}

// the selfmade parsing is necessary because the original plugin uses `-`
// instead of `--` for the flags. the getopts crate only supports `--` though.
pub fn parse() -> Config {
  let mut interval_to_check: u64 = 0;
  let mut search_pattern: String = String::from("");
  let mut logfile: String = String::from("");

  let mut max_critical_matches = 1;
  let mut max_warning_matches = 1;
  let mut date_pattern = String::from("%b %d %H:%M:%S");
  let mut timeposition = 0; // TODO
  let mut debug = false; // TODO
  let mut verbose = false;

  let args: Vec<String> = std::env::args().collect();
  let mut prior_arg = ""; // TODO something cleaner, maybe not build a string here
  for arg in args.iter().skip(1).map(|s| s.as_str()) {
    match arg {
      "-h" | "-help" => {
          print_usage(&args[0]);
          std::process::exit(0);
      },
      "-version" => {
          print_version();
          std::process::exit(0);
      },
      "-d" | "-debug" => {
        debug = true;
      },
      "-v" | "-verbose" => {
        verbose = true;
      },
      &_ => {
        // if the current argument can not be matched
        // let's look if it is a value for a preceding flag
        match prior_arg {
          "-l" | "-logfile" => {
            logfile = arg.clone().to_string();
          },
          "-p" | "-pattern" => {
            search_pattern = arg.clone().to_string();
          },
          "-i" | "-interval" => {
            interval_to_check = arg.parse().unwrap_or_else(|e| {
              eprintln!("ERROR: \"-interval {}\" can not be parsed due to {:?}", arg, e);
              std::process::exit(3);
            });
          },
        
          "-w" | "-warning" => {
            max_warning_matches = arg.parse().unwrap_or_else(|e| {
              eprintln!("ERROR: \"-warning {}\" can not be parsed due to {:?}", arg, e);
              std::process::exit(3);
            });
          },
          "-c" | "-critical" => {
            max_critical_matches = arg.parse().unwrap_or_else(|e| {
              eprintln!("ERROR: \"-critical {}\" can not be parsed due to {:?}", arg, e);
              std::process::exit(3);
            });
          },
          "-timepattern" => {
            date_pattern = arg.clone().to_string();
          },
          "-timeposition" => {
            timeposition = arg.parse().unwrap_or_else(|e| {
              eprintln!("ERROR: \"-timeposition {}\" can not be parsed due to {:?}", arg, e);
              std::process::exit(3);
            });
          },
          &_ => {
            // unexpected arguments don't crash the program, as they also don't crash
            // the original script.
          }
        }

        prior_arg = arg;
      },
    }
  }

  if logfile.is_empty() {
    eprintln!("no -logfile");
    std::process::exit(3);
  }
  if search_pattern.is_empty() {
    eprintln!("no -pattern");
    std::process::exit(3);
  }
  if interval_to_check < 1 {
    eprintln!("interval needs to be set and be >= 1");
    std::process::exit(3);
  }

  let conf = Config::new(
    interval_to_check,
    search_pattern,
    logfile,

    max_critical_matches,
    max_warning_matches,
    date_pattern,
    timeposition,
    debug,
    verbose,
  );
  conf
}
