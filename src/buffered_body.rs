use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::Hash;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Buf, BytesMut};
use futures::Future;
use hyper::body::{Body, Frame};
use hyper::HeaderMap;
use pin_project_lite::pin_project;

pin_project! {
    /// Future that resolves into a [`Collected`].
    ///
    /// [`Collected`]: crate::Collected
    pub struct BufferBody<T>
    where
        T: Body,
        T: ?Sized,
    {
        pub(crate) collected: Option<BufferedBody>,
        #[pin]
        pub(crate) body: T,
    }
}

impl<T: Body + ?Sized> Future for BufferBody<T>
{
    type Output = Result<BufferedBody, T::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> std::task::Poll<Self::Output>
    {
        let mut me = self.project();

        loop {
            debug!("Polling...");

            let frame = futures_core::ready!(me.body.as_mut().poll_frame(cx));

            let frame = if let Some(frame) = frame {
                frame?
            } else {
                return Poll::Ready(Ok(me.collected.take().expect("polled after complete")));
            };

            me.collected.as_mut().unwrap().push_frame(frame);
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct BufferedBody
{
    bufs: BytesMut,
    trailers: Option<HeaderMap>,
}

impl BufferedBody
{
    pub fn replace_strings(&mut self, map: &HashMap<String, String>) -> usize
    {
        if map.len() == 0 {
            return self.bufs.len();
        }

        let mut body_utf8: String = String::from_utf8(self.bufs.to_vec() /* we could avoid a copy here */).unwrap();

        for (key, value) in map {
            body_utf8 = body_utf8.replace(key, &value);
        }

        self.bufs = BytesMut::from(body_utf8.as_bytes());

        return self.bufs.len();
    }

    /// If there is a trailers frame buffered, returns a reference to it.
    /// Returns `None` if the body contained no trailers.
    pub fn trailers(&self) -> Option<&HeaderMap>
    {
        self.trailers.as_ref()
    }

    pub(crate) fn push_frame<B>(&mut self, frame: Frame<B>)
    where
        B: Buf,
    {
        let frame = match frame.into_data() {
            Ok(mut data) => {
                // Only push this frame if it has some data in it, to avoid crashing on
                // `BufList::push`.
                while data.has_remaining() {
                    // Append the data to the buffer.
                    self.bufs.extend(data.chunk());
                    data.advance(data.remaining());
                }
                return;
            }
            Err(frame) => frame,
        };

        if let Ok(trailers) = frame.into_trailers() {
            if let Some(current) = &mut self.trailers {
                current.extend(trailers);
            } else {
                self.trailers = Some(trailers);
            }
        };
    }

    pub fn collect_buffered<T>(body: T) -> BufferBody<T>
    where
        T: Body,
        T: Sized,
    {
        BufferBody {
            body: body,
            collected: Some(BufferedBody::default()),
        }
    }

    pub fn from_bytes(b: &[u8]) -> BufferedBody
    {
        let mut bufs = BytesMut::new();
        bufs.extend(b);
        BufferedBody { bufs, trailers: None }
    }
}

impl Body for BufferedBody
{
    type Data = BytesMut;
    type Error = Infallible;

    fn poll_frame(mut self: Pin<&mut Self>, _: &mut Context<'_>)
        -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>>
    {
        let frame = if self.bufs.len() > 0
        /* Shall we skip this frame if body is empty? */
        {
            let frame = Frame::data(self.bufs.to_owned());
            self.bufs.clear();
            frame
        } else if let Some(trailers) = self.trailers.take() {
            Frame::trailers(trailers)
        } else {
            return Poll::Ready(None);
        };

        Poll::Ready(Some(Ok(frame)))
    }
}

impl Hash for BufferedBody
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H)
    {
        self.bufs.hash(state);
        //self.trailers.hash(state);
    }
}
