use md5;
use regex::Regex;
use crate::db::Db;
use reqwest::header;
use serde::{Serialize, Deserialize};
use k8s_openapi::http;
use std::error::Error;
use rusqlite::NO_PARAMS;
use core::ops::{Sub, Add};
use chrono::{Local, Duration};
use std::collections::HashMap;
use chrono::naive::NaiveDateTime;
use k8s_openapi::api::core::v1 as api;
use ssh2::DisconnectCode::ProtocolVersionNotSupported;

// clean statistics data
pub fn clean_data(save_days: i64) -> Result<(), Box<dyn Error>> {
    let duration = Duration::days(save_days);
    let time = Local::now().sub(duration).format("%Y-%m-%d %H:%M:%S").to_string();

    let tables = ["cpu_info", "memory_info", "network_stats_info"];
    if let Ok(db) = Db::get_db() {
        for table in tables {
            if let Err(_e) = db.conn.execute(&format!("delete from {} where created_at<?1", table), &[&time]) {
                Err("清理失败")?;
            }
        }
    } else {
        Err("数据库连接错误")?;
    }    

    Ok(())
}

pub fn check_login(username: &str, password: &str) -> Result<u32, Box<dyn Error>> 
{
    let pass = format!("{:x}", md5::compute(password));
    if let Ok(db) = Db::get_db() {
        match db.conn.query_row::<u32,_,_>("select id from admin where username=?1 and password=?2", &[username, &pass], |row| {
            row.get(0)
        }) {
            Ok(ret) => {
                return Ok(ret);
            },
            Err(_e) => {
               Err("帐号密码错误")?;
            }
        }
    } else {
        Err("数据库连接错误")?;
    }    

    return Err("")?;
}

#[derive(Debug)]
pub enum ChartDuration
{
    Minutes(u8), 
    Hours(u8), 
    Days(u8), 
}

pub fn str_to_chart_duration(s: &str) -> Result<ChartDuration, Box<dyn Error>>
{
    let re = Regex::new(r"^(?P<num>\d+)(?P<type>[ihd])$")?;
    if let Some(r) = re.captures(s) {
        let d = match &r["type"] {
            "i" => ChartDuration::Minutes(r["num"].parse()?),
            "h" => ChartDuration::Hours(r["num"].parse()?),
            "d" => ChartDuration::Days(r["num"].parse()?),
            _ => {
              return Err("格式错误")?;
            }
        };
        return Ok(d);
    } else {
        Err("格式错误")?
    }
}

// Memory
#[derive(Serialize,Debug)]
pub struct MemoryChartLine
{
    name: String,
    time: String,
    memory_total: f64,
    memory_free: f64,
}

pub fn get_memory_chart() -> Result<Vec<MemoryChartLine>, Box<dyn Error>>
{
    let duration = Duration::minutes(3);
    let time = Local::now()
        .sub(duration)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    if let Ok(db) = Db::get_db() {
        let sql = "select m.client_id,m.memory_free,m.memory_total,m.created_at,c.name from memory_info as m
                    join client as c on m.client_id=c.id where m.created_at>?1";
        match db.conn.prepare(sql) {
            Ok(mut smtm) => {
                if let Ok(mut ret) = smtm.query(&[&time]) {
                    let mut data:Vec<MemoryChartLine> = vec!();
                    while let Some(row) = ret.next().unwrap() {
                        let name = row.get(4)?;
                        let time = row.get(3)?;
                        let memory_free = row.get(1)?;
                        let memory_total = row.get(2)?;
                        
                        let line = MemoryChartLine{
                            name,
                            time,
                            memory_total,
                            memory_free,
                        };
                        data.push(line)
                    }

                    return Ok(data);
                }
            },
            Err(_e) => {
                Err("查询错误")?;
            }
        }
    } else {
        Err("数据库连接错误")?;
    }    

    return Err("")?;
}

