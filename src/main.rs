#![feature(proc_macro_hygiene, decl_macro)]

mod db;
mod function;
mod model;

use db::{Db};
use function::Res;
use rocket_contrib::templates::Template;
use model::{check_login, get_client_statistics, StatisticsRow, TaskRow, set_task as set_operation, delete_client as delete_client_operation,edit_client as edit_client_operation,
    add_client as add_client_operation,get_tasks,cancel_task as cancel_task_operation};
use std::collections::HashMap;

#[macro_use] extern crate rocket;


use rocket::Outcome;
use rocket::http::Status;
use rocket_contrib::serve::{StaticFiles};
use rocket::request::{self, Request, FromRequest, Form};
use rocket::response::Redirect;
use rocket::http::{Cookie, Cookies};
use rocket_contrib::json::Json;
use chrono::{Local, NaiveDateTime};
use serde::Deserialize;
use rusqlite::{NO_PARAMS};

#[derive(Debug)]
struct Client(i32);

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
            match db.conn.query_row::<Vec<i32>,_,_>("select id,is_enable from client where client_ip=?1", &[&remote_ip], |row| {
                Ok(vec![row.get(0)?, row.get(1)?])
            }) {
                Ok(ret) => {
                    if ret[1] == 1 {
                        if let Err(_e) = db.conn.execute("update client set is_online=1,last_online_time=?1 where client_ip=?2", &[&now[..], &remote_ip]) {
                            return Outcome::Failure((Status::BadRequest, ClientError::NotPermit));
                        }
                        
                        return Outcome::Success(Client(ret[0]));
                    } else {
                        Outcome::Failure((Status::BadRequest, ClientError::NotPermit))
                    }
                },
                Err(_e) => {
                    Outcome::Failure((Status::BadRequest, ClientError::NotPermit))
                }
            }
        } else {
            return Outcome::Failure((Status::BadRequest, ClientError::DbError));
        }
    }
}

#[derive(Debug)]
struct Admin(i32);

#[derive(Debug)]
enum AdminError{
    NotPermit,
}

impl <'a, 'r> FromRequest<'a, 'r> for Admin {
    type Error = AdminError;

    fn from_request(request: &'a Request<'r>) -> request::Outcome<Self, Self::Error> {
        if let Some(id) = request.cookies().get_private("user_id") {
            if let Ok(d) = id.value().parse::<i32>() {
                return Outcome::Success(Admin(d));
            } else {
                return Outcome::Failure((Status::Forbidden, AdminError::NotPermit));
            }
        } else {
            return Outcome::Failure((Status::Forbidden, AdminError::NotPermit));
        }
    }
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
                    if now - time.timestamp() + 3600*8 > offline_second {
                        ids.push(id);
                    } 
                } else {
                    ids.push(id);
                    continue;                 
                }
            }
        }
    }

    if ids.len() > 0 {
        let mut sql = "update client set is_online=0 where ".to_string();
        for id in ids {
            sql = format!("{} id={} or", sql, id);
        }        
        let len = sql.len();
        sql = sql.chars().take(len-2).collect();
        db.conn.execute(&sql, NO_PARAMS).unwrap_or_default();
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

                if let Err(_e) = db.conn.execute(&format!("update task set is_valid=?1,pulled_at='{}' where client_id=?2 and is_valid=?3", now), &[0, client_id, 1]) {
                    return Res::error(Some("操作错误".to_string()));
                }
            }
        },
        Err(_e) => {
            return Res::error(Some("数据查询错误".to_string()));
        }
    }
    
    Res::ok(None, Some(tasks))
}

#[derive(Deserialize, Debug)]
struct StatusParams<'a> {
    cpu_user: Option<f32>,
    cpu_system: Option<f32>,
    cpu_nice: Option<f32>,
    cpu_idle: Option<f32>,
    cpu_temp: Option<f32>,
    uptime: Option<u64>,
    boot_time: Option<&'a str>,
    memory_free: Option<u64>,
    memory_total: Option<u64>,
}

