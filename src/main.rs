#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use] extern crate rocket;

mod db;
mod function;
mod model;

use db::{Db};
use function::{Res, clean_data};
use rocket_contrib::templates::Template;
use model::{check_login, get_client_statistics, StatisticsRow, TaskRow, set_task as set_operation, delete_client as delete_client_operation,edit_client as edit_client_operation,
    add_client as add_client_operation,get_client,get_tasks,cancel_task as cancel_task_operation,get_memory_chart as get_memory_chart_m,MemoryChartLine,
    get_cpu_chart as get_cpu_chart_m,CpuChartLine};

use rocket::Outcome;
use rocket::State;
use rocket::http::Status;
use rocket_contrib::serve::{StaticFiles};
use rocket::request::{self, Request, FromRequest, Form};
use rocket::response::Redirect;
use rocket::http::{Cookie, Cookies};
use rocket_contrib::json::Json;
use chrono::{Local, NaiveDateTime};
use serde::Deserialize;
use rusqlite::{NO_PARAMS};

use std::sync::{ Mutex };
use std::net::TcpStream;
use chrono::Duration;
use ssh2::Session; 
use std::io::prelude::*;

#[macro_export]
macro_rules! fatal {
    ($($tt: tt)*) => {
        use std::io::Write;
        writeln!(&mut ::std::io::stderr(), $($tt)*).unwrap();        
        ::std::process::exit(1);
    };
}

#[derive(Debug)]
struct Client(i32);

#[derive(Debug)]
enum ClientError{
    NotPermit,
    DbError,
}

// Client guard
impl <'a, 'r> FromRequest<'a, 'r> for Client {
    type Error = ClientError;

