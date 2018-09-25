#!/usr/bin/perl
##############################################################################
#
# NAME:         check_timed_logs.pl
#
# AUTHOR:       Gerd Radecke
#
# COMMENT:  Script searches a text file for the appearance of a given RegEx within a given time period.
# 			Using additional parameters you can adjust: Time string format, 
#			time string position, number of pattern matches required to be "successful".
#
#           Return Values for NRPE:
#           	OK - There are only 0 instances of $pattern in the last $interval minutes (0)
#           	CRITICAL - There are $hits instances of \"$pattern\" in the last $interval minutes (2)
#           	WARNING - There are $hits instances of \"$pattern\" in the last $interval minutes (1)
#           	UNKNOWN - There were no files matching the passed filename (3)
#
# REQUIRES: 	perl-Time-Piece perl-File-ReadBackwards 
#				ON RHEL-based systems you can run: yum install perl-Time-Piece perl-File-ReadBackwards
#
# CHANGELOG:
# 1.0 2013-02-19 - initial version
# 1.0.1 2013-02-27 - fixed false variable reference
# 1.0.2 2013-10-07 - integrated threshold comparison fix by Christoph Tavan - thanks ;)
#
##############################################################################


use File::ReadBackwards; # EPEL RPM: perl-File-ReadBackwards.noarch 
use Getopt::Long;
use Time::Piece; # RHEL package: perl-Time-Piece
use File::Find;

$time_pattern = '%Y-%m-%d %H:%M:%S';
$warning = 1;
$critical = 1;

$time_position = 0;
$result = GetOptions (
            "pattern=s" => \$pattern, # string e.g. "CRITICAL"
            "logfile=s" => \$logfile, # string e.g. "/var/log/messages" 
            "interval=i" => \$interval, # int e.g. 30 for half an hour
            "timepattern=s" => \$time_pattern, #string e.g. '%Y-%m-%d %H:%M:%S'
			"timeposition=i" => \$time_position, # int, each line is split into string on the space character, this provides the index of the first string block for the time
            "warning|w=i" => \$warning, # int e.g. 3
			"critical|c=i" => \$critical, # int e.g. 5
			"debug|d|vv" => \$debug, # flag/boolean
			"verbose|v" => \$verbose, # flag/boolean
            "help|h|?" => \$usage # flag/boolean  - is help called?
            ); 
            
print $count;
if ($usage || !(defined($pattern) && $pattern ne "") || !(defined($logfile) && $logfile ne "") || !(defined($interval) && $interval gt 0 )) {
    print "\nUsage: $0
		\t -pattern <regex-pattern> 
		\t -logfile <path to log file> 
		\t -interval <minutes> 
		\t [-timepattern <POSIX time pattern>] 
		\t [-warning|w <number_of_required_hits>] [-critical|c <number_of_required_hits>] 
		\t [-timeposition <time_string_index_on_line>] \n\n";
	print "To allow for rotating logfiles, any file that matches the passed filename and was changed within the passed interval is checked. e.g. If you pass /var/log/applog, this could match /var/log/applog.0, /var/log/applog.old and so on. However, it does not handle compressed (e.g. gzip/bzip) files. \n\n";
    print "Default time pattern is: %Y-%m-%d %H:%M:%S  => 2012-12-31 17:20:40\n";
	print "Example Time patterns (from a RHEL system):
			BSD/Syslog: %b %d %H:%M:%S => Dec 31 17:20:40
			Apache Logs: %d/%b/%Y:%H:%M:%S (with -timeposition 3) => 31/Dec/2012:17:20:40
			Websphere Logs: %d-%b-%Y %I:%M:%S %p => 31-Dec-2012 05:20:40 PM
			Nagios logs: %s => 1361260238 (seconds since 01-01-1970) \n";
	print "For a posix time format documentation check out: http://linux.die.net/man/3/strftime \n\n";
    print "Default warning/critical threshold of pattern matches to find is: 1 -> unless you change this, you will only get OK or CRITICAL, but never WARNING\n\n";
	print "Default time position is 0 \n";
	print "\t Time Position: each line is split into an array of strings on the space character, this provides the index for the first time string.\n";
	print "\t Note: If the line starts with the time, that means we start at index 0.\n\n";
    print "The values for interval and warning/critical need to be larger than zero \n";
    exit;
}

my $now = localtime;

