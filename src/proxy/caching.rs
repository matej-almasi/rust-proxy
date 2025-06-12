use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::task;
use std::task::Poll;
use std::vec::IntoIter;

use dashmap::DashMap;
use futures::stream;
use futures::stream::Iter;
use http::Uri;
use http_body_util::StreamBody;
use hyper::body::Body;
use hyper::body::Frame;
use hyper::Request;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone)]
pub struct CacheService<S> {
    inner: S,
}

impl<S> CacheService<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S, RqB, RsB> tower::Service<Request<RqB>> for CacheService<S>
where
    S: tower::Service<Request<RqB>, Response = http::Response<RsB>> + Send + Clone + 'static,
    S::Future: Send,
    RqB: Send + 'static,
    RsB: Body + Send + 'static,
    RsB::Data: Send,
{
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    type Response = http::Response<CacheableBody<RsB>>;

    fn poll_ready(&mut self, cx: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<RqB>) -> Self::Future {
        // the clone may not be ready, calling call on it may panic.
        // Therefore, we have to use the original service (which is guaranteed
        // to be ready if the user called poll_ready).
        let inner = self.inner.clone();
        let mut inner = mem::replace(&mut self.inner, inner);

        let fut = async move {
            let cache = DashMap::<Uri, http::Response<CacheableBody<RsB>>>::new();
            let uri = req.uri().to_owned();

            let (parts, body) = inner.call(req).await?.into_parts();

            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

            let cached_parts = parts.clone();

            tokio::spawn(async move {
                let mut frames = Vec::new();

                while let Some(frame) = rx.recv().await {
                    frames.push(frame);
                }

                let body = CacheableBody::CachedBody {
                    inner: StreamBody::new(stream::iter(frames)),
                };

                cache.insert(uri, http::Response::from_parts(cached_parts, body));
            });

            let caching_body = CacheableBody::CachingBody {
                inner: body,
                cache_tx: tx,
            };

            Ok(http::Response::from_parts(parts, caching_body))
        };

        Box::pin(fut)
    }
}

pub type ResultFrame<D> = crate::Result<Frame<D>>;

pub enum CacheableBody<B: Body> {
    CachedBody {
        inner: StreamBody<Iter<IntoIter<ResultFrame<B::Data>>>>,
    },

    CachingBody {
        inner: B,
        cache_tx: UnboundedSender<ResultFrame<B::Data>>,
    },
}

impl<B> Body for CacheableBody<B>
where
    B: Body + Unpin,
    B::Data: Clone,
{
    type Data = B::Data;
    type Error = crate::Error;

    fn is_end_stream(&self) -> bool {
        match self {
            Self::CachingBody { inner, .. } => inner.is_end_stream(),
            Self::CachedBody { inner } => inner.is_end_stream(),
        }
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        match self {
            Self::CachingBody { inner, .. } => inner.size_hint(),
            Self::CachedBody { inner } => inner.size_hint(),
        }
    }

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        match *self {
            Self::CachedBody { ref mut inner } => Pin::new(inner).poll_frame(cx),
            Self::CachingBody {
                ref mut inner,
                ref cache_tx,
            } => {
                let poll = Pin::new(inner)
                    .poll_frame(cx)
                    .map_err(|_| crate::Error::CachingError);

                if let Poll::Ready(Some(ref frame)) = &poll {
                    let frame = frame
                        .as_ref()
                        .map(|frame| {
                            if frame.is_data() {
                                Frame::data(frame.data_ref().unwrap().clone())
                            } else {
                                Frame::trailers(frame.trailers_ref().unwrap().clone())
                            }
                        })
                        .map_err(|_| crate::Error::CachingError);

                    if let Err(e) = cache_tx.send(frame) {
                        tracing::warn!("Couldn't send cache body: {e}");
                    };
                }

                poll
            }
        }
    }
}
