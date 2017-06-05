extern crate futures;
extern crate tokio_core;

extern crate serde_json;

extern crate telegram_bot;
use telegram_bot::{ApiClient, Update};
use telegram_bot::util;

extern crate telegram_bot_api;
use telegram_bot_api as api;

use tokio_core::reactor;
use futures::{Future, Stream};
use futures::future;

fn main() {
    let mut event_loop = reactor::Core::new().unwrap();
    let handle = event_loop.handle();

    let try_client = ApiClient::init(handle, &std::env::var("BOT_TOKEN").expect("Please define the BOT_TOKEN env variable"));
    let client = event_loop.run(try_client).expect("Authentication problem");
    let work = client
        .update_stream()
        .for_each(|update| {
            println!("{:?}", update);
            let w: util::BoxFuture<()> = match update {
                Update::Message(msg) => {
                    let msg: api::response::Message = serde_json::from_value(msg)
                        .expect("Unexpected message format");
                    let ret: util::BoxFuture<serde_json::Value> = client
                        .request("sendMessage",
                                 &api::request::Message {
                                     chat_id: msg.chat.id,
                                     text: msg.text.expect("please send text for now"),
                                 });
                    Box::new(ret.and_then(|r| {
                                              println!("{:?}", r);
                                              future::ok(())
                                          }))
                }
                _ => Box::new(future::ok(())),
            };
            w
        });
    event_loop.run(work).unwrap();
}
