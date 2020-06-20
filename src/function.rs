use rocket_contrib::json::Json;
use serde::{Serialize};

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