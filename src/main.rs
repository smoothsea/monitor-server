#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use] extern crate rocket;

mod db;
mod function;
mod model;

use db::Db;
use ssh2::Session; 
use chrono::Duration;
use std::sync::Mutex;
use serde::Deserialize;
use rusqlite::NO_PARAMS;
use std::net::TcpStream;
use rocket::http::Status;
use std::io::prelude::*;
use rocket::{State, Outcome};
use rocket_contrib::json::Json;
use rocket::response::Redirect;
use function::{Res, clean_data};
use chrono::{Local, NaiveDateTime};
use rocket::http::{Cookie, Cookies};
use rocket_contrib::serve::StaticFiles;
use rocket_contrib::templates::Template;
use rocket::request::{self, Request, FromRequest, Form};

#[macro_export]
macro_rules! fatal {
    ($($tt: tt)*) => {
        use std::io::Write;
        writeln!(&mut ::std::io::stderr(), $($tt)*).unwrap();        
        ::std::process::exit(1);
    };
}

// Client auth
#[derive(Debug)]
struct Client(u32);

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
        let machine_id = request.headers().get_one("authorization").unwrap_or("");

        let db:Db;
        if let Ok(d) = Db::get_db() {
            db = d;
            // verifys client info
            match db.conn.query_row::<u32,_,_>("select id from client where client_ip=?1 and is_enable=1", &[&remote_ip], |row| {
                Ok(row.get(0)?)
            }) {
                Ok(ret) => {
                    // updates client connection info
                    if let Err(_e) = db.conn.execute("update client set is_online=1,last_online_time=?1 where client_ip=?2", &[&now[..], &remote_ip]) {
                        return Outcome::Failure((Status::BadRequest, ClientError::NotPermit));
                    }
                    
                    return Outcome::Success(Client(ret));
                },
                Err(_e) => {
                    // 
                    if machine_id != "" {
                        model::create_apply(machine_id, &remote_ip).unwrap_or(());
                    }
                    Outcome::Failure((Status::BadRequest, ClientError::NotPermit))
                }
            }
        } else {
            return Outcome::Failure((Status::BadRequest, ClientError::DbError));
        }
    }
}

// Admin auth
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
            return Outcome::Forward(());
        }
    }
}

