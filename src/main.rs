#![feature(proc_macro_hygiene, decl_macro)]

mod db;
mod function;
use db::{Db};
use function::Res;

#[macro_use] extern crate rocket;


use rocket::Outcome;
use rocket::http::Status;
use rocket::request::{self, Request, FromRequest};
use rocket_contrib::json::Json;
use chrono::{Local, DateTime, NaiveDateTime};
use chrono::prelude::*;
use serde::Deserialize;
use rusqlite::{NO_PARAMS};

#[derive(Debug)]
struct Client(u32);

#[derive(Debug)]
enum ClientError{
    NotPermit,
    DbError,
}

impl <'a, 'r> FromRequest<'a, 'r> for Client {
    type Error = ClientError;

    fn from_request(request: &'a Request<'r>) -> request::Outcome<Self, Self::Error> {
        let remote_addr = request.remote().unwrap().to_string();
        let remote:Vec<&str> = remote_addr.split(":").collect();
        let remote_ip = remote[0];
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let db:Db;
        if let Ok(d) = Db::get_db() {
            db = d;
            match db.conn.query_row::<Vec<u32>,_,_>("select id,is_enable from client where client_ip=?1", &[&remote_ip], |row| {
                Ok(vec![row.get(0)?, row.get(1)?])
            }) {
                Ok(ret) => {
                    if (ret[1] == 1) {
                        db.conn.execute("update client set is_online=1,last_online_time=?1 where client_ip=?2", &[&now[..], &remote_ip]);
                        
                        return Outcome::Success(Client(ret[0]));
                    } else {
                        Outcome::Failure((Status::BadRequest, ClientError::NotPermit))
                    }
                },
                Err(e) => {
                    Outcome::Failure((Status::BadRequest, ClientError::NotPermit))
                }
            }
        } else {
            return Outcome::Failure((Status::BadRequest, ClientError::DbError));
        }
    }
}

#[get("/")]
fn index(client:Client) -> &'static str {
    "Hello, world!"
}

#[get("/check_online")]
fn check_online() -> &'static str {
    let now = Local::now().timestamp();
    let offline_second = 30;
    let db:Db;
    if let Ok(d) = Db::get_db() {
        db = d;
    } else {
        return "error";
    }

    let ret = db.conn.prepare("select * from client where is_online=1");
    let mut ids = Vec::new();
    if let Ok(mut smtm) = db.conn.prepare("select id,last_online_time from client where is_online=1") {
        if let Ok(mut ret) = smtm.query(NO_PARAMS) {

            while let Some(row) = ret.next().unwrap() {
                let last_online_time:String;
                let mut id:i32 = 0;
                if let Ok(i) = row.get(0) {
                    id = i;
                } 
                
                if let Ok(time) = row.get(1) {
                    last_online_time = time;
                } else {
                    ids.push(id);
                    continue;
                }

                if let Ok(time) =  NaiveDateTime::parse_from_str(&last_online_time, "%Y-%m-%d %H:%M:%S") {
                    if (now - time.timestamp() + 3600*8 > offline_second) {
                        ids.push(id);
                    } 
                } else {
                    ids.push(id);
                    continue;                 
                }
            }
        }
    }

    if (ids.len() > 0) {
        let mut sql = "update client set is_online=0 where ".to_string();
        for id in ids {
            sql = format!("{} id={} or", sql, id);
        }        
        let len = sql.len();
        sql = sql.chars().take(len-2).collect();
        db.conn.execute(&sql, NO_PARAMS);
    }

    ""
}


#[post("/get_task")]
fn get_task(client:Client) -> Json<Res>{
    let client_id = client.0;
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let db:Db;
    if let Ok(d) = Db::get_db() {
        db = d;
    } else {
        return Res::error(Some("数据库连接错误".to_string()));
    }
    
    let mut tasks:Vec<String> = Vec::new();
    match db.conn.prepare("select task_type from task where client_id=?1 and is_valid=?2") {
        Ok(mut smtm) => {
            if let Ok(mut ret) = smtm.query(&[client_id, 1]) {
                while let Some(row) = ret.next().unwrap() {
                    if let Ok(task) = row.get(0) {
                        tasks.push(task);
                    }
                }

                if let Err(e) = db.conn.execute(&format!("update task set is_valid=?1,pulled_at='{}' where client_id=?2 and is_valid=?3", now), &[0, client_id, 1]) {
                    return Res::error(Some("操作错误".to_string()));
                }
            }
        },
        Error => {
            return Res::error(Some("数据查询错误".to_string()));
        }
    }
    
    Res::ok(None, Some(tasks))
}

#[derive(Deserialize, Debug)]
struct TaskParams {
    client_id: i64,
    task_type: String,
}

#[post("/set_task", data="<task>")]
fn set_task(task: Json<TaskParams>) -> Json<Res>{
    let db:Db;
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(d) = Db::get_db() {
        db = d;
    } else {
        return Res::error(Some("数据库连接错误".to_string()));
    }

    if let Err(e) = db.conn.query_row::<i64, _, _>("select id from client where id=?1 and is_valid=1 and is_online=1", &[task.client_id], |row| {
        row.get(0)
    }) {
        Res::error(Some("客户不存在".to_string()));
    }

    if let Err(e) = db.conn.execute(&format!("insert into task (client_id,task_type,created_at) values ({}, ?1, ?2)", task.client_id), &[&task.task_type, &now]) {
        println!("{:?}", e);
        return Res::error(Some("插入失败".to_string()));
    }
    Res::ok(None, None)
}

fn main() {
    let db:Db = Db::get_db().unwrap_or_else(|e| {
        println!("数据库加载错误，{}", e);
        std::process::exit(9);
    });

    if let Err(e) = db.check_init() {
        println!("{:?}", e);
        std::process::exit(9);
    }

    rocket::ignite()
    .mount("/", routes![index, get_task, set_task, check_online])
    .launch();
}