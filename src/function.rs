use rocket_contrib::json::Json;
use serde::{Serialize};
use rand::Rng;

use crate::model::clean_data as clean;

#[derive(Serialize)]
pub struct Res {
    ok: u8,
    message: Option<String>,
    data: Option<Vec<String>>,
}

impl Res {
    pub fn ok(message: Option<String>, data: Option<Vec<String>>) -> Json<Res> {
        Json(Res {
            ok: 1,
            message,
            data
        })
    }

    pub fn error(message: Option<String>) -> Json<Res> {
        Json(Res {
            ok: 0,
            message,
            data: None
        })
    }
}

// Cleans statistics data randomly
pub fn clean_data() {
    let mut rng = rand::thread_rng();
    let rand = rng.gen_range(0..100);

    if rand < 2 {
        if let Err(_) = clean(7){};
    }
}