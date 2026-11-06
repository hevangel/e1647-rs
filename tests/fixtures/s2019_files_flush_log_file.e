<'
// T8.8 (clause 30.5.16): the predefined files struct is reachable as a
// method receiver and flush_log_file() parses with no arguments (the
// run-time call reports E0419 - the log-file machinery is not wired).
extend sys {
    run() is also {
        files.flush_log_file();
    };
};
'>
