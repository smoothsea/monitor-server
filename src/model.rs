use std::error::Error;
use md5;
use rusqlite::{NO_PARAMS};
use serde::{Serialize};
use chrono::{Local};

use crate::db::Db;

pub fn check_login(username: &str, password: &str) -> Result<i64, Box<dyn Error>> 
{
    let pass = format!("{:x}", md5::compute(password));
    if let Ok(db) = Db::get_db() {
        match db.conn.query_row::<i64,_,_>("select id from admin where username=?1 and password=?2", &[username, &pass], |row| {
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

#[derive(Serialize,Debug)]
pub struct StatisticsRow
{
    id: f64,
    client_ip: Option<String>,
    name: Option<String>,
    is_online: u32,
    last_online_time: Option<String>,
    is_enable: u32,
    created_at: Option<String>,
    uptime: Option<f64>,
    boot_time: Option<String>,
    cpu_user: Option<f64>,
    cpu_system: Option<f64>,
    cpu_nice: Option<f64>,
    cpu_idle: Option<f64>,
    memory_free: Option<f64>,
    memory_total: Option<f64>,
}

pub fn get_client_statistics() -> Result<Vec<StatisticsRow>, Box<dyn Error>>
{
    if let Ok(db) = Db::get_db() {
        let sql = "select client.id,client.client_ip,client.name,client.is_online,client.last_online_time,client.is_enable,client.created_at,client.uptime,client.boot_time,
            cpu.cpu_user,cpu.cpu_system,cpu.cpu_nice,cpu.cpu_idle,memory.memory_free,memory.memory_total 
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

#[derive(Serialize,Debug)]
pub struct TaskRow
{
    id: f64,
    is_valid: i8,
    task_type: String,
    cancled_at: Option<String>,
    pulled_at: Option<String>,
    created_at: Option<String>,
    client_id: f64,
}

pub fn get_tasks(client_id: u64) -> Result<Vec<TaskRow>, Box<dyn Error>>
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

pub fn set_task(client_id: u64, task: String) -> Result<(), Box<dyn Error>>
{
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(db) = Db::get_db() {
        if let Err(_e) = db.conn.query_row::<i32, _, _>(&format!("select id from client where id={} and is_enable=1 and is_online=1", client_id), NO_PARAMS, |row| {
            row.get(0)
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

pub fn delete_client(client_id: u64) -> Result<(), Box<dyn Error>>
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

pub fn edit_client(client_id: u64, name: &str, client_ip: &str, is_enable: u32) -> Result<(), Box<dyn Error>>
{
    if let Ok(db) = Db::get_db() {
        if let Ok(_d) = db.conn.query_row::<i32, _, _>(&format!("select id from client where id!={} and client_ip=?1", client_id), &[&client_ip], |row| {
            row.get(0)
        }) {
            Err("该用户ip已使用")?;
        }

        if let Err(_e) = db.conn.execute(&format!("update client set name=?1,client_ip=?2,is_enable={} where id={}", is_enable, client_id), &[&name, &client_ip]) {
            Err("修改失败")?;
        }
        Ok(())
    } else {
        Err("数据库连接错误")?
    }
}

pub fn add_client(name: &str, client_ip: &str) -> Result<(), Box<dyn Error>>
{
    if let Ok(db) = Db::get_db() {
        if let Ok(_d) = db.conn.query_row::<i32, _, _>("select id from client where client_ip=?1", &[&client_ip], |row| {
            row.get(0)
        }) {
            Err("该ip已使用")?;
        }
        
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        if let Err(_e) = db.conn.execute(&format!("insert into client (name,client_ip,is_enable,created_at) values(?1,?2,{},'{}')", 1, now), &[&name, &client_ip]) {
            println!("{:?}", _e);
            Err("添加失败")?;
        }
        Ok(())
    } else {
        Err("数据库连接错误")?
    }
}

pub fn cancel_task(task_id: u64) -> Result<(), Box<dyn Error>>
{
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(db) = Db::get_db() {
        if let Err(_e) = db.conn.query_row::<i32, _, _>(&format!("select id from task where id={} and is_valid=1", task_id), NO_PARAMS, |row| {
            row.get(0)
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