// Cpu
#[derive(Serialize,Debug)]
pub struct CpuChartLine
{
    name: String,
    time: String,
    cpu_system: f64,
    cpu_user: f64,
}

pub fn get_cpu_chart() -> Result<Vec<CpuChartLine>, Box<dyn Error>>
{
    let duration = Duration::minutes(3);
    let time = Local::now()
        .sub(duration)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    if let Ok(db) = Db::get_db() {
        let sql = "select m.client_id,m.cpu_user,m.cpu_system,m.created_at,c.name from cpu_info as m
                    join client as c on m.client_id=c.id where m.created_at>?1";
        match db.conn.prepare(sql) {
            Ok(mut smtm) => {
                if let Ok(mut ret) = smtm.query(&[&time]) {
                    let mut data:Vec<CpuChartLine> = vec!();
                    while let Some(row) = ret.next().unwrap() {
                        let name = row.get(4)?;
                        let time = row.get(3)?;
                        let cpu_user = row.get(1)?;
                        let cpu_system = row.get(2)?;
                        
                        let line = CpuChartLine{
                            name,
                            time,
                            cpu_system,
                            cpu_user,
                        };
                        data.push(line)
                    }

                    return Ok(data);
                }
            },
            Err(_e) => {
                Err("查询错误")?;
            }
        }
    } else {
        Err("数据库连接错误")?;
    }    

    return Err("")?;
}

// Net
#[derive(Serialize,Debug)]
pub struct ByteChartLine
{
    name: String,
    time: String,
    byte: i64,
}

