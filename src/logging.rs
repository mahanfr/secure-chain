
use colored::Colorize;


/// Colorize based on logging level
/// level 0: info(blue)
/// level 1: success(green)
/// level 2: warning(yellow)
/// level 3: error(red)
/// ignores if logging is set to file
pub fn log_color(level: u8) -> colored::ColoredString {
    match level {
        0 => "[Info]".blue(),
        1 => "[Success]".green(),
        2 => "[Waring]".yellow(),
        3 => "[Error]".red(),
        _ => "[Info]".into()
    }
}
/// Log info
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)+) => {
        println!("{} {}",crate::logging::log_color(0),format_args!($($arg)+))
    };
}

/// Log success
#[macro_export]
macro_rules! log_success {
    ($($arg:tt)+) => {
        println!("{} {}",crate::logging::log_color(1),format_args!($($arg)+))
    };
}

/// Log Warning
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)+) => {
        println!("{} {}",crate::logging::log_color(2),format_args!($($arg)+))
    };
}

/// Log Error
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)+) => {
        eprintln!("{} {}",crate::logging::log_color(3),format_args!($($arg)+))
    };
}