#[post("/set_status", data="<status>")]
fn set_status(client:Client, status:Json<StatusParams>) -> Json<Res>{
    let client_id = client.0;
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let db:Db;
    if let Ok(d) = Db::get_db() {
        db = d;
    } else {
        return Res::error(Some("数据库连接错误".to_string()));
    }

    if let Err(_e) = db.conn.execute(&format!("insert into cpu_info (client_id,cpu_user,cpu_system,cpu_idle,cpu_nice,created_at) values ({}, {}, {}, {}, {}, '{}')", client_id,
    status.cpu_user.unwrap_or_else(||{return 0.0}), status.cpu_system.unwrap_or_else(||{return 0.0}), status.cpu_idle.unwrap_or_else(||{return 0.0}), status.cpu_nice.unwrap_or_else(||{return 0.0}),
    now),
         NO_PARAMS) {
        return Res::error(Some("插入失败".to_string()));
    }

    if let Err(e) = db.conn.execute(&format!("insert into memory_info (client_id,memory_free,memory_total,created_at) values ({}, {}, {}, '{}')", client_id,
    status.memory_free.unwrap_or_else(||{return 0}), status.memory_total.unwrap_or_else(||{return 0}),
    now), 
    NO_PARAMS) {
        println!("{:?}", e);
        return Res::error(Some("插入失败".to_string()));
    }

    if let Err(e) = db.conn.execute(&format!("update client set uptime={},boot_time='{}' where id={}", status.uptime.unwrap_or_else(||{return 0}), 
        &(status.boot_time.unwrap_or_else(||{return "";})), client_id), 
    NO_PARAMS
    ) {
        println!("{:?}", e);
        return Res::error(Some("插入失败".to_string()));
    }

    Res::ok(None, None)
}

#[derive(Deserialize, Debug)]
struct TaskParams {
    client_id: u64,
    task_type: String,
}

#[post("/set_task", data="<task>")]
fn set_task(task: Json<TaskParams>) -> Json<Res>{
    let task_type = task.task_type.clone();
    match set_operation(task.client_id, task_type) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

#[get("/")]
fn index() -> Redirect{
    Redirect::to(uri!(login))
}

#[get("/login")]
fn login() -> Template{
    Template::render("login", "")
}

#[derive(FromForm, Debug)]
struct LoginParams {
    username: String,
    password: String,
}
 
#[post("/login", data="<params>")]
fn do_login(params: Form<LoginParams>, mut cookies: Cookies) -> Template{
    let mut render = HashMap::new();
    match check_login(&params.username, &params.password) {
        Ok(id) => {
            render.insert("url", "/statistics");
            render.insert("message", "登陆成功");

            cookies.add_private(Cookie::new("user_id", id.to_string()));
        },
        Err(_e) => {
            render.insert("url", "/login");
            render.insert("message", "账号或密码错误");;
        }
    }
    Template::render("do_login", &render)
}

#[get("/statistics")]
fn statistics(_admin: Admin) -> Template{
    Template::render("statistics", "")
}

#[post("/get_statistics")]
fn get_statistics(_admin: Admin) -> Json<Vec<StatisticsRow>>{
    if let Ok(ret) = get_client_statistics() {
        Json(ret)     
    } else {
        Json(vec!())
    }
}

#[derive(FromForm, Debug)]
struct OprateParams {
    client_id: u64,
    operation: String,
}

#[post("/operate", data="<params>")]
fn operate(_admin: Admin, params:Form<OprateParams>) -> Json<Res> {
    let operation = params.operation.clone();
    match set_operation(params.client_id, operation) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

#[derive(FromForm, Debug)]
struct DeleteClientParams {
    client_id: u64,
}

#[post("/delete_client", data="<params>")]
fn delete_client(_admin: Admin, params:Form<DeleteClientParams>) -> Json<Res> 
{
    match delete_client_operation(params.client_id) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

#[derive(FromForm, Debug)]
struct EditClientParams {
    client_id: u64,
    name: String,
    client_ip: String,
    is_enable: u32,  
}

#[post("/edit_client", data="<params>")]
fn edit_client(_admin: Admin, params:Form<EditClientParams>) -> Json<Res> 
{
    match edit_client_operation(params.client_id, &params.name, &params.client_ip, params.is_enable) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

#[derive(FromForm, Debug)]
struct TasksParams {
    client_id: u64,
}

#[post("/tasks", data="<params>")]
fn tasks(_admin: Admin, params:Form<TasksParams>) -> Json<Vec<TaskRow>>
{
    if let Ok(ret) = get_tasks(params.client_id) {
        Json(ret)     
    } else {
        Json(vec!())
    }
}

#[derive(FromForm, Debug)]
struct CancelTaskParams {
    task_id: u64,
}

#[post("/cancel_task", data="<params>")]
fn cancel_task(_admin: Admin, params:Form<CancelTaskParams>) -> Json<Res> 
{
    match cancel_task_operation(params.task_id) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

#[derive(FromForm, Debug)]
struct AddClientParams {
    name: String,
    client_ip: String,
}

#[post("/add_client", data="<params>")]
fn add_client(_admin: Admin, params:Form<AddClientParams>) -> Json<Res> 
{
    match add_client_operation(&params.name, &params.client_ip) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
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
    .mount("/public", StaticFiles::from("./templates/static"))
    .mount("/", routes![get_task, set_status, 
    set_task, check_online, login, do_login,
     statistics, get_statistics, operate, index,
     delete_client, edit_client, add_client, tasks,
     cancel_task])
    .attach(Template::fairing())
    .launch();
}