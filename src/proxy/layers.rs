use tower::Service;

pub(super) struct CacheService<S> {
    inner: S,
}

impl<S, Request> Service<Request> for CacheService<S>
where
    S: Service<Request>,
{
    type Error = S::Error;
    type Future = S::Future;
    type Response = S::Response;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {}
}