$oldestDate = $now - $interval*60;
if ($debug) { print "Now: $now and tzoffset: ". ($now)->tzoffset ."\n"; }
if ($debug) { print "Oldest date: $oldestDate and tzoffset: ". ($oldestDate)->tzoffset ."\n"; }


$hits = 0; # number of matches for the regex within the log files will be counted in this variable
$validFileNames = 0; # number of files that match the given filename
my @dateFields = $time_pattern =~ / /g; #  how many spaces do we have in our time pattern?
my $dateFieldsCount = @dateFields; # count the number spaces in the date format

if ($debug) { 
$verbose = 1; # if we debug, we want to have all information
print "Interval: $interval equals " . ($interval/1440) . " Fraction of days.\n";
}


$logfile=~m/^.+\//; 
$DIR=$&; # greedy matching from theline above

@files = find(\&process, $DIR);
sub process {

### note the following is done for each file that is found and matches the name and date criteria
	if ($File::Find::name =~ m/$logfile/ && (-T)) { # match only files that are ASCII files (-T) and that contain the file name
		$validFileNames += 1;
		if ($debug) {  print "Found: $File::Find::name has age " . (-M) ." (in Fraction of days) \n"; }

		# -M returns the last change date of the file in fraction of days. e.g. 24 ago -> 1, 6 hours ago -> 0.25
		if ((-M) < ($interval/1440)) {  # match only files whose last change (-M) is within the change interval
										# perldoc defines -M : Script start time minus file modification time, in days.

		$LOGS = File::ReadBackwards->new($File::Find::name) or
			die "Can't read file: $File::Find::name\n";

		while (defined($line = $LOGS->readline) ) {
			my @fields = split ' ', $line; # split the line into an array, split on ' '(space)
			$dateString = ""; # reset the datestring for each line
			for ($i=0; $i <= $dateFieldsCount; $i++) {
				$dateString .= $fields[$time_position + $i] . " "; # concatenate all date strings into one parseable string
			}
			$dateString =~ s/^\s+|\s+$//g ; # remove both leading and tailing whitespace - perl 6 will have a trim() function, until then - regex !
			$dateString =~ s/<|>|\]|\[//g ; # remove brackets
			#if ($debug) { print "Datestring: $dateString \n";} # this is only needed if you are unsure which strings of the array are part of your datestring
			
			my $dt =  Time::Piece->strptime($dateString, $time_pattern); # parse string into Time::Piece object
			my $dt_tzadjusted = ($dt - $now->tzoffset); # TIME::PIECE assumes the parsed dates will be UTC, we need to adjust to the local tz offset
			
			# some date formats don't have the year information e.g. Dec 31 15:50:57 -> the year would automatically be parsed to 1970, 
			# which is probably never correct. We will correct this to this or last year
			if ($dt->year eq 1970) { 
				$dt = $dt->add_years($now->year - 1970); # We cannot set the year directly. So we add the number of years that have passed since 1970. 
				$dt_tzadjusted = ($dt - $now->tzoffset);
				# NOTE: If $now is January 1st and we're looking at log files from the end of last year, we will add too many years
				# hence if the date is now in the future, we subtract one year again.
				if ($dt_tzadjusted > $now) { 
					$dt = $dt->add_years(-1);
					$dt_tzadjusted = ($dt - $now->tzoffset);
				}
			}

			if ($dt_tzadjusted > $oldestDate) { # is the date bigger=>newer than the oldest date we want to look at?
				if ($line =~ m/$pattern/){ # if the line contains the regex pattern
					if ($debug) {print $dt . " => "; }
					if ($verbose) { print $line; }
					$hits++; # increase by 1 hit
				}
			}
			else{
				last; #if the date is older than the oldest we still care about, leave this loop -> go to the next file if available
			}
		}

		close(LOGS);
		}

	}
}## the find sub process ends here



if ($hits >= ($critical + 0)) {
    print "CRITICAL - There are $hits instances of \"$pattern\" in the last $interval minutes\n";
    exit 2; }
if ($hits >= ($warning + 0)) {
    print "WARNING - There are $hits instances of \"$pattern\" in the last $interval minutes\n";
    exit 1; }
if ($validFileNames == 0) {
	print "UNKNOWN - There were no files matching the passed filename: \"$logfile\"\n";
	exit 3; }
else {
    print "OK - There are only $hits instances of \"$pattern\" in the last $interval minutes - Warning threshold is $warning\n";
    exit 0;
}
