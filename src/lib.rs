extern crate hyper;
extern crate hyper_tls;
extern crate futures;
extern crate tokio_core;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

#[macro_use]
extern crate error_chain;

use std::time::Duration;
use std::str::FromStr;
use std::rc::Rc;

use tokio_core::reactor;
use futures::{Future, Stream, Async, Poll};
use futures::future;

use hyper::{Uri, Method};
use hyper::client::{Client, Request};
use hyper::header::ContentType;
use hyper_tls::HttpsConnector;

use serde::ser::Serialize;
use serde::de::DeserializeOwned;

pub mod errors;
use errors::{Error, ErrorKind};

pub mod util {
    use super::futures;
    use super::errors::Error;
    pub type Future<T> = futures::Future<Item = T, Error = Error>;
    pub type BoxFuture<T> = Box<Future<T>>;
}

mod request {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
    pub struct Empty;
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
    pub struct Update {
        pub offset: i32,
        pub timeout: i32,
    }
}

mod response {
    use super::serde_json;
    use super::errors::*;

    #[derive(Clone, Debug, Deserialize)]
    #[serde(untagged)]
    pub enum Response {
        Ok { result: serde_json::Value },
        Error { description: String },
    }
    #[derive(Clone, Debug, Deserialize)]
    pub struct RawUpdate {
        pub update_id: i32,
        message: Option<serde_json::Value>,
        edited_message: Option<serde_json::Value>,
        channel_post: Option<serde_json::Value>,
        edited_channel_post: Option<serde_json::Value>,
        inline_query: Option<serde_json::Value>,
        chosen_inline_result: Option<serde_json::Value>,
        callback_query: Option<serde_json::Value>,
        shipping_query: Option<serde_json::Value>,
        pre_checkout_query: Option<serde_json::Value>,
    }
    #[derive(Debug,Clone)]
    pub enum Update {
        Message(serde_json::Value),
        EditedMessage(serde_json::Value),
        ChannelPost(serde_json::Value),
        EditedChannelPost(serde_json::Value),
        InlineQuery(serde_json::Value),
        ChosenInlineResult(serde_json::Value),
        CallbackQuery(serde_json::Value),
        ShippingQuery(serde_json::Value),
        PreCheckoutQuery(serde_json::Value),
    }
    impl RawUpdate {
        pub fn get(self) -> Result<Update> {
            if let Some(m) = self.message {
                Ok(Update::Message(m))
            } else if let Some(e) = self.edited_message {
                Ok(Update::EditedMessage(e))
            } else if let Some(c) = self.channel_post {
                Ok(Update::ChannelPost(c))
            } else if let Some(e) = self.edited_channel_post {
                Ok(Update::EditedChannelPost(e))
            } else if let Some(i) = self.inline_query {
                Ok(Update::InlineQuery(i))
            } else if let Some(c) = self.chosen_inline_result {
                Ok(Update::ChosenInlineResult(c))
            } else if let Some(c) = self.callback_query {
                Ok(Update::CallbackQuery(c))
            } else if let Some(s) = self.shipping_query {
                Ok(Update::ShippingQuery(s))
            } else if let Some(p) = self.pre_checkout_query {
                Ok(Update::PreCheckoutQuery(p))
            } else {
                Err(ErrorKind::ApiResponse("Unknown update response".to_owned()).into())
            }
        }
    }
}
pub use response::Update;

#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    timeout: Duration,
    client: Rc<Client<HttpsConnector>>,
}

pub struct UpdateStream<'a> {
    client: &'a ApiClient,
    next_offset: i32,
    pending_response: Option<util::BoxFuture<Vec<response::RawUpdate>>>,
    pending_updates: Vec<response::RawUpdate>,
}
impl<'a> UpdateStream<'a> {
    fn new(client: &'a ApiClient) -> UpdateStream<'a> {
        UpdateStream {
            client,
            next_offset: 0,
            pending_response: None,
            pending_updates: Vec::new(),
        }
    }
}

impl ApiClient {
    fn new(handle: reactor::Handle, token: &str) -> ApiClient {
        let base_url = format!("https://api.telegram.org/bot{}/", token);
        let client = Client::configure()
            .connector(HttpsConnector::new(4, &handle))
            .build(&handle);
        let timeout = Duration::from_secs(120);
        ApiClient {
            base_url,
            timeout,
            client: Rc::new(client),
        }
    }
    pub fn init(handle: reactor::Handle, token: &str) -> util::BoxFuture<ApiClient> {
        let client = ApiClient::new(handle, token);
        Box::new(client
                     .get_me()
                     .then(|res| match res {
                               Ok(_) => future::ok::<_, Error>(client),
                               Err(err) => future::err::<_, Error>(err),
                           }))
    }
    pub fn request<S, D>(&self, endpoint: &str, data: &S) -> util::BoxFuture<D>
        where S: Serialize,
              D: DeserializeOwned + 'static
    {
        let uri = Uri::from_str(&format!("{}{}", self.base_url, endpoint)).unwrap();
        let mut req = Request::new(Method::Post, uri);
        req.headers_mut().set(ContentType::json());
        req.set_body(serde_json::to_string(data).expect("Error converting struct to json"));
        Box::new(self.client
                     .request(req)
                     .from_err::<Error>()
                     .and_then(|res| {
            res.body()
                .from_err::<Error>()
                .fold(Vec::new(), |mut v, chunk| {
                    v.extend(&chunk[..]);
                    future::ok::<_, Error>(v)
                })
                .and_then(|chunks| {
                              let s = String::from_utf8(chunks).unwrap();
                              future::result::<response::Response, Error>(serde_json::from_str(&s)
                                                                              .map_err(|e| {
                                                                                           e.into()
                                                                                       }))
                          })
                .and_then(|response| match response {
                              response::Response::Ok { result } => {
                                  serde_json::from_value(result).map_err(|e| e.into())
                              }
                              response::Response::Error { description } => {
                                  return Err(ErrorKind::ApiResponse(description).into());
                              }
                          })
        }))
    }

    fn get_me(&self) -> util::BoxFuture<serde_json::Value> {
        self.request("getMe", &request::Empty)
    }
    fn get_updates(&self, offset: i32) -> util::BoxFuture<Vec<response::RawUpdate>> {
        let req = request::Update {
            offset,
            timeout: self.timeout.as_secs() as i32,
        };
        self.request("getUpdates", &req)
    }
    pub fn update_stream<'a>(&'a self) -> UpdateStream<'a> {
        UpdateStream::new(&self)
    }
}

impl<'a> Stream for UpdateStream<'a> {
    type Item = response::Update;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<response::Update>, Error> {
        loop {
            while let Some(update) = self.pending_updates.pop() {
                let new_offset = update.update_id;
                if new_offset < self.next_offset {
                    continue;
                }
                self.next_offset = new_offset + 1;
                return match update.get() {
                           Ok(up) => Ok(Async::Ready(Some(up))),
                           Err(err) => Err(err),
                       };
            }

            let pending_response = self.pending_response.take();
            if let Some(mut pending) = pending_response {
                match pending.poll() {
                    Ok(Async::Ready(updates)) => {
                        self.pending_updates = updates;
                        continue;
                    }
                    Ok(Async::NotReady) => {
                        self.pending_response = Some(pending);
                        return Ok(Async::NotReady);
                    }
                    Err(e) => {
                        self.pending_response = Some(pending);
                        return Err(e);
                    }
                }
            }
            self.pending_response = Some(self.client.get_updates(self.next_offset));
        }

    }
}
