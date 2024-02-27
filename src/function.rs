use std::sync::Mutex;

use rocket_contrib::json::Json;
use serde::Serialize;
use rand::Rng;

use crate::model::clean_data as clean;

#[derive(Serialize)]
pub struct Res<T> {
    ok: u8,
    message: Option<String>,
    data: Option<T>,
}

impl <T:Serialize>Res<T> {
    pub fn ok(message: Option<String>, data: Option<T>) -> Json<Res::<T>> {
        Json(Res {
            ok: 1,
            message,
            data
        })
    }

    pub fn error(message: Option<String>) -> Json<Res::<T>> {
        Json(Res {
            ok: 0,
            message,
            data: None
        })
    }
}

// Cleans statistics data randomly
pub fn clean_data(locker:& Mutex<bool>) {
    let mut rng = rand::thread_rng();
    let rand = rng.gen_range(0..1000);

    if rand < 2 {
        if let Ok(_) = locker.try_lock() {
            if let Err(_) = clean(7){};
        }
    }
}
