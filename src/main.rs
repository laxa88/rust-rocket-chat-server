#[macro_use]
extern crate rocket;

use std::sync::Arc;

use rocket::form::Form;
use rocket::fs::{relative, FileServer};
use rocket::response::stream::{Event, EventStream};
use rocket::serde::{Deserialize, Serialize};
use rocket::tokio::select;
use rocket::tokio::sync::broadcast::{channel, error::RecvError, Sender};
use rocket::tokio::sync::RwLock;
use rocket::{Shutdown, State};

#[derive(Debug, Clone)]
struct ChatState {
    pub messages: Arc<RwLock<Vec<Message>>>,
}

#[derive(Debug, Clone, FromForm, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Message {
    #[field(validate = len(..30))]
    pub room: String,

    #[field(validate = len(..20))]
    pub username: String,

    pub message: String,
}

#[post("/message", data = "<form>")]
async fn post(chat_state: &State<ChatState>, form: Form<Message>, queue: &State<Sender<Message>>) {
    println!("### post {:?}", chat_state);

    let messages = &chat_state.messages;
    messages.write().await.push(form.clone().into());

    let _res = queue.send(form.into_inner());
}

/// Returns an infinite stream of server-sent events. Each event is a message
/// pulled from a broadcast queue sent by the `post` handler.
#[get("/events")]
async fn events(
    chat_state: &State<ChatState>,
    queue: &State<Sender<Message>>,
    mut end: Shutdown,
) -> EventStream![] {
    let messages = chat_state.messages.read().await.clone();

    println!("### event {:?}", messages);

    let mut rx = queue.subscribe();

    EventStream! {
        yield Event::json(&messages);

        loop {
            let msg = select! {
                msg = rx.recv() => match msg {
                    Ok(msg) => msg,
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(_)) => continue,
                },
                _ = &mut end => break,
            };

            yield Event::json(&msg);
        }
    }
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .manage(channel::<Message>(1024).0)
        .manage(ChatState {
            messages: Arc::new(RwLock::new(Vec::new())),
        })
        .mount("/", routes![post, events])
        .mount("/", FileServer::from(relative!("static")))
}