pub fn get_byte_chart(direction: u8, duration: ChartDuration) -> Result<Vec<ByteChartLine>, Box<dyn Error>>
{
    let byte_filed = match direction {
        0 => "rx_bytes",
        1 => "tx_bytes",
        _ => "rx_bytes",
    };
    let d = match duration {
        ChartDuration::Minutes(i) => Duration::minutes(i as i64), 
        ChartDuration::Hours(i) => Duration::hours(i as i64), 
        ChartDuration::Days(i) => Duration::days(i as i64), 
    };
    let time = Local::now()
        .sub(d)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    if let Ok(db) = Db::get_db() {
        let sql = &format!(r"
                    select m.client_id,sum(m.{}) as bytes,m.created_at,c.name from network_stats_info as m
                    join client as c on m.client_id=c.id 
                    where m.created_at>?1 
                    group by m.client_id,m.created_at
            ", byte_filed);
        match db.conn.prepare(sql) {
            Ok(mut smtm) => {
                if let Ok(mut ret) = smtm.query(&[&time]) {
                    let mut data:Vec<ByteChartLine> = vec!();
                    let mut client_all_flow_record:HashMap<String, (i64, String)> = HashMap::new();
                    while let Some(row) = ret.next().unwrap() {
                        let byte: i64 = row.get(1)?;
                        let name: String = row.get(3)?;
                        let time: String = row.get(2)?;

                        if client_all_flow_record.contains_key(&name) {
                            let (last_bytes, last_time) = client_all_flow_record.get(&name).unwrap();
                            let byte_diff = byte - last_bytes;
                            let time_diff = NaiveDateTime::parse_from_str(&time, "%Y-%m-%d %H:%M:%S")?.timestamp()
                                - NaiveDateTime::parse_from_str(&last_time, "%Y-%m-%d %H:%M:%S")?.timestamp();
                            let mut byte_per_secend = 0;
                            if time_diff != 0 {
                                byte_per_secend = byte_diff/time_diff as i64;
                            }
                            let line = ByteChartLine{
                                name: name.clone(),
                                time: time.clone(),
                                byte: byte_per_secend,
                            };
                            data.push(line);
                            if let Some(x) = client_all_flow_record.get_mut(&name) {
                                *x = (byte, time);
                            } 
                        } else {
                            client_all_flow_record.insert(name, (byte, time));
                        }
                    }

                    return Ok(data);
                }
            },
            Err(_e) => {
                Err("查询错误")?;
            }
        }
    } else {
        Err("数据库连接错误")?;
    }    

    return Err("")?;
}

// Client
#[derive(Serialize,Debug)]
pub struct Client
{
    pub id: u32,
    pub ssh_address: Option<String>,
    pub ssh_username: Option<String>,
    pub ssh_password: Option<String>,
}

pub fn delete_client(client_id: u32) -> Result<(), Box<dyn Error>>
{
    if let Ok(db) = Db::get_db() {
        if let Err(_e) = db.conn.execute(&format!("delete from client where id={}", client_id), NO_PARAMS) {
            Err("删除失败")?;
        }
        Ok(())
    } else {
        Err("数据库连接错误")?
    }
}

pub fn get_client(client_id: u32) -> Result<Client, Box<dyn Error>>
{
    if let Ok(db) = Db::get_db() {
        if let Ok(ret) = db.conn.query_row(&format!("select id,ssh_address,ssh_username,ssh_password from client where id={}", client_id), NO_PARAMS, |row| {
            let client = Client{
                id: row.get(0)?,
                ssh_address: row.get(1)?,
                ssh_username: row.get(2)?,
                ssh_password: row.get(3)?,
            };
            return Ok(client);
        }) {
            Ok(ret)
        } else {
            Err("用户信息错误")?
        }
    } else {
        Err("数据库连接错误")?
    }
}

pub fn edit_client(client_id: u32, name: &str, client_ip: &str, is_enable: u32, ssh_address: &str, ssh_username: &str, ssh_password: &str) -> Result<(), Box<dyn Error>>
{
    if let Ok(db) = Db::get_db() {
        if let Ok(_d) = db.conn.query_row::<(), _, _>(&format!("select id from client where id!={} and client_ip=?1", client_id), &[&client_ip], |_row| {
            Ok(())
        }) {
            Err("该用户ip已使用")?;
        }

        if let Err(_e) = db.conn.execute(&format!("update client set name=?1,client_ip=?2,ssh_address=?3,ssh_username=?4,ssh_password=?5,is_enable={} where id={}",
             is_enable, client_id), &[&name, &client_ip, &ssh_address, &ssh_username, &ssh_password]) {
            Err("修改失败")?;
        }
        Ok(())
    } else {
        Err("数据库连接错误")?
    }
}

pub fn add_client(name: &str, client_ip: &str, ssh_address: &str, ssh_username: &str, ssh_password: &str) -> Result<(), Box<dyn Error>>
{
    if let Ok(db) = Db::get_db() {
        if let Ok(_d) = db.conn.query_row::<(), _, _>("select id from client where client_ip=?1", &[&client_ip], |_row| {
            Ok(())
        }) {
            Err("该ip已使用")?;
        }
        
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        if let Err(_e) = db.conn.execute(&format!("insert into client (name,client_ip,ssh_address,ssh_username,ssh_password,is_enable,created_at) values(?1,?2,?3,?4,?5,{},'{}')", 1, now),
         &[&name, &client_ip, &ssh_address, &ssh_username, &ssh_password]) {
            println!("{:?}", _e);
            Err("添加失败")?;
        }
        Ok(())
    } else {
        Err("数据库连接错误")?
    }
}

#[derive(Serialize,Debug)]
pub struct StatisticsRow
{
    id: u32,
    client_ip: Option<String>,
    name: Option<String>,
    is_online: u8,
    last_online_time: Option<String>,
    is_enable: u8,
    created_at: Option<String>,
    uptime: Option<f64>,
    boot_time: Option<String>,
    cpu_user: Option<f64>,
    cpu_system: Option<f64>,
    cpu_nice: Option<f64>,
    cpu_idle: Option<f64>,
    memory_free: Option<f64>,
    memory_total: Option<f64>,
    system_version: Option<String>,
    package_manager_update_count: u32,
    ssh_address: Option<String>,
    ssh_username: Option<String>,
    ssh_password: Option<String>,
    cpu_temp: Option<f64>,
    disk_avail: Option<i64>,
    disk_total: Option<i64>,
}

pub fn get_client_statistics() -> Result<Vec<StatisticsRow>, Box<dyn Error>>
{
    //TODO return encrypted ssh_password
    if let Ok(db) = Db::get_db() {
        let sql = "select client.id,client.client_ip,client.name,client.is_online,client.last_online_time,client.is_enable,client.created_at,client.uptime,client.boot_time,
            cpu.cpu_user,cpu.cpu_system,cpu.cpu_nice,cpu.cpu_idle,memory.memory_free,memory.memory_total,
            client.system_version,client.package_manager_update_count,
            ssh_address,ssh_username,ssh_password,cpu_temp,
            disk_avail,disk_total
            from client
            left join (select * from cpu_info as info inner join (select max(id) as mid from cpu_info group by client_id) as least_info on info.id=least_info.mid) as cpu on cpu.client_id=client.id
            left join (select * from memory_info as info inner join (select max(id) as mid from memory_info group by client_id) as least_info on info.id=least_info.mid) as memory on memory.client_id=client.id
        ";
        match db.conn.prepare(sql) {
            Ok(mut smtm) => {
                if let Ok(mut ret) = smtm.query(NO_PARAMS) {
                    let mut data:Vec<StatisticsRow> = vec!();
                    while let Some(row) = ret.next().unwrap() {
                        let item = StatisticsRow {
                            id: row.get(0)?,
                            client_ip: row.get(1)?,
                            name: row.get(2)?,
                            is_online: row.get(3)?,
                            last_online_time: row.get(4)?,
                            is_enable: row.get(5)?,
                            created_at: row.get(6)?,
                            uptime: row.get(7)?,
                            boot_time: row.get(8)?,
                            cpu_user: row.get(9)?,
                            cpu_system: row.get(10)?,
                            cpu_nice: row.get(11)?,
                            cpu_idle: row.get(12)?,
                            memory_free: row.get(13)?,
                            memory_total: row.get(14)?,
                            system_version: row.get(15)?,
                            package_manager_update_count: row.get(16).unwrap_or(0),
                            ssh_address: row.get(17)?,
                            ssh_username: row.get(18)?,
                            ssh_password: row.get(19)?,
                            cpu_temp: row.get(20)?,
                            disk_avail: row.get(21)?,
                            disk_total: row.get(22)?,
                        };
                        data.push(item);
                    }
                    return Ok(data);
                }
            },
            Err(_e) => {
                Err("查询错误")?;
            }
        }
    } else {
        Err("数据库连接错误")?;
    }    

    return Err("")?;
}

//  Task
#[derive(Serialize,Debug)]
pub struct TaskRow
{
    id: u32,
    is_valid: u8,
    task_type: String,
    cancled_at: Option<String>,
    pulled_at: Option<String>,
    created_at: Option<String>,
    client_id: u32,
}

pub fn get_tasks(client_id: u32) -> Result<Vec<TaskRow>, Box<dyn Error>>
{
    if let Ok(db) = Db::get_db() {
        let sql = format!("select id,is_valid,task_type,cancled_at,pulled_at,created_at,client_id from task where client_id={} order by created_at desc limit 10", client_id);
        match db.conn.prepare(&sql) {
            Ok(mut smtm) => {
                if let Ok(mut ret) = smtm.query(NO_PARAMS) {
                    let mut data:Vec<TaskRow> = vec!();
                    while let Some(row) = ret.next().unwrap() {
                        let item = TaskRow {
                            id: row.get(0)?,
                            is_valid: row.get(1)?,
                            task_type: row.get(2)?,
                            cancled_at: row.get(3)?,
                            pulled_at: row.get(4)?,
                            created_at: row.get(5)?,
                            client_id: row.get(6)?,
                        };
                        data.push(item);
                    }
                    return Ok(data);
                }
            },
            Err(_e) => {
                println!("{:?}", _e);
                Err("查询错误")?;
            }
        }
    } else {
        Err("数据库连接错误")?;
    }    

    return Err("")?;
}

pub fn set_task(client_id: u32, task: String) -> Result<(), Box<dyn Error>>
{
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(db) = Db::get_db() {
        if let Err(_e) = db.conn.query_row::<(), _, _>(&format!("select id from client where id={} and is_enable=1 and is_online=1", client_id), NO_PARAMS, |_row| {
            Ok(())
        }) {
            Err("客户不存在或者不在线")?;
        }

        if let Ok(_d) = db.conn.query_row::<i32, _, _>(&format!("select id from task where client_id={} and is_valid=1 and task_type='{}'", client_id, task), NO_PARAMS, |row| {
            row.get(0)
        }) {
            Err("该任务只能提交一次")?;
        }
    
        if let Err(_e) = db.conn.execute(&format!("insert into task (client_id,task_type,created_at) values ({}, ?1, ?2)", client_id), &[&task, &now]) {
            Err("插入失败")?;
        }
        Ok(())
    } else {
        Err("数据库连接错误")?
    }
}

pub fn cancel_task(task_id: u32) -> Result<(), Box<dyn Error>>
{
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(db) = Db::get_db() {
        if let Err(_e) = db.conn.query_row::<(), _, _>(&format!("select id from task where id={} and is_valid=1", task_id), NO_PARAMS, |_row| {
            Ok(())
        }) {
            Err("任务信息错误")?;
        }
    
        if let Err(_e) = db.conn.execute(&format!("update task set is_valid=0,cancled_at=?1 where id={}", task_id), &[&now]) {
            Err("取消失败")?;
        }
        Ok(())
    } else {
        Err("数据库连接错误")?
    }
}

// Client apply
const CLIENT_APPLY_STATUS_WAIT:u8 = 0;
const CLIENT_APPLY_STATUS_PASS:u8 = 1;
const CLIENT_APPLY_STATUS_REJECT:u8 = 2;
const CLIENT_APPLY_EXPIRE_HOURS:i64 = 24;
pub fn create_apply(machine_id: &str, client_ip: &str) -> Result<(), Box<dyn Error>>
{
    let duration = Duration::hours(CLIENT_APPLY_EXPIRE_HOURS);
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let apply_expire_date = Local::now().add(duration).format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(db) = Db::get_db() {
        if let Err(_e) = db.conn.query_row::<(), _, _>(&format!("select id from client_apply where (machine_id=?1 and client_ip='{}' and status in ({}, {})) or (machine_id=?1 and client_ip='{}' and status={} and created_at<=?2 )", client_ip, CLIENT_APPLY_STATUS_PASS, CLIENT_APPLY_STATUS_REJECT, client_ip, CLIENT_APPLY_STATUS_WAIT), &[machine_id, &apply_expire_date], |_row| {
            Ok(())
        }) {
            if let Err(_e) = db.conn.execute(&format!("insert into client_apply (machine_id, client_ip, status, created_at) values (?1, ?2, {}, ?3)", CLIENT_APPLY_STATUS_WAIT), &[machine_id, client_ip, &now]) {
                Err("申请失败")?;
            }
        }
    
        Ok(())
    } else {
        Err("数据库连接错误")?
    }
}

#[derive(Serialize,Debug)]
pub struct ClientApplyRow 
{
    id: u32,
    machine_id: String,
    client_ip: String,
    status: u8,
    created_at: Option<String>,
    updated_at: Option<String>,
}

pub fn get_client_applys() -> Result<Vec<ClientApplyRow>, Box<dyn Error>>
{
    if let Ok(db) = Db::get_db() {
        let sql = format!("select id,machine_id,client_ip,status,created_at,updated_at from client_apply order by created_at desc limit 10");
        match db.conn.prepare(&sql) {
            Ok(mut smtm) => {
                if let Ok(mut ret) = smtm.query(NO_PARAMS) {
                    let mut data:Vec<ClientApplyRow> = vec!();
                    while let Some(row) = ret.next().unwrap() {
                        let item = ClientApplyRow {
                            id: row.get(0)?,
                            machine_id: row.get(1)?,
                            client_ip: row.get(2)?,
                            status: row.get(3)?,
                            created_at: row.get(4)?,
                            updated_at: row.get(5)?,
                        };
                        data.push(item);
                    }
                    return Ok(data);
                }
            },
            Err(_e) => {
                println!("{:?}", _e);
                Err("查询错误")?;
            }
        }
    } else {
        Err("数据库连接错误")?;
    }    

    return Err("")?;
}

pub fn pass_apply(id: u32) -> Result<(), Box<dyn Error>>
{
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut apply = None;
    if let Ok(db) = Db::get_db() {
        if let Ok(row) = db.conn.query_row::<ClientApplyRow, _, _>(&format!("select id,machine_id,client_ip,status from client_apply where id={} and status={}", id, CLIENT_APPLY_STATUS_WAIT), NO_PARAMS, |row| {
            Ok(ClientApplyRow {
                id: row.get(0).unwrap_or(id),
                machine_id: row.get(1)?,
                client_ip: row.get(2)?,
                status: row.get(3).unwrap_or(1),
                created_at: None,
                updated_at: None,
            })
        }) {
            apply = Some(row);
        } else {
            Err("申请信息错误")?;
        }
    
        if let Some(a) = apply {
            add_client(&a.client_ip, &a.client_ip, &a.client_ip, "", "")?;
           
            if let Err(_e) = db.conn.execute(&format!("update client_apply set status={},updated_at=?1 where id={}", CLIENT_APPLY_STATUS_PASS, id), &[&now]) {
                Err("修改失败")?;
            }
        }
        Ok(())
    } else {
        Err("数据库连接错误")?
    }
}

pub fn reject_apply(id: u32) -> Result<(), Box<dyn Error>>
{
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(db) = Db::get_db() {
        if let Err(_e) = db.conn.query_row::<(), _, _>(&format!("select id from client_apply where id={} and status={}", id, CLIENT_APPLY_STATUS_WAIT), NO_PARAMS, |_row| {Ok(())}) {
            Err("申请信息错误")?;
        }
    
        if let Err(_e) = db.conn.execute(&format!("update client_apply set status={},updated_at=?1 where id={}", CLIENT_APPLY_STATUS_REJECT, id), &[&now]) {
            Err("操作失败")?;
        }
        Ok(())
    } else {
        Err("数据库连接错误")?
    }
}

//  Setting
#[derive(Serialize,Debug)]
pub struct SettingRow
{
    pub pihole_server: String,
    pub pihole_web_password: String,
    pub es_server: String,
    pub k8s_server: String,
    pub k8s_auth_token: String,
}

pub fn get_setting() -> Result<SettingRow, Box<dyn Error>>
{
    if let Ok(db) = Db::get_db() {
        let sql = format!("select pihole_server,pihole_web_password,es_server,k8s_server,k8s_auth_token from config limit 1");
        match db.conn.query_row(&sql, NO_PARAMS, |row| Ok(SettingRow{
            pihole_server: row.get(0)?,
            pihole_web_password: row.get(1)?,
            es_server: row.get(2)?,
            k8s_server: row.get(3)?,
            k8s_auth_token: row.get(4)?,
        })) {
            Ok(data) => {
                return Ok(data);
            },
            Err(_e) => {
                println!("{:?}", _e);
                Err("查询错误")?;
            }
        }
    } else {
        Err("数据库连接错误")?;
    }    

    return Err("")?;
}

pub fn save_setting(pihole_server: &str, pihole_web_password: &str, es_server: &str, k8s_server: &str, k8s_auth_token: &str) -> Result<(), Box<dyn Error>>
{
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(db) = Db::get_db() {
        let sql = format!("select id from config limit 1");
        match db.conn.query_row::<i64, _, _>(&sql, NO_PARAMS, |row| row.get(0)) {
            Ok(id) => {
                if let Err(_e) = db.conn.execute(&format!("update config set pihole_server=?1,es_server=?2,k8s_server=?3,updated_at=?4,pihole_web_password=?5,k8s_auth_token=?6 where id={}", id), &[pihole_server, es_server, k8s_server, &now, pihole_web_password, k8s_auth_token]) {
                    println!("{:?}", _e);
                    Err("保存失败")?;
                }
            },
            Err(_e) => {
                if let Err(_e) = db.conn.execute(&format!("insert into config (pihole_server, es_server, k8s_server, created_at, updated_at, pihole_web_password, k8s_auth_token) values (?1, ?2, ?3, ?4, ?5, ?6, ?7)"), &[pihole_server, es_server, k8s_server, &now, &now, pihole_web_password, k8s_auth_token]) {
                    Err("保存失败")?;
                }
            }
        }
    } else {
        Err("数据库连接错误")?;
    }    

    return Ok(());
}

// Pihole
#[derive(Serialize,Deserialize,Debug)]
struct PiholeDomainRow {
    domain: String,
    count: u32,
}

#[derive(Serialize,Deserialize,Debug)]
struct PiholeSummaryRet {
    domains_being_blocked: u32,
    dns_queries_today: u32,
    ads_blocked_today: u32,
    unique_clients: u32,
}

#[derive(Serialize,Deserialize,Debug)]
struct PiholeTopListRet {
    top_queries: HashMap<String, u32>,
}

#[derive(Serialize,Debug)]
pub struct PiholeData {
    statistics: Option<PiholeSummaryRet>,
    domain_list: Option<PiholeTopListRet>,
}

pub fn get_pihole_statistics() -> Result<PiholeData, Box<dyn Error>> {
    let setting = get_setting()?;
    if &setting.pihole_server != "" {
        let statistics:Option<PiholeSummaryRet> = reqwest::blocking::get(format!("{}/admin/api.php", setting.pihole_server))?.json().ok();
        let domain_list:Option<PiholeTopListRet> = reqwest::blocking::get(format!("{}/admin/api.php?topItems=10&auth={}", setting.pihole_server, setting.pihole_web_password))?.json().ok();
        return Ok(PiholeData {
            statistics,
            domain_list,
        })
    }

    Err("未配置Pihole相关信息")?
}

#[derive(Serialize, Debug)]
pub struct K8sListData {
    list: serde_json::value::Value,
}

pub fn get_k8s_list(t: u8) -> Result<K8sListData, Box<dyn Error>> {
    println!("{}", t);
    let (mut request, _) = api::Pod::list("default", Default::default())?;
    if t == 1 {
        (request, _) = api::Service::list("default", Default::default())?;
    } else if t == 2 {
        (request, _) = api::Node::list(Default::default())?;
    }
    println!("{:?}", request);
    let list:serde_json::value::Value = request_k8s(request)?.json()?;
    Ok(K8sListData {
        list
    })
}

fn request_k8s(req: http::Request<Vec<u8>>) -> Result<reqwest::blocking::Response, Box<dyn Error>> {
    let setting = get_setting()?;

    if &setting.k8s_server != "" && &setting.k8s_auth_token != "" {
        let mut headers = header::HeaderMap::new();
        headers.insert("Authorization", header::HeaderValue::from_str(&format!("Bearer {}", &setting.k8s_auth_token))?);
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(true)
            .build()?;
        return Ok(client.get(&format!("{}{}", &setting.k8s_server, req.uri())).send()?);
    }

    Err("未配置K8s相关信息")?
}
