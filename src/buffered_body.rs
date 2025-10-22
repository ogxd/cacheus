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
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
            trace!("Polling...");

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

impl Serialize for BufferedBody {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("BufferedBody", 2)?;
        s.serialize_field("bufs", &self.bufs.to_vec())?;
        if let Some(trailers) = &self.trailers {
            // convert HeaderMap → HashMap<String, String>
            let map: HashMap<String, String> = trailers
                .iter()
                .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or_default().to_string()))
                .collect();
            s.serialize_field("trailers", &Some(map))?;
        } else {
            s.serialize_field("trailers", &Option::<HashMap<String, String>>::None)?;
        }
        s.end()
    }
}

impl<'de> Deserialize<'de> for BufferedBody {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            bufs: Vec<u8>,
            trailers: Option<HashMap<String, String>>,
        }

        let h = Helper::deserialize(deserializer)?;
        let mut header_map = None;
        if let Some(t) = h.trailers {
            let mut map = HeaderMap::new();
            for (k, v) in t {
                map.insert(k.parse::<hyper::header::HeaderName>().unwrap(), v.parse::<hyper::header::HeaderValue>().unwrap());
            }
            header_map = Some(map);
        }

        Ok(BufferedBody {
            bufs: BytesMut::from(&h.bufs[..]),
            trailers: header_map,
        })
    }
}

impl BufferedBody
{
    pub fn replace_strings(&mut self, find: &String, replacement: &String) -> usize
    {
        let mut body_utf8: String = String::from_utf8(self.bufs.to_vec() /* we could avoid a copy here */).unwrap();

        body_utf8 = body_utf8.replace(find, replacement);

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
