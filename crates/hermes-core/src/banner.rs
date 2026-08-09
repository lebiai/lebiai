//! ASCII banner for lebi-AI.

pub const LOGO: &str = "\
\x1b[34m  _            _     _  \x1b[0m \x1b[35m     _    ___\x1b[0m
\x1b[34m | |    ___   | |__ (_) \x1b[0m \x1b[35m    / \\  |_ _|\x1b[0m
\x1b[34m | |   / _ \\  | '_ \\| | \x1b[0m \x1b[35m   / _ \\  | |\x1b[0m
\x1b[34m | |__| (_) | | |_) | | \x1b[0m \x1b[35m  / ___ \\ | |\x1b[0m
\x1b[34m |_____\\___/  |_.__/|_| \x1b[0m \x1b[35m /_/   \\_\\___|\x1b[0m
\x1b[90m        ⚡ 越用越像你的手感 ⚡\x1b[0m";

pub const LOGO_PLAIN: &str = "\
  _            _     _       _    ___
 | |    ___   | |__ (_)     / \\  |_ _|
 | |   / _ \\  | '_ \\| |    / _ \\  | |
 | |__| (_) | | |_) | |   / ___ \\ | |
 |_____\\___/  |_.__/|_|  /_/   \\_\\___|
        ⚡ 越用越像你的手感 ⚡";

pub fn print_banner() {
    if crate::style::ansi_enabled() {
        eprintln!("{LOGO}");
    } else {
        eprintln!("{LOGO_PLAIN}");
    }
}
