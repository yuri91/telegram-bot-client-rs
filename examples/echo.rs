extern crate futures;
extern crate tokio_core;

extern crate serde_json;

extern crate telegram_bot_client;
use telegram_bot_client::{BotFactory, Update};

extern crate telegram_bot_types;
use telegram_bot_types as types;

use tokio_core::reactor;
use futures::{Future, Stream};
use futures::future;

fn main() {
    let mut event_loop = reactor::Core::new().unwrap();
    let handle = event_loop.handle();

    let factory = BotFactory::new(handle.clone());
    let (bot, updates) =
        factory.new_bot(&std::env::var("BOT_TOKEN")
                             .expect("Please define the BOT_TOKEN env variable"));
    let work = updates
        .filter_map(|update| {
                        println!("{:?}", update);
                        match update {
                            Update::Message(msg) => Some(msg),
                            _ => None,
                        }
                    })
        .for_each(|msg| {
            let msg: types::response::Message = serde_json::from_value(msg)
                .expect("Unexpected message format");
            let ret = bot
                        .request::<_, serde_json::Value>("sendMessage",
                                 &types::request::Message::new(
                                     msg.chat.id,
                                     msg.text.expect("please send text for now")));
            ret.and_then(|r| {
                             println!("{:?}", r);
                             future::ok(())
                         })
        });
    event_loop.run(work).unwrap();
}
