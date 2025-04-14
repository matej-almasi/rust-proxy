#[derive(PartialEq, PartialOrd, Clone, Copy, Debug)]
pub enum Severity {
    Debug,
    Information,
    Warning,
    Error,
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Severity::*;

        match self {
            Debug => write!(f, "DEBUG"),
            Information => write!(f, "INFO"),
            Warning => write!(f, "WARNING"),
            Error => write!(f, "ERROR"),
            Critical => write!(f, "CRITICAL"),
        }
    }
}

pub trait Logger {
    /// Log a `message` with the given [`Severity`].
    fn log(&mut self, message: &str, severity: Severity);

    /// Log a `message` with [`Severity::Debug`].
    fn debug(&mut self, message: &str) {
        self.log(message, Severity::Debug);
    }

    /// Log a `message` with [`Severity::Information`].
    fn info(&mut self, message: &str) {
        self.log(message, Severity::Information);
    }

    /// Log a `message` with [`Severity::Warning`].
    fn warn(&mut self, message: &str) {
        self.log(message, Severity::Warning);
    }

    /// Log a `message` with [`Severity::Error`].
    fn error(&mut self, message: &str) {
        self.log(message, Severity::Error);
    }

    /// Log a `message` with [`Severity::Critical`].
    fn critical(&mut self, message: &str) {
        self.log(message, Severity::Critical);
    }
}

pub struct StubLogger;

impl Logger for StubLogger {
    fn log(&mut self, _message: &str, _severity: Severity) {}
}
