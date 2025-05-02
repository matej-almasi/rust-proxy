use crate::proxy::logger::{Logger, Severity};

pub struct ConsoleLogger {
    severity: Severity,
}
impl ConsoleLogger {
    pub fn with_severity(severity: Severity) -> ConsoleLogger {
        ConsoleLogger { severity }
    }
}

impl Logger for ConsoleLogger {
    fn log(&self, message: &str, severity: Severity) {
        if severity < self.severity {
            return;
        }

        let timestamp = chrono::Local::now();

        if severity < Severity::Error {
            println!("{timestamp} {severity}: {message}")
        }
    }
}