    fn from_request(request: &'a Request<'r>) -> request::Outcome<Self, Self::Error> {
        let remote_ip = request.real_ip().unwrap_or(request.client_ip().unwrap()).to_string();
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let db:Db;
        if let Ok(d) = Db::get_db() {
            db = d;
            // verifys client info
            match db.conn.query_row::<Vec<i32>,_,_>("select id from client where client_ip=?1 and is_enable=1", &[&remote_ip], |row| {
                Ok(vec![row.get(0)?])
            }) {
                Ok(ret) => {
                    // updates client connection info
                    if let Err(_e) = db.conn.execute("update client set is_online=1,last_online_time=?1 where client_ip=?2", &[&now[..], &remote_ip]) {
                        return Outcome::Failure((Status::BadRequest, ClientError::NotPermit));
                    }
                    
                    return Outcome::Success(Client(ret[0]));
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

// Admin guard
impl <'a, 'r> FromRequest<'a, 'r> for Admin {
    type Error = AdminError;

    fn from_request(request: &'a Request<'r>) -> request::Outcome<Self, Self::Error> {
        clean_data();   // clean data

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

// updates client online status
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
    system_version: Option<String>,
    package_manager: Option<String>,
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

    if let Err(_e) = db.conn.execute(&format!("insert into memory_info (client_id,memory_free,memory_total,created_at) values ({}, {}, {}, '{}')", client_id,
    status.memory_free.unwrap_or_else(||{return 0}), status.memory_total.unwrap_or_else(||{return 0}),
    now), 
    NO_PARAMS) {
        return Res::error(Some("插入失败".to_string()));
    }

    let mut sql = format!("update client set uptime={},boot_time='{}'", status.uptime.unwrap_or_else(||{return 0}), 
    &(status.boot_time.unwrap_or_else(||{return "";})));
    if let Some(i) = status.system_version.clone() {
        sql = format!("{},system_version='{}'", sql, i);
    }
    if let Some(i) = status.package_manager.clone() {
        sql = format!("{},package_manager_update_count='{}'", sql, i);
    } 
    sql = format!("{} where id={} ", sql, client_id);

    if let Err(_e) = db.conn.execute(&sql, 
        NO_PARAMS
    ) {
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
fn do_login(params: Form<LoginParams>, mut cookies: Cookies) -> Json<Res>{
    match check_login(&params.username, &params.password) {
        Ok(id) => {
            let mut cookie = Cookie::build("user_id", id.to_string()).finish();
            cookie.set_max_age(Duration::days(30));
            cookies.add_private(cookie);
            return Res::ok(None, None);
        },
        Err(_e) => {
            return Res::error(Some("帐号密码错误".to_string())); 
        }
    }
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

#[post("/get_memory_chart")]
fn get_memory_chart(_admin: Admin) -> Json<Option<Vec<MemoryChartLine>>>{
    if let Ok(ret) = get_memory_chart_m() {
        Json(Some(ret))     
    } else {
        Json(None)
    }
}

#[post("/get_cpu_chart")]
fn get_cpu_chart(_admin: Admin) -> Json<Option<Vec<CpuChartLine>>>{
    if let Ok(ret) = get_cpu_chart_m() {
        Json(Some(ret))     
    } else {
        Json(None)
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
    ssh_address: Option<String>,
    ssh_username: Option<String>,
    ssh_password: Option<String>,
}

#[post("/add_client", data="<params>")]
fn add_client(_admin: Admin, params:Form<AddClientParams>) -> Json<Res> 
{
    match add_client_operation(&params.name, &params.client_ip, 
        &params.ssh_address.clone().unwrap_or("".to_string()), &params.ssh_username.clone().unwrap_or("".to_string()), &params.ssh_password.clone().unwrap_or("".to_string())) {
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
    ssh_address: Option<String>,
    ssh_username: Option<String>,
    ssh_password: Option<String>,
}

#[post("/edit_client", data="<params>")]
fn edit_client(_admin: Admin, params:Form<EditClientParams>) -> Json<Res> 
{
    match edit_client_operation(params.client_id, &params.name, &params.client_ip, params.is_enable, 
        &params.ssh_address.clone().unwrap_or("".to_string()), &params.ssh_username.clone().unwrap_or("".to_string()), &params.ssh_password.clone().unwrap_or("".to_string())) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

#[derive(FromForm, Debug)]
struct ConnectSshClientParams {
    client_id: i64,
}

#[post("/connect_ssh_client", data="<params>")]
fn connect_ssh_client(_admin: Admin, params:Form<ConnectSshClientParams>, session: State<SshSession>) -> Json<Res>
{
    let client;
    match get_client(params.client_id) {
        Ok(c) => client = c,
        Err(_e) => {
            return Res::error(Some("客户错误".to_string())); 
        },
    }

    let mut client_id = session.client_id.lock().unwrap(); 
    let mut s = session.session.lock().unwrap();
    let mut is_init = false;
    if let Some(id) = *client_id {
        if id != params.client_id {
            is_init = true;
        }
    } else {
        is_init = true;
    }

    if is_init {
        // Connect to the local SSH server
        match TcpStream::connect(client.ssh_address.unwrap_or("".to_string())) {
            Ok(r) => {
                let tcp = r;
                let mut sess = Session::new().unwrap();
                sess.set_tcp_stream(tcp);
                sess.handshake().unwrap();
                if let Err(e) = sess.userauth_password(&client.ssh_username.unwrap_or("".to_string()), &client.ssh_password.unwrap_or("".to_string())) {
                    return Res::error(Some(format!("{}", e))); 
                }
                *s = Some(sess);
            },
            Err(e) => {
                return Res::error(Some(format!("{}", e)));
            }
        }
    }

    *client_id = Some(params.client_id);
    return Res::ok(None, None);
}

#[derive(FromForm, Debug)]
struct SshCommandParams {
    client_id: i64,
    command: String,
}

#[post("/run_ssh_command", data="<params>")]
fn run_ssh_command(_admin: Admin, params:Form<SshCommandParams>, session: State<SshSession>) -> Json<Res>
{
    let client_id = session.client_id.lock().unwrap(); 
    let s = session.session.lock().unwrap();
    if let Some(id) = *client_id {
        if id != params.client_id {
            return Res::error(Some("请重新连接终端".to_string()));
        }
    } else {
        return Res::error(Some("请先连接终端".to_string()));
    }

    if let Some(session) = &*s {
        let mut channel = session.channel_session().unwrap();
        channel.exec(&params.command).unwrap();
        let mut s = String::new();
        channel.read_to_string(&mut s).unwrap();
        return Res::ok(Some(s), None);
    } else {
        return Res::error(Some("查询错误".to_string()));
    }
}

struct SshSession {
    client_id: Mutex<Option<i64>>,
    session: Mutex<Option<Session>>,
}

fn main() {
    let db:Db = Db::get_db().unwrap_or_else(|e| {
        fatal!("数据库加载错误, {}", e);
    });

    if let Err(e) = db.check_init() {
        fatal!("{}", e);
    }

    let ssh_client = SshSession {
        client_id: Mutex::new(None),
        session: Mutex::new(None),
    };

    rocket::ignite()
    .mount("/public", StaticFiles::from("./templates/static"))
    .mount("/", routes![get_task, set_status, 
    set_task, check_online, login, do_login,
     statistics, get_statistics, operate, index,
     delete_client, edit_client, add_client, tasks,
     cancel_task,get_memory_chart,get_cpu_chart,
     connect_ssh_client,run_ssh_command,
     ])
    .attach(Template::fairing())
    .manage(ssh_client)
    .launch();
}