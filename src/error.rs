use std::error::Error;

type BoxedAsyncError = Box<dyn Error + Send + Sync + 'static>;

#[derive(Debug)]
pub struct ProxyError {
    kind: ErrorKind,
    source: Option<BoxedAsyncError>,
}

impl ProxyError {
    pub fn new(kind: ErrorKind) -> Self {
        Self { kind, source: None }
    }

    pub fn source(&mut self, source: BoxedAsyncError) {
        self.source = Some(source);
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }
}

impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error running proxy: {}", self.kind)
    }
}

impl Error for ProxyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|src| src.as_ref() as &(dyn Error + 'static))
    }
}

#[derive(Debug)]
pub enum ErrorKind {
    UpstreamHostDNSResolutionError,
    UpstreamHostNotFound,
    UpstreamRequestFail,
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKind::UpstreamHostDNSResolutionError => {
                write!(f, "Failed to DNS resolve proxied resource.")
            }
            ErrorKind::UpstreamRequestFail => write!(f, "Failed requesting proxied resource."),
            ErrorKind::UpstreamHostNotFound => write!(f, "Proxied host not found."),
        }
    }
}