// updates client online status
#[get("/check_online")]
fn check_online() {
    let now = Local::now().timestamp();
    let offline_second = 30;
    let db:Db;
    if let Ok(d) = Db::get_db() {
        db = d;
    } else {
        return ();
    }

    let mut ids = Vec::new();
    if let Ok(mut smtm) = db.conn.prepare("select id,last_online_time from client where is_online=1") {
        if let Ok(mut ret) = smtm.query(NO_PARAMS) {
            while let Some(row) = ret.next().unwrap() {
                let id:u32 = row.get(0).unwrap();
                let last_online_time:String;
                if let Ok(time) = row.get(1) {
                    last_online_time = time;
                } else {
                    ids.push(id);
                    continue;
                }

                if let Ok(time) = NaiveDateTime::parse_from_str(&last_online_time, "%Y-%m-%d %H:%M:%S") {
                    if now - time.timestamp() + 3600*8 > offline_second {
                        ids.push(id);
                    } 
                } else {
                    ids.push(id);
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
    network_stats: Vec<NetworkStat>,
    disk_avail: Option<u64>,
    disk_total: Option<u64>,
}

#[derive(Deserialize, Debug)]
struct NetworkStat {
   pub if_name: String,
   pub rx_bytes: u64, 
   pub tx_bytes: u64, 
   pub rx_packets: u64, 
   pub tx_packets: u64, 
   pub rx_errors: u64, 
   pub tx_errors: u64, 
}


#[post("/set_status", data="<status>")]
fn set_status(client:Client, status:Json<StatusParams>) -> Json<Res::<Vec<String>>>{
    let client_id = client.0;
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let db:Db;
    if let Ok(d) = Db::get_db() {
        db = d;
    } else {
        return Res::error(Some("数据库连接错误".to_string()));
    }

    if let Err(_e) = db.conn.execute(&format!("insert into cpu_info (client_id,cpu_user,cpu_system,cpu_idle,cpu_nice,created_at) values ({}, {}, {}, {}, {}, '{}')", client_id,
    status.cpu_user.unwrap_or(0.0), status.cpu_system.unwrap_or(0.0), status.cpu_idle.unwrap_or(0.0), status.cpu_nice.unwrap_or(0.0),
    now),
         NO_PARAMS) {
        return Res::error(Some("插入失败".to_string()));
    }

    if let Err(_e) = db.conn.execute(&format!("insert into memory_info (client_id,memory_free,memory_total,created_at) values ({}, {}, {}, '{}')", client_id,
    status.memory_free.unwrap_or(0), status.memory_total.unwrap_or(0),
    now), 
    NO_PARAMS) {
        return Res::error(Some("插入失败".to_string()));
    }

    for row in status.network_stats.iter() {
        if let Err(_e) = db.conn.execute(&format!("insert into network_stats_info(client_id, if_name, rx_bytes, tx_bytes, rx_packets, tx_packets, rx_errors, tx_errors, created_at) values ({}, '{}', {}, {}, {}, {}, {}, {}, '{}')", client_id, row.if_name, row.rx_bytes, row.tx_bytes, row.rx_packets, row.tx_packets, row.rx_errors, row.tx_errors, now), 
        NO_PARAMS) {
            return Res::error(Some("插入失败".to_string()));
        }
    }

    let mut sql = format!("update client set uptime={},boot_time='{}',cpu_temp={}", status.uptime.unwrap_or(0), &(status.boot_time.unwrap_or("")), status.cpu_temp.unwrap_or(0.0));
    if let Some(i) = status.system_version.clone() {
        sql = format!("{},system_version='{}'", sql, i);
    }
    if let Some(i) = status.package_manager.clone() {
        sql = format!("{},package_manager_update_count='{}'", sql, i);
    } 
    if let Some(i) = status.disk_avail.clone() {
        sql = format!("{},disk_avail={},disk_total={}", sql, i, status.disk_total.clone().unwrap());
    } 
    sql = format!("{} where id={} ", sql, client_id);

    if let Err(_e) = db.conn.execute(&sql, 
        NO_PARAMS
    ) {
        return Res::error(Some("插入失败".to_string()));
    }

    Res::ok(None, None)
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
fn do_login(params: Form<LoginParams>, mut cookies: Cookies) -> Json<Res::<Vec<String>>>{
    match model::check_login(&params.username, &params.password) {
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

// Client
#[get("/statistics")]
fn statistics(_admin: Admin) -> Template{
    Template::render("statistics", "")
}

#[post("/get_statistics")]
fn get_statistics(_admin: Admin) -> Json<Vec<model::StatisticsRow>>{
    if let Ok(ret) = model::get_client_statistics() {
        Json(ret)     
    } else {
        Json(vec!())
    }
}

#[derive(FromForm, Debug)]
struct DeleteClientParams {
    client_id: u32,
}

#[post("/delete_client", data="<params>")]
fn delete_client(_admin: Admin, params:Form<DeleteClientParams>) -> Json<Res::<Vec<String>>> 
{
    match model::delete_client(params.client_id) {
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
fn add_client(_admin: Admin, params:Form<AddClientParams>) -> Json<Res::<Vec<String>>> 
{
    match model::add_client(&params.name, &params.client_ip, 
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
    client_id: u32,
    name: String,
    client_ip: String,
    is_enable: u32,  
    ssh_address: Option<String>,
    ssh_username: Option<String>,
    ssh_password: Option<String>,
}

#[post("/edit_client", data="<params>")]
fn edit_client(_admin: Admin, params:Form<EditClientParams>) -> Json<Res::<Vec<String>>> 
{
    match model::edit_client(params.client_id, &params.name, &params.client_ip, params.is_enable, 
        &params.ssh_address.clone().unwrap_or("".to_string()), &params.ssh_username.clone().unwrap_or("".to_string()), &params.ssh_password.clone().unwrap_or("".to_string())) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

// Memory
#[post("/get_memory_chart")]
fn get_memory_chart(_admin: Admin) -> Json<Vec<model::MemoryChartLine>>{
    if let Ok(ret) = model::get_memory_chart() {
        Json(ret)     
    } else {
        Json(vec!())
    }
}

// Cpu
#[post("/get_cpu_chart")]
fn get_cpu_chart(_admin: Admin) -> Json<Vec<model::CpuChartLine>>{
    if let Ok(ret) = model::get_cpu_chart() {
        Json(ret)     
    } else {
        Json(vec!())
    }
}

// Net
#[derive(Deserialize, Debug)]
struct ByteChartParams {
    direction: u8,
    duration: String,
}

#[post("/get_byte_chart", data="<params>")]
fn get_byte_chart(params: Json<ByteChartParams>, _admin: Admin) -> Json<Vec<model::ByteChartLine>>{
    let duration = match model::str_to_chart_duration(&params.duration) {
        Ok(d) => d,
        Err(_e) => {
            return Json(vec!());
        }
    };
    if let Ok(ret) = model::get_byte_chart(params.direction, duration) {
        Json(ret)     
    } else {
        Json(vec!())
    }
}

// Task
#[derive(FromForm, Debug)]
struct TasksParams {
    client_id: u32,
}

#[post("/tasks", data="<params>")]
fn tasks(_admin: Admin, params:Form<TasksParams>) -> Json<Vec<model::TaskRow>>
{
    if let Ok(ret) = model::get_tasks(params.client_id) {
        Json(ret)     
    } else {
        Json(vec!())
    }
}

#[derive(FromForm, Debug)]
struct CancelTaskParams {
    task_id: u32,
}

#[post("/cancel_task", data="<params>")]
fn cancel_task(_admin: Admin, params:Form<CancelTaskParams>) -> Json<Res::<Vec<String>>> 
{
    match model::cancel_task(params.task_id) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

#[post("/get_task")]
fn get_task(client:Client) -> Json<Res::<Vec<String>>>{
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

#[derive(FromForm, Debug)]
struct OprateParams {
    client_id: u32,
    operation: String,
}

#[post("/operate", data="<params>")]
fn operate(_admin: Admin, params:Form<OprateParams>) -> Json<Res::<Vec<String>>> {
    let operation = params.operation.clone();
    match model::set_task(params.client_id, operation) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

#[derive(Deserialize, Debug)]
struct TaskParams {
    client_id: u32,
    task_type: String,
}

#[post("/set_task", data="<task>")]
fn set_task(task: Json<TaskParams>) -> Json<Res::<Vec<String>>>{
    let task_type = task.task_type.clone();
    match model::set_task(task.client_id, task_type) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

// Client apply
#[post("/client_applys")]
fn client_applys(_admin: Admin) -> Json<Vec<model::ClientApplyRow>>
{
    if let Ok(ret) = model::get_client_applys() {
        Json(ret)     
    } else {
        Json(vec!())
    }
}

#[derive(FromForm, Debug)]
struct ApplyOperationParam {
    id: u32,
}

#[post("/pass_apply", data="<params>")]
fn pass_apply(_admin: Admin, params:Form<ApplyOperationParam>) -> Json<Res::<Vec<String>>> 
{
    match model::pass_apply(params.id) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

#[post("/reject_apply", data="<params>")]
fn reject_apply(_admin: Admin, params:Form<ApplyOperationParam>) -> Json<Res::<Vec<String>>> 
{
    match model::reject_apply(params.id) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

// Terminal
#[derive(FromForm, Debug)]
struct ConnectSshClientParams {
    client_id: u32,
}

#[post("/connect_ssh_client", data="<params>")]
fn connect_ssh_client(_admin: Admin, params:Form<ConnectSshClientParams>, session: State<SshSession>) -> Json<Res::<Vec<String>>>
{
    let client;
    match model::get_client(params.client_id) {
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
    client_id: u32,
    command: String,
}

#[post("/run_ssh_command", data="<params>")]
fn run_ssh_command(_admin: Admin, params:Form<SshCommandParams>, session: State<SshSession>) -> Json<Res::<Vec<String>>>
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
        channel.exec(&format!("/bin/bash -c \"{}\"", params.command)).unwrap();
        let mut s = String::new();
        channel.read_to_string(&mut s).unwrap();

        let mut channel = session.channel_session().unwrap();
        channel.exec("pwd -P").unwrap();
        let mut cwd = String::new();
        channel.read_to_string(&mut cwd).unwrap();
        
        let separator = "--separator--";
        return Res::ok(Some(format!("{}{}{}", s, separator, cwd)), None);
    } else {
        return Res::error(Some("查询错误".to_string()));
    }
}

struct SshSession {
    client_id: Mutex<Option<u32>>,
    session: Mutex<Option<Session>>,
}

// Setting 
#[post("/get_setting")]
fn get_setting(_admin: Admin) -> Json<model::SettingRow>
{
    if let Ok(ret) = model::get_setting() {
        Json(ret)     
    } else {
        Json(model::SettingRow {
            pihole_server: "".to_string(),
            pihole_web_password: "".to_string(),
            es_server: "".to_string(),
            k8s_server: "".to_string(),
        })
    }
}

#[derive(FromForm, Debug)]
struct SaveSettingParam {
    pihole_server: String,
    pihole_web_password: String,
    es_server: String,
    k8s_server: String,
}

#[post("/save_setting", data="<params>")]
fn save_setting(_admin: Admin, params:Form<SaveSettingParam>) -> Json<Res::<Vec<String>>> 
{
    match model::save_setting(&params.pihole_server, &params.pihole_web_password, &params.es_server, &params.k8s_server) {
        Ok(_d) => {
            return Res::ok(None, None);
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
}

// Pilehole 
#[post("/get_pihole_statistics")]
fn get_pihole_statistics(_admin: Admin) -> Json<Res::<model::PiholeData>>
{
    match model::get_pihole_statistics() {
        Ok(d) => {
            return Res::ok(None, Some(d));
        },
        Err(e) => {
            return Res::error(Some(e.to_string())); 
        }
    } 
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
    .mount("/", routes![
         check_online, login, do_login,
         get_task, set_status, operate, tasks, cancel_task, set_task, 
         statistics, get_statistics, index, delete_client, edit_client, add_client, 
         get_memory_chart,
         get_cpu_chart,
         get_byte_chart,
         connect_ssh_client,run_ssh_command,
         client_applys,pass_apply,reject_apply,
         get_setting, save_setting,
         get_pihole_statistics,
     ])
    .attach(Template::fairing())
    .manage(ssh_client)
    .launch();
}
