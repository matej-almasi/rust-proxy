use hyper::body::Incoming;

use hyper::Request;

use hyper::service::Service;

use super::logger::Logger;

use std::sync::Arc;

pub(super) struct LogService<S> {
    pub(crate) inner: S,
    pub(crate) logger: Arc<dyn Logger + Send + Sync>,
}

impl<S> LogService<S> {
    pub fn new(inner: S, logger: Arc<dyn Logger + Send + Sync>) -> Self {
        LogService { inner, logger }
    }
}

impl<S> Service<Request<Incoming>> for LogService<S>
where
    S: Service<Request<Incoming>>,
{
    type Error = S::Error;
    type Future = S::Future;
    type Response = S::Response;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        println!("TEST");
        self.inner.call(req)
    }
}
