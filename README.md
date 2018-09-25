# check_timed_logs_fast

[![Build Status](https://travis-ci.org/cmichi/check_timed_logs_fast.svg?branch=master)](https://travis-ci.org/cmichi/check_timed_logs_fast)
[![codecov](https://codecov.io/gh/cmichi/check_timed_logs_fast/branch/master/graph/badge.svg)](https://codecov.io/gh/cmichi/check_timed_logs_fast)

__Project Status:__ It works, I'm working on adding more tests and structuring
the code better.

This is a blazingly fast reimplementation of the [check_timed_logs](https://exchange.nagios.org/directory/Plugins/Log-Files/check_timed_logs/details)
nagios plugin in Rust (the original is in Perl). The API stayed the same,
so you can just replace the original perl script with the binary.

The purpose of the plugin is to monitor log files and alert if there
are more than X occurrences of a regex in the last Y minutes (e.g. more
than one exception in the last minute or more than 5 warnings in the last
two minutes).  

In the company where I work we had problems with very large and verbose
log files. The plugin took a long time for parsing and nagios times out
after a few seconds of getting no reaction from a check — this
then falsely shows up as a critical incident in monitoring.


## Installation

Follows.


## Performance

| Log File Size     | 1.2M      | 37M        | 5.7G       |
| ------------------|-----------|------------|----------- |
| Original          | 0.878 sec | 20.287 sec | >30 min    |
| Rust Rewrite      | 0.031 sec | 0.676 sec  | 83.088 sec |
| Improvement       | 96.4 %    | 96.6 %     |            |

These metrics provide only a rough idea, I haven't looked in detail
at the exact difference in RAM usage (it doesn't seem to have increased
though). The performance is also dependent on the complexity of the
regular expression.

I did the benchmarks using the following command on a high performance server
(which shouldn't really matter since only one core is utilized anyway and the
RAM fingerprint is low).

	perf stat
		-r 10
		-d ./check_timed_logs_fast -pattern '.*nonExistentPattern.*' -i 9999999 -c 1 -logfile ./log

The command executs the check ten times and parses the entire file, the
resulting average execution time is the duration in the table above.

The crazy rate of improvement comes from Rust and using `memmap` to read the
file backwards. At the moment the implementation is pretty straight forward
— one process which blocks with the i/o operations and the parsing.
I suspect that there is room for more improvement and would like to implement
two additional strategies:

1. split work into worker threads
2. asynchronous processing and if possible asynchronous syscalls.

Furthermore, I know for sure (because I benchmarked it) that the `fancy-regex`
crate is a slowing factor. The `regex` crate had better performance, but doesn't
support advanced regex features like e.g. look-ahead. Since I want to stay
compatible to the original `check_timed_logs` script I have to use a (slower)
crate which supports these features.


## License

	Copyright (c)

		2018 Michael Mueller, http://micha.elmueller.net/

	Permission is hereby granted, free of charge, to any person obtaining
	a copy of this software and associated documentation files (the
	"Software"), to deal in the Software without restriction, including
	without limitation the rights to use, copy, modify, merge, publish,
	distribute, sublicense, and/or sell copies of the Software, and to
	permit persons to whom the Software is furnished to do so, subject to
	the following conditions:

	The above copyright notice and this permission notice shall be
	included in all copies or substantial portions of the Software.

	THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
	EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
	MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
	NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
	LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
	OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
	WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